//! glTF model loader

use std::path::{Path, PathBuf};

use glam::{Quat, Vec3};
use gltf::animation::util::ReadOutputs;
use gltf::buffer::Data;
use gltf::image::Format;
use gltf::mesh::Mode;
use wgpu::{Device, Queue};

use crate::descriptor::{AlphaMode, MaterialDescriptor, MaterialType, ShaderDescriptor};
use crate::mesh::Mesh3D;
use crate::mesh::Vertex3D;
use crate::texture::{TextureError, TextureImage};

#[derive(thiserror::Error, Debug)]
pub enum GltfError {
    #[error("Failed to load glTF file '{path}': {source}")]
    Load { path: String, source: gltf::Error },
    #[error("Failed to read glTF buffers: {0}")]
    Buffer(String),
    #[error("Primitive mode {0:?} is not supported. Only triangles are supported.")]
    UnsupportedMode(Mode),
    #[error("Mesh has no positions")]
    MissingPositions,
    #[error("Failed to convert glTF image '{label}' into RGBA texture data: {source}")]
    TextureImage { label: String, source: TextureError },
}

/// Result of loading a glTF file.
pub struct GltfScene {
    /// External files that should invalidate this scene during hot reload.
    pub dependencies: Vec<PathBuf>,
    /// Loaded meshes with their names.
    pub meshes: Vec<(String, Mesh3D)>,
    /// Loaded material descriptors with their names.
    pub materials: Vec<(String, MaterialDescriptor)>,
    /// Loaded CPU-side images with their labels.
    pub images: Vec<(String, TextureImage)>,
    /// Imported transform animation clips with their labels.
    pub animations: Vec<(String, GltfAnimationClip)>,
    /// Material index for each loaded mesh, aligned with [`Self::meshes`].
    pub mesh_material_indices: Vec<Option<usize>>,
    /// Node hierarchy information for spawning entities.
    pub nodes: Vec<GltfNode>,
}

/// Imported glTF transform animation clip.
#[derive(Clone, Debug, PartialEq)]
pub struct GltfAnimationClip {
    /// Imported animation label.
    pub name: String,
    /// Clip duration in seconds.
    pub duration_seconds: f32,
    /// Transform animation channels in this clip.
    pub channels: Vec<GltfAnimationChannel>,
}

/// A single imported glTF animation channel targeting one node transform property.
#[derive(Clone, Debug, PartialEq)]
pub struct GltfAnimationChannel {
    /// Source glTF node index targeted by this channel.
    pub target_node: usize,
    /// Sampler interpolation mode.
    pub interpolation: GltfAnimationInterpolation,
    /// Keyframed transform property values.
    pub curve: GltfAnimationCurve,
}

/// Interpolation mode declared by a glTF animation sampler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GltfAnimationInterpolation {
    /// Linear vector interpolation or spherical rotation interpolation.
    Linear,
    /// Hold the previous keyframe value.
    Step,
    /// Cubic spline sampler. Oxide currently imports this as a declared mode.
    CubicSpline,
}

/// Keyframed transform property values for an imported animation channel.
#[derive(Clone, Debug, PartialEq)]
pub enum GltfAnimationCurve {
    /// Local translation keyframes.
    Translations(Vec<GltfVec3Keyframe>),
    /// Local rotation keyframes.
    Rotations(Vec<GltfQuatKeyframe>),
    /// Local scale keyframes.
    Scales(Vec<GltfVec3Keyframe>),
}

/// Imported vector keyframe for glTF translation or scale channels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GltfVec3Keyframe {
    /// Timestamp in seconds.
    pub time_seconds: f32,
    /// Keyframe value.
    pub value: Vec3,
}

/// Imported quaternion keyframe for glTF rotation channels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GltfQuatKeyframe {
    /// Timestamp in seconds.
    pub time_seconds: f32,
    /// Keyframe value.
    pub value: Quat,
}

/// Represents a node in the glTF hierarchy.
#[derive(Clone, Debug)]
pub struct GltfNode {
    /// Original glTF node index.
    pub node_index: usize,
    /// Name of the node (if available).
    pub name: Option<String>,
    /// Index of the mesh (if this node has a mesh).
    pub mesh_index: Option<usize>,
    /// Local transform: position.
    pub translation: Vec3,
    /// Local transform: rotation.
    pub rotation: Quat,
    /// Local transform: scale.
    pub scale: Vec3,
    /// Child nodes.
    pub children: Vec<GltfNode>,
}

