//! glTF model loader

use std::path::Path;

use glam::{Quat, Vec3};
use gltf::buffer::Data;
use gltf::mesh::Mode;
use wgpu::{Device, Queue};

use crate::mesh::Mesh3D;
use crate::mesh::Vertex3D;

#[derive(thiserror::Error, Debug)]
pub enum GltfError {
    #[error("Failed to load glTF file '{path}': {source}")]
    Load {
        path: String,
        source: gltf::Error,
    },
    #[error("Failed to read glTF buffers: {0}")]
    Buffer(String),
    #[error("Primitive mode {0:?} is not supported. Only triangles are supported.")]
    UnsupportedMode(Mode),
    #[error("Mesh has no positions")]
    MissingPositions,
}

/// Result of loading a glTF file.
pub struct GltfScene {
    /// Loaded meshes with their names.
    pub meshes: Vec<(String, Mesh3D)>,
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
    let (document, buffers, _images) = gltf::import(path).map_err(|source| GltfError::Load {
        path: path_str.clone(),
        source,
    })?;

    // Extract meshes
    let mut meshes = Vec::new();
    for (mesh_idx, mesh) in document.meshes().enumerate() {
        for (prim_idx, primitive) in mesh.primitives().enumerate() {
            // Only support triangle mode
            if primitive.mode() != Mode::Triangles {
                return Err(GltfError::UnsupportedMode(primitive.mode()));
            }

            let mesh_name = format!("mesh_{}_prim{}", mesh_idx, prim_idx);
            let loaded_mesh = load_primitive(device, queue, &primitive, &buffers, &mesh_name)?;
            meshes.push((mesh_name, loaded_mesh));
        }
    }

    // Extract node hierarchy
    let nodes = extract_nodes(&document, &meshes);

    Ok(GltfScene { meshes, nodes })
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