//! Light buffer management for GPU

use wgpu::{BindGroup, BindGroupLayout, Buffer, Device, Queue};

use oxide_ecs::world::World;

use super::components::{AmbientLight, DirectionalLight, PointLight};
use super::uniform::LightUniform;

/// GPU buffer and bind group for light data.
pub struct LightBuffer {
    pub buffer: Buffer,
    pub bind_group_layout: BindGroupLayout,
    pub bind_group: BindGroup,
}

impl LightBuffer {
    /// Creates a new light buffer with its bind group.
    pub fn new(device: &Device) -> Self {
        // Create the uniform buffer
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Light Uniform Buffer"),
            size: std::mem::size_of::<LightUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Light Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Light Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            buffer,
            bind_group_layout,
            bind_group,
        }
    }

    /// Updates the light buffer from the ECS world.
    pub fn update(&self, queue: &Queue, world: &mut World) {
        // Gather ambient light (take the first one if multiple exist)
        let ambient = {
            let mut query = world.query::<&AmbientLight>();
            let first = query.iter(world).next().copied();
            first
        };

        // Gather directional lights
        let directional_lights: Vec<DirectionalLight> = {
            let mut query = world.query::<&DirectionalLight>();
            query.iter(world).copied().collect()
        };

        // Gather point lights
        let point_lights: Vec<PointLight> = {
            let mut query = world.query::<&PointLight>();
            query.iter(world).copied().collect()
        };

        // Create and upload the uniform
        let uniform = LightUniform::new(
            ambient.as_ref(),
            &directional_lights.iter().collect::<Vec<_>>(),
            &point_lights.iter().collect::<Vec<_>>(),
        );
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Updates the light buffer with a pre-built uniform.
    pub fn update_uniform(&self, queue: &Queue, uniform: &LightUniform) {
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(uniform));
    }
}