impl Default for GltfNode {
    fn default() -> Self {
        Self {
            name: None,
            node_index: 0,
            mesh_index: None,
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            children: Vec::new(),
        }
    }
}

/// Loads a glTF file and extracts meshes.
pub fn load_gltf(
    device: &Device,
    queue: &Queue,
    path: impl AsRef<Path>,
) -> Result<GltfScene, GltfError> {
    let path = path.as_ref();
    let path_str = path.display().to_string();

    // Load the glTF document and buffers
    let (document, buffers, images) = gltf::import(path).map_err(|source| GltfError::Load {
        path: path_str.clone(),
        source,
    })?;

    let dependencies = gltf_document_dependencies(path, &document);
    let materials = extract_materials(&document);
    let images = extract_images(&document, images)?;
    let animations = extract_animations(&document, &buffers);

    // Extract meshes
    let mut meshes = Vec::new();
    let mut mesh_material_indices = Vec::new();
    for (mesh_idx, mesh) in document.meshes().enumerate() {
        for (prim_idx, primitive) in mesh.primitives().enumerate() {
            // Only support triangle mode
            if primitive.mode() != Mode::Triangles {
                return Err(GltfError::UnsupportedMode(primitive.mode()));
            }

            let mesh_name = format!("mesh_{}_prim{}", mesh_idx, prim_idx);
            mesh_material_indices.push(primitive.material().index());
            let loaded_mesh = load_primitive(device, queue, &primitive, &buffers, &mesh_name)?;
            meshes.push((mesh_name, loaded_mesh));
        }
    }

    // Extract node hierarchy
    let nodes = extract_nodes(&document, &meshes);

    Ok(GltfScene {
        dependencies,
        meshes,
        materials,
        images,
        animations,
        mesh_material_indices,
        nodes,
    })
}

fn gltf_document_dependencies(path: &Path, document: &gltf::Document) -> Vec<PathBuf> {
    let base = path.parent().map(PathBuf::from);
    let mut dependencies = Vec::new();

    for buffer in document.buffers() {
        if let gltf::buffer::Source::Uri(uri) = buffer.source() {
            if let Some(path) = resolve_gltf_uri(base.as_ref(), uri) {
                dependencies.push(path);
            }
        }
    }

    for image in document.images() {
        if let gltf::image::Source::Uri { uri, .. } = image.source() {
            if let Some(path) = resolve_gltf_uri(base.as_ref(), uri) {
                dependencies.push(path);
            }
        }
    }

    dependencies.sort();
    dependencies.dedup();
    dependencies
}

fn resolve_gltf_uri(base: Option<&PathBuf>, uri: &str) -> Option<PathBuf> {
    if uri.trim().is_empty() || uri.starts_with("data:") {
        return None;
    }

    let path = PathBuf::from(uri);
    Some(if path.is_absolute() {
        path
    } else if let Some(base) = base {
        base.join(path)
    } else {
        path
    })
}

fn extract_materials(document: &gltf::Document) -> Vec<(String, MaterialDescriptor)> {
    document
        .materials()
        .enumerate()
        .map(|(idx, material)| {
            let name = format!("material_{idx}");
            let pbr = material.pbr_metallic_roughness();
            let albedo_texture = pbr
                .base_color_texture()
                .map(|texture| format!("#image_{}", texture.texture().source().index()));
            let normal_texture = material
                .normal_texture()
                .map(|texture| format!("#image_{}", texture.texture().source().index()));
            let metallic_texture = pbr
                .metallic_roughness_texture()
                .map(|texture| format!("#image_{}_metallic", texture.texture().source().index()));
            let roughness_texture = pbr
                .metallic_roughness_texture()
                .map(|texture| format!("#image_{}_roughness", texture.texture().source().index()));
            let alpha_mode = match material.alpha_mode() {
                gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
                gltf::material::AlphaMode::Mask => AlphaMode::Mask,
                gltf::material::AlphaMode::Blend => AlphaMode::Blend,
            };
            let descriptor = MaterialDescriptor {
                name: name.clone(),
                material_type: MaterialType::Lit,
                shader: ShaderDescriptor::Builtin {
                    shader: "lit".to_string(),
                },
                fallback_shader: Some("lit".to_string()),
                base_color: pbr.base_color_factor(),
                metallic_factor: pbr.metallic_factor(),
                roughness_factor: pbr.roughness_factor(),
                emissive_color: material.emissive_factor(),
                alpha_mode,
                albedo_texture,
                normal_texture,
                metallic_texture,
                roughness_texture,
            };
            (name, descriptor)
        })
        .collect()
}

