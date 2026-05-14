//! Light buffer management for GPU.

use wgpu::{BindGroup, BindGroupLayout, Buffer, Device, Queue};

use oxide_ecs::world::World;

use super::components::{AmbientLight, DirectionalLight, PointLight};
use super::uniform::{GpuPointLight, LightUniform, MAX_POINT_LIGHTS};

pub struct LightBuffer {
    pub uniform_buffer: Buffer,
    pub point_light_buffer: Buffer,
    pub bind_group_layout: BindGroupLayout,
    pub bind_group: BindGroup,
    point_light_capacity: usize,
}

impl LightBuffer {
    pub fn new(device: &Device) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Light Uniform Buffer"),
            size: std::mem::size_of::<LightUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let point_light_capacity = 1usize;
        let point_light_buffer = Self::create_point_light_buffer(device, point_light_capacity);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Light Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = Self::create_bind_group(
            device,
            &bind_group_layout,
            &uniform_buffer,
            &point_light_buffer,
        );

        Self {
            uniform_buffer,
            point_light_buffer,
            bind_group_layout,
            bind_group,
            point_light_capacity,
        }
    }

    pub fn update(&mut self, device: &Device, queue: &Queue, world: &mut World) {
        let ambient = {
            let mut query = world.query::<&AmbientLight>();
            let first = query.iter(world).next().copied();
            first
        };

        let directional_lights: Vec<DirectionalLight> = {
            let mut query = world.query::<&DirectionalLight>();
            query.iter(world).copied().collect()
        };

        let point_lights: Vec<PointLight> = {
            let mut query = world.query::<&PointLight>();
            query.iter(world).copied().take(MAX_POINT_LIGHTS).collect()
        };

        self.ensure_point_light_capacity(device, point_lights.len().max(1));

        let gpu_point_lights: Vec<GpuPointLight> = point_lights
            .iter()
            .map(|light| {
                GpuPointLight::from_values(
                    light.position,
                    light.color,
                    light.intensity,
                    light.radius,
                )
            })
            .collect();

        let uniform = LightUniform::new(
            ambient.as_ref(),
            &directional_lights.iter().collect::<Vec<_>>(),
            gpu_point_lights.len(),
        );

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        if !gpu_point_lights.is_empty() {
            queue.write_buffer(
                &self.point_light_buffer,
                0,
                bytemuck::cast_slice(&gpu_point_lights),
            );
        }
    }

    fn ensure_point_light_capacity(&mut self, device: &Device, required: usize) {
        if required <= self.point_light_capacity {
            return;
        }

        self.point_light_capacity = required.next_power_of_two();
        self.point_light_buffer =
            Self::create_point_light_buffer(device, self.point_light_capacity);
        self.bind_group = Self::create_bind_group(
            device,
            &self.bind_group_layout,
            &self.uniform_buffer,
            &self.point_light_buffer,
        );
    }

    fn create_point_light_buffer(device: &Device, capacity: usize) -> Buffer {
        let byte_size = (capacity * std::mem::size_of::<GpuPointLight>()) as u64;
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Point Light Storage Buffer"),
            size: byte_size.max(std::mem::size_of::<GpuPointLight>() as u64),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn create_bind_group(
        device: &Device,
        layout: &BindGroupLayout,
        uniform_buffer: &Buffer,
        point_light_buffer: &Buffer,
    ) -> BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Light Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: point_light_buffer.as_entire_binding(),
                },
            ],
        })
    }
}
