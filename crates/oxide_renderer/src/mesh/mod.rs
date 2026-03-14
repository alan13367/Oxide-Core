//! Mesh module

mod primitive;
mod vertex;

pub use primitive::*;
pub use vertex::{triangle_vertices, Vertex, Vertex3D};

use wgpu::{Buffer, BufferDescriptor, BufferUsages, Device};

pub struct Mesh3D {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub index_count: u32,
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
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: label.map(|l| format!("{} Vertex Buffer", l)).as_deref(),
            size: std::mem::size_of_val(vertices) as wgpu::BufferAddress,
            usage: BufferUsages::VERTEX,
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
        }
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