fn extract_images(
    document: &gltf::Document,
    images: Vec<gltf::image::Data>,
) -> Result<Vec<(String, TextureImage)>, GltfError> {
    let metallic_roughness_sources = gltf_metallic_roughness_image_sources(document);
    images
        .into_iter()
        .enumerate()
        .try_fold(Vec::new(), |mut images, (idx, image)| {
            let label = format!("image_{idx}");
            let rgba = gltf_image_to_rgba(&image);
            if metallic_roughness_sources.contains(&idx) {
                let metallic_label = format!("image_{idx}_metallic");
                let metallic_rgba = metallic_rgba_from_metallic_roughness_rgba(&rgba);
                let metallic = TextureImage::from_rgba(image.width, image.height, metallic_rgba)
                    .map_err(|source| GltfError::TextureImage {
                        label: metallic_label.clone(),
                        source,
                    })?;
                images.push((metallic_label, metallic));

                let roughness_label = format!("image_{idx}_roughness");
                let roughness_rgba = roughness_rgba_from_metallic_roughness_rgba(&rgba);
                let roughness = TextureImage::from_rgba(image.width, image.height, roughness_rgba)
                    .map_err(|source| GltfError::TextureImage {
                        label: roughness_label.clone(),
                        source,
                    })?;
                images.push((roughness_label, roughness));
            }
            let texture =
                TextureImage::from_rgba(image.width, image.height, rgba).map_err(|source| {
                    GltfError::TextureImage {
                        label: label.clone(),
                        source,
                    }
                })?;
            images.push((label, texture));
            Ok(images)
        })
}

fn extract_animations(
    document: &gltf::Document,
    buffers: &[Data],
) -> Vec<(String, GltfAnimationClip)> {
    document
        .animations()
        .enumerate()
        .filter_map(|(idx, animation)| {
            let name = format!("animation_{idx}");
            let mut channels = Vec::new();
            let mut duration_seconds = 0.0_f32;

            for channel in animation.channels() {
                let sampler = channel.sampler();
                let reader =
                    channel.reader(|buffer| buffers.get(buffer.index()).map(|data| &*data.0));
                let Some(inputs) = reader.read_inputs() else {
                    continue;
                };
                let times = inputs.collect::<Vec<_>>();
                let Some(outputs) = reader.read_outputs() else {
                    continue;
                };
                let interpolation = match sampler.interpolation() {
                    gltf::animation::Interpolation::Linear => GltfAnimationInterpolation::Linear,
                    gltf::animation::Interpolation::Step => GltfAnimationInterpolation::Step,
                    gltf::animation::Interpolation::CubicSpline => {
                        GltfAnimationInterpolation::CubicSpline
                    }
                };
                let target_node = channel.target().node().index();
                let curve = match outputs {
                    ReadOutputs::Translations(values) => {
                        let keyframes = times
                            .iter()
                            .copied()
                            .zip(values)
                            .map(|(time_seconds, value)| GltfVec3Keyframe {
                                time_seconds,
                                value: Vec3::from_array(value),
                            })
                            .collect::<Vec<_>>();
                        if keyframes.is_empty() {
                            continue;
                        }
                        duration_seconds =
                            duration_seconds.max(keyframes.last().unwrap().time_seconds);
                        GltfAnimationCurve::Translations(keyframes)
                    }
                    ReadOutputs::Rotations(values) => {
                        let keyframes = times
                            .iter()
                            .copied()
                            .zip(values.into_f32())
                            .map(|(time_seconds, value)| GltfQuatKeyframe {
                                time_seconds,
                                value: Quat::from_xyzw(value[0], value[1], value[2], value[3]),
                            })
                            .collect::<Vec<_>>();
                        if keyframes.is_empty() {
                            continue;
                        }
                        duration_seconds =
                            duration_seconds.max(keyframes.last().unwrap().time_seconds);
                        GltfAnimationCurve::Rotations(keyframes)
                    }
                    ReadOutputs::Scales(values) => {
                        let keyframes = times
                            .iter()
                            .copied()
                            .zip(values)
                            .map(|(time_seconds, value)| GltfVec3Keyframe {
                                time_seconds,
                                value: Vec3::from_array(value),
                            })
                            .collect::<Vec<_>>();
                        if keyframes.is_empty() {
                            continue;
                        }
                        duration_seconds =
                            duration_seconds.max(keyframes.last().unwrap().time_seconds);
                        GltfAnimationCurve::Scales(keyframes)
                    }
                    ReadOutputs::MorphTargetWeights(_) => continue,
                };

                channels.push(GltfAnimationChannel {
                    target_node,
                    interpolation,
                    curve,
                });
            }

            (!channels.is_empty()).then_some((
                name.clone(),
                GltfAnimationClip {
                    name,
                    duration_seconds,
                    channels,
                },
            ))
        })
        .collect()
}

