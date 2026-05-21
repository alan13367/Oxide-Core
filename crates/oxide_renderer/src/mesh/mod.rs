//! Mesh module

mod primitive;
mod vertex;

pub use primitive::*;
pub use vertex::{skin_vertices, triangle_vertices, MeshSkinning, Vertex, Vertex3D};

use wgpu::{Buffer, BufferDescriptor, BufferUsages, Device};

pub struct Mesh3D {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub index_count: u32,
    pub vertices: Vec<Vertex3D>,
    pub indices: Vec<u16>,
    pub skinning: Option<MeshSkinning>,
}

impl Mesh3D {
    pub fn new_cube(device: &Device) -> Self {
        let vertices = cube_vertices();
        let indices = cube_indices();

        Self::create(device, &vertices, &indices, Some("Cube"))
    }

    pub fn new_sphere(device: &Device, segments: u32, rings: u32) -> Self {
        let vertices = sphere_vertices(segments, rings);
        let indices = sphere_indices(segments, rings);

        Self::create(device, &vertices, &indices, Some("Sphere"))
    }

    /// Creates a mesh from vertex and index data.
    pub fn create(
        device: &Device,
        vertices: &[Vertex3D],
        indices: &[u16],
        label: Option<&str>,
    ) -> Self {
        Self::create_with_skinning(device, vertices, indices, None, label)
    }

    /// Creates a mesh from vertex/index data and optional skinning attributes.
    pub fn create_with_skinning(
        device: &Device,
        vertices: &[Vertex3D],
        indices: &[u16],
        skinning: Option<MeshSkinning>,
        label: Option<&str>,
    ) -> Self {
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: label.map(|l| format!("{} Vertex Buffer", l)).as_deref(),
            size: std::mem::size_of_val(vertices) as wgpu::BufferAddress,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });

        vertex_buffer
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(vertices));
        vertex_buffer.unmap();

        let index_buffer = device.create_buffer(&BufferDescriptor {
            label: label.map(|l| format!("{} Index Buffer", l)).as_deref(),
            size: std::mem::size_of_val(indices) as wgpu::BufferAddress,
            usage: BufferUsages::INDEX,
            mapped_at_creation: true,
        });

        index_buffer
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(indices));
        index_buffer.unmap();

        Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            vertices: vertices.to_vec(),
            indices: indices.to_vec(),
            skinning,
        }
    }

    /// Returns CPU-skinned vertices for the provided joint matrices.
    pub fn skinned_vertices(&self, joint_matrices: &[glam::Mat4]) -> Option<Vec<Vertex3D>> {
        self.skinning
            .as_ref()
            .map(|skinning| skin_vertices(&self.vertices, skinning, joint_matrices))
    }

    /// Writes replacement vertices into the existing GPU vertex buffer.
    ///
    /// Returns false when the replacement vertex count does not match the mesh
    /// buffer allocation.
    pub fn write_vertices(&mut self, queue: &wgpu::Queue, vertices: &[Vertex3D]) -> bool {
        if vertices.len() != self.vertices.len() {
            return false;
        }
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(vertices));
        self.vertices.clear();
        self.vertices.extend_from_slice(vertices);
        true
    }
}

pub struct Mesh {
    pub vertex_buffer: Buffer,
    pub vertex_count: u32,
}

impl Mesh {
    pub fn new_triangle(device: &Device) -> Self {
        let vertices = triangle_vertices();

        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Triangle Vertex Buffer"),
            size: std::mem::size_of_val(&vertices) as wgpu::BufferAddress,
            usage: BufferUsages::VERTEX,
            mapped_at_creation: true,
        });

        vertex_buffer
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(&vertices));
        vertex_buffer.unmap();

        Self {
            vertex_buffer,
            vertex_count: vertices.len() as u32,
        }
    }
}
