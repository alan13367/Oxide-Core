//! glTF model loader

use std::path::{Path, PathBuf};

use glam::{Quat, Vec3};
use gltf::buffer::Data;
use gltf::image::Format;
use gltf::mesh::Mode;
use wgpu::{Device, Queue};

use crate::descriptor::{MaterialDescriptor, MaterialType, ShaderDescriptor};
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
    /// Material index for each loaded mesh, aligned with [`Self::meshes`].
    pub mesh_material_indices: Vec<Option<usize>>,
    /// Node hierarchy information for spawning entities.
    pub nodes: Vec<GltfNode>,
}

/// Represents a node in the glTF hierarchy.
#[derive(Clone, Debug)]
pub struct GltfNode {
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
    let images = extract_images(images)?;

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
            let descriptor = MaterialDescriptor {
                name: name.clone(),
                material_type: MaterialType::Lit,
                shader: ShaderDescriptor::Builtin {
                    shader: "lit".to_string(),
                },
                fallback_shader: Some("lit".to_string()),
                base_color: pbr.base_color_factor(),
                albedo_texture,
                normal_texture: None,
                roughness_texture: None,
            };
            (name, descriptor)
        })
        .collect()
}

fn extract_images(
    images: Vec<gltf::image::Data>,
) -> Result<Vec<(String, TextureImage)>, GltfError> {
    images
        .into_iter()
        .enumerate()
        .map(|(idx, image)| {
            let label = format!("image_{idx}");
            let rgba = gltf_image_to_rgba(&image);
            let texture =
                TextureImage::from_rgba(image.width, image.height, rgba).map_err(|source| {
                    GltfError::TextureImage {
                        label: label.clone(),
                        source,
                    }
                })?;
            Ok((label, texture))
        })
        .collect()
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
            .enumerate()
            .map(|(idx, node)| convert_node(idx, &node, meshes))
            .collect(),
        None => Vec::new(),
    }
}

/// Converts a glTF node to our GltfNode type.
fn convert_node(node_idx: usize, node: &gltf::Node, meshes: &[(String, Mesh3D)]) -> GltfNode {
    let (t, r, s) = node.transform().decomposed();

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
        name: Some(format!("node_{}", node_idx)),
        mesh_index,
        translation: Vec3::new(t[0], t[1], t[2]),
        rotation: Quat::from_xyzw(r[0], r[1], r[2], r[3]),
        scale: Vec3::new(s[0], s[1], s[2]),
        children: node
            .children()
            .enumerate()
            .map(|(idx, child)| convert_node(idx, &child, meshes))
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
}