fn gltf_metallic_roughness_image_sources(document: &gltf::Document) -> Vec<usize> {
    let mut sources: Vec<_> = document
        .materials()
        .filter_map(|material| {
            material
                .pbr_metallic_roughness()
                .metallic_roughness_texture()
                .map(|texture| texture.texture().source().index())
        })
        .collect();
    sources.sort_unstable();
    sources.dedup();
    sources
}

fn metallic_rgba_from_metallic_roughness_rgba(rgba: &[u8]) -> Vec<u8> {
    let mut metallic = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        let value = pixel[2];
        metallic.extend_from_slice(&[value, value, value, 255]);
    }
    metallic
}

fn roughness_rgba_from_metallic_roughness_rgba(rgba: &[u8]) -> Vec<u8> {
    let mut roughness = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        let value = pixel[1];
        roughness.extend_from_slice(&[value, value, value, 255]);
    }
    roughness
}

fn gltf_image_to_rgba(image: &gltf::image::Data) -> Vec<u8> {
    match image.format {
        Format::R8 => expand_u8_pixels(&image.pixels, 1),
        Format::R8G8 => expand_u8_pixels(&image.pixels, 2),
        Format::R8G8B8 => expand_u8_pixels(&image.pixels, 3),
        Format::R8G8B8A8 => image.pixels.clone(),
        Format::R16 => expand_u16_pixels(&image.pixels, 1),
        Format::R16G16 => expand_u16_pixels(&image.pixels, 2),
        Format::R16G16B16 => expand_u16_pixels(&image.pixels, 3),
        Format::R16G16B16A16 => expand_u16_pixels(&image.pixels, 4),
        Format::R32G32B32FLOAT => expand_f32_pixels(&image.pixels, 3),
        Format::R32G32B32A32FLOAT => expand_f32_pixels(&image.pixels, 4),
    }
}

fn expand_u8_pixels(pixels: &[u8], channels: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((pixels.len() / channels) * 4);
    for pixel in pixels.chunks_exact(channels) {
        rgba.push(pixel[0]);
        rgba.push(if channels > 1 { pixel[1] } else { pixel[0] });
        rgba.push(if channels > 2 { pixel[2] } else { pixel[0] });
        rgba.push(if channels > 3 { pixel[3] } else { 255 });
    }
    rgba
}

fn expand_u16_pixels(pixels: &[u8], channels: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((pixels.len() / (channels * 2)) * 4);
    for pixel in pixels.chunks_exact(channels * 2) {
        let channel = |index: usize| -> u8 {
            let offset = index * 2;
            if index < channels {
                let value = u16::from_le_bytes([pixel[offset], pixel[offset + 1]]);
                (value / 257) as u8
            } else if index == 3 {
                255
            } else {
                let value = u16::from_le_bytes([pixel[0], pixel[1]]);
                (value / 257) as u8
            }
        };
        rgba.push(channel(0));
        rgba.push(channel(1));
        rgba.push(channel(2));
        rgba.push(channel(3));
    }
    rgba
}

fn expand_f32_pixels(pixels: &[u8], channels: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((pixels.len() / (channels * 4)) * 4);
    for pixel in pixels.chunks_exact(channels * 4) {
        let channel = |index: usize| -> u8 {
            if index < channels {
                let offset = index * 4;
                let value = f32::from_le_bytes([
                    pixel[offset],
                    pixel[offset + 1],
                    pixel[offset + 2],
                    pixel[offset + 3],
                ]);
                (value.clamp(0.0, 1.0) * 255.0).round() as u8
            } else if index == 3 {
                255
            } else {
                let value = f32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]);
                (value.clamp(0.0, 1.0) * 255.0).round() as u8
            }
        };
        rgba.push(channel(0));
        rgba.push(channel(1));
        rgba.push(channel(2));
        rgba.push(channel(3));
    }
    rgba
}

