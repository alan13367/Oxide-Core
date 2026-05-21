//! Vertex types and attributes

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::{VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode};

/// Simple vertex for basic demos (position + color)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl Vertex {
    pub fn new(position: [f32; 3], color: [f32; 3]) -> Self {
        Self { position, color }
    }

    pub fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x3,
                },
            ],
        }
    }
}

pub fn triangle_vertices() -> Vec<Vertex> {
    vec![
        Vertex::new([0.0, 0.5, 0.0], [1.0, 0.0, 0.0]),
        Vertex::new([-0.5, -0.5, 0.0], [0.0, 1.0, 0.0]),
        Vertex::new([0.5, -0.5, 0.0], [0.0, 0.0, 1.0]),
    ]
}

/// Full 3D vertex format with position, normal, and UV
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex3D {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex3D {
    pub fn new(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            uv,
        }
    }

    pub fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex3D>() as wgpu::BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: VertexFormat::Float32x2,
                },
            ],
        }
    }
}

/// Per-vertex joint indices and weights for linear blend skinning.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshSkinning {
    /// Joint indices influencing each vertex.
    pub joints: Vec<[u16; 4]>,
    /// Joint weights influencing each vertex.
    pub weights: Vec<[f32; 4]>,
}

impl MeshSkinning {
    /// Creates skinning attributes aligned with a mesh vertex buffer.
    pub fn new(joints: Vec<[u16; 4]>, weights: Vec<[f32; 4]>) -> Self {
        Self { joints, weights }
    }

    /// Returns true when no vertex skinning attributes are stored.
    pub fn is_empty(&self) -> bool {
        self.joints.is_empty() || self.weights.is_empty()
    }

    /// Returns the number of vertices with skinning attributes.
    pub fn len(&self) -> usize {
        self.joints.len().min(self.weights.len())
    }
}

/// Applies linear blend skinning to CPU-side mesh vertices.
pub fn skin_vertices(
    vertices: &[Vertex3D],
    skinning: &MeshSkinning,
    joint_matrices: &[Mat4],
) -> Vec<Vertex3D> {
    vertices
        .iter()
        .enumerate()
        .map(|(vertex_index, vertex)| {
            let Some(joints) = skinning.joints.get(vertex_index) else {
                return *vertex;
            };
            let Some(weights) = skinning.weights.get(vertex_index) else {
                return *vertex;
            };

            let position = Vec3::from_array(vertex.position);
            let normal = Vec3::from_array(vertex.normal);
            let mut skinned_position = Vec3::ZERO;
            let mut skinned_normal = Vec3::ZERO;
            let mut total_weight = 0.0;

            for influence in 0..4 {
                let weight = weights[influence];
                if weight <= 0.0 {
                    continue;
                }
                let joint_index = joints[influence] as usize;
                let Some(joint_matrix) = joint_matrices.get(joint_index) else {
                    continue;
                };
                skinned_position += joint_matrix.transform_point3(position) * weight;
                skinned_normal += joint_matrix.transform_vector3(normal) * weight;
                total_weight += weight;
            }

            if total_weight <= f32::EPSILON {
                return *vertex;
            }

            Vertex3D {
                position: (skinned_position / total_weight).to_array(),
                normal: (skinned_normal / total_weight)
                    .try_normalize()
                    .unwrap_or(normal)
                    .to_array(),
                uv: vertex.uv,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_vertices_blends_joint_transforms() {
        let vertices = vec![Vertex3D::new([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.5])];
        let skinning = MeshSkinning::new(vec![[0, 1, 0, 0]], vec![[0.25, 0.75, 0.0, 0.0]]);
        let joints = vec![
            Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0)),
            Mat4::from_translation(Vec3::new(0.0, 3.0, 0.0)),
        ];

        let skinned = skin_vertices(&vertices, &skinning, &joints);

        assert_eq!(skinned.len(), 1);
        assert_eq!(skinned[0].position, [1.0, 2.5, 0.0]);
        assert_eq!(skinned[0].normal, [0.0, 1.0, 0.0]);
        assert_eq!(skinned[0].uv, [0.5, 0.5]);
    }

    #[test]
    fn skin_vertices_keeps_vertices_without_valid_weights() {
        let vertex = Vertex3D::new([1.0, 2.0, 3.0], [0.0, 1.0, 0.0], [0.0, 0.0]);
        let skinning = MeshSkinning::new(vec![[8, 9, 10, 11]], vec![[1.0, 0.0, 0.0, 0.0]]);

        assert_eq!(skin_vertices(&[vertex], &skinning, &[]), vec![vertex]);
    }
}