/// Extracts the node hierarchy from a glTF document.
fn extract_nodes(document: &gltf::Document, meshes: &[(String, Mesh3D)]) -> Vec<GltfNode> {
    let scenes: Vec<_> = document.scenes().collect();
    let scene = scenes.first();

    match scene {
        Some(scene) => scene
            .nodes()
            .map(|node| convert_node(&node, meshes))
            .collect(),
        None => Vec::new(),
    }
}

/// Converts a glTF node to our GltfNode type.
fn convert_node(node: &gltf::Node, meshes: &[(String, Mesh3D)]) -> GltfNode {
    let (t, r, s) = node.transform().decomposed();
    let node_index = node.index();

    // Find mesh index if this node has a mesh
    let mesh_index = node.mesh().map(|mesh| {
        // Find the index in our meshes vector
        let mesh_idx = mesh.index();
        meshes
            .iter()
            .position(|(name, _)| name.starts_with(&format!("mesh_{}_", mesh_idx)))
            .unwrap_or(0)
    });

    GltfNode {
        node_index,
        name: Some(format!("node_{node_index}")),
        mesh_index,
        translation: Vec3::new(t[0], t[1], t[2]),
        rotation: Quat::from_xyzw(r[0], r[1], r[2], r[3]),
        scale: Vec3::new(s[0], s[1], s[2]),
        children: node
            .children()
            .map(|child| convert_node(&child, meshes))
            .collect(),
    }
}

/// Loads a single primitive as a Mesh3D.
fn load_primitive(
    device: &Device,
    _queue: &Queue,
    primitive: &gltf::Primitive,
    buffers: &[Data],
    name: &str,
) -> Result<Mesh3D, GltfError> {
    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

    // Read positions (required)
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or(GltfError::MissingPositions)?
        .collect();

    // Read normals (optional, default to up)
    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .map(|iter| iter.collect())
        .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

    // Read UVs (optional, default to 0,0)
    let uvs: Vec<[f32; 2]> = reader
        .read_tex_coords(0)
        .map(|tex| tex.into_f32().collect())
        .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

    // Build vertices
    let vertices: Vec<Vertex3D> = positions
        .iter()
        .zip(normals.iter())
        .zip(uvs.iter())
        .map(|((&pos, &normal), &uv)| Vertex3D {
            position: pos,
            normal,
            uv,
        })
        .collect();

    // Read indices
    let indices: Vec<u16> = reader
        .read_indices()
        .map(|indices| indices.into_u32().map(|i| i as u16).collect())
        .unwrap_or_else(|| (0..vertices.len() as u16).collect());

    // Create the mesh
    Ok(Mesh3D::create(device, &vertices, &indices, Some(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gltf_rgb8_image_expands_to_rgba() {
        let image = gltf::image::Data {
            pixels: vec![255, 0, 128],
            format: Format::R8G8B8,
            width: 1,
            height: 1,
        };

        assert_eq!(gltf_image_to_rgba(&image), vec![255, 0, 128, 255]);
    }

    #[test]
    fn gltf_rgba16_image_downsamples_to_rgba8() {
        let image = gltf::image::Data {
            pixels: vec![0xff, 0xff, 0x00, 0x00, 0x80, 0x80, 0xff, 0xff],
            format: Format::R16G16B16A16,
            width: 1,
            height: 1,
        };

        assert_eq!(gltf_image_to_rgba(&image), vec![255, 0, 128, 255]);
    }

    #[test]
    fn gltf_document_dependencies_resolve_external_uris() {
        let raw = br#"{
            "asset": { "version": "2.0" },
            "buffers": [
                { "uri": "mesh.bin", "byteLength": 4 },
                { "uri": "data:application/octet-stream;base64,AAAA", "byteLength": 4 }
            ],
            "images": [
                { "uri": "textures/albedo.png" }
            ]
        }"#;
        let gltf = gltf::Gltf::from_slice(raw).unwrap();

        assert_eq!(
            gltf_document_dependencies(Path::new("assets/models/level.gltf"), &gltf.document),
            vec![
                PathBuf::from("assets/models/mesh.bin"),
                PathBuf::from("assets/models/textures/albedo.png"),
            ]
        );
    }

    #[test]
    fn gltf_animations_extract_transform_channels() {
        let raw = br#"{
            "asset": { "version": "2.0" },
            "buffers": [{ "byteLength": 32 }],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 8 },
                { "buffer": 0, "byteOffset": 8, "byteLength": 24 }
            ],
            "accessors": [
                { "bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0.0], "max": [1.0] },
                { "bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC3" }
            ],
            "nodes": [{}],
            "animations": [{
                "samplers": [{ "input": 0, "output": 1, "interpolation": "LINEAR" }],
                "channels": [{ "sampler": 0, "target": { "node": 0, "path": "translation" } }]
            }]
        }"#;
        let gltf = gltf::Gltf::from_slice(raw).unwrap();
        let mut bytes = Vec::new();
        for value in [0.0_f32, 1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        let clips = extract_animations(&gltf.document, &[Data(bytes)]);

        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].0, "animation_0");
        assert_eq!(clips[0].1.duration_seconds, 1.0);
        assert_eq!(clips[0].1.channels.len(), 1);
        let GltfAnimationCurve::Translations(keyframes) = &clips[0].1.channels[0].curve else {
            panic!("expected translation channel");
        };
        assert_eq!(clips[0].1.channels[0].target_node, 0);
        assert_eq!(keyframes[0].value, Vec3::ZERO);
        assert_eq!(keyframes[1].value, Vec3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn gltf_materials_preserve_pbr_factors() {
        let raw = br#"{
            "asset": { "version": "2.0" },
            "images": [
                { "uri": "albedo.png" },
                { "uri": "normal.png" },
                { "uri": "metallic_roughness.png" }
            ],
            "textures": [
                { "source": 0 },
                { "source": 1 },
                { "source": 2 }
            ],
            "materials": [
                {
                    "pbrMetallicRoughness": {
                        "baseColorFactor": [0.8, 0.7, 0.6, 1.0],
                        "metallicFactor": 0.35,
                        "roughnessFactor": 0.85,
                        "baseColorTexture": { "index": 0 },
                        "metallicRoughnessTexture": { "index": 2 }
                    },
                    "normalTexture": { "index": 1 },
                    "emissiveFactor": [0.1, 0.2, 0.3],
                    "alphaMode": "BLEND"
                }
            ]
        }"#;
        let gltf = gltf::Gltf::from_slice(raw).unwrap();

        let materials = extract_materials(&gltf.document);

        assert_eq!(materials.len(), 1);
        let descriptor = &materials[0].1;
        assert_eq!(descriptor.base_color, [0.8, 0.7, 0.6, 1.0]);
        assert_eq!(descriptor.metallic_factor, 0.35);
        assert_eq!(descriptor.roughness_factor, 0.85);
        assert_eq!(descriptor.emissive_color, [0.1, 0.2, 0.3]);
        assert_eq!(descriptor.alpha_mode, AlphaMode::Blend);
        assert_eq!(descriptor.albedo_texture.as_deref(), Some("#image_0"));
        assert_eq!(descriptor.normal_texture.as_deref(), Some("#image_1"));
        assert_eq!(
            descriptor.metallic_texture.as_deref(),
            Some("#image_2_metallic")
        );
        assert_eq!(
            descriptor.roughness_texture.as_deref(),
            Some("#image_2_roughness")
        );
    }

    #[test]
    fn gltf_images_publish_metallic_and_roughness_textures_from_packed_map() {
        let raw = br#"{
            "asset": { "version": "2.0" },
            "images": [
                { "uri": "metallic_roughness.png" }
            ],
            "textures": [
                { "source": 0 }
            ],
            "materials": [
                {
                    "pbrMetallicRoughness": {
                        "metallicRoughnessTexture": { "index": 0 }
                    }
                }
            ]
        }"#;
        let gltf = gltf::Gltf::from_slice(raw).unwrap();
        let images = vec![gltf::image::Data {
            pixels: vec![7, 64, 128, 255, 11, 200, 220, 255],
            format: Format::R8G8B8A8,
            width: 2,
            height: 1,
        }];

        let images = extract_images(&gltf.document, images).unwrap();

        assert_eq!(images.len(), 3);
        let metallic = images
            .iter()
            .find(|(name, _)| name == "image_0_metallic")
            .expect("expected generated metallic texture");
        assert_eq!(
            metallic.1.rgba,
            vec![128, 128, 128, 255, 220, 220, 220, 255]
        );
        let roughness = images
            .iter()
            .find(|(name, _)| name == "image_0_roughness")
            .expect("expected generated roughness texture");
        assert_eq!(roughness.1.rgba, vec![64, 64, 64, 255, 200, 200, 200, 255]);
        assert!(images.iter().any(|(name, _)| name == "image_0"));
    }
}
