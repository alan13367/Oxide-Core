//! Debug line rendering for physics visualization and debugging.
//!
//! Provides a simple API for drawing lines, shapes, and arrows in 3D space.
//! Uses a dynamic vertex buffer updated each frame.

use std::mem;

use glam::{Mat4, Vec3};
use oxide_ecs::Resource;
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, BufferDescriptor, BufferUsages, Device, RenderPass,
    RenderPipeline, ShaderModuleDescriptor, ShaderSource,
};

use crate::mesh::Vertex;

/// Maximum number of line vertices in the debug buffer.
pub const MAX_DEBUG_VERTICES: usize = 64 * 1024;

/// Debug line renderer for drawing wireframe shapes and lines.
#[derive(Resource)]
pub struct DebugLines {
    /// Accumulated line vertices for the current frame.
    vertices: Vec<Vertex>,
    /// GPU vertex buffer.
    vertex_buffer: Buffer,
    /// Render pipeline for line rendering.
    pipeline: RenderPipeline,
    /// Camera bind group layout.
    camera_layout: BindGroupLayout,
    /// Number of vertices currently in the buffer.
    vertex_count: u32,
}

impl DebugLines {
    /// Create a new debug line renderer.
    pub fn new(device: &Device, format: wgpu::TextureFormat) -> Self {
        // Create vertex buffer
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Debug Lines Vertex Buffer"),
            size: (MAX_DEBUG_VERTICES * mem::size_of::<Vertex>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create camera bind group layout
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Debug Lines Camera Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Debug Lines Pipeline Layout"),
            bind_group_layouts: &[&camera_layout],
            immediate_size: 0,
        });

        // Create shader
        let shader_source = include_str!("../shaders/debug_lines.wgsl");
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Debug Lines Shader"),
            source: ShaderSource::Wgsl(shader_source.into()),
        });

        // Create pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Debug Lines Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3, // position
                        1 => Float32x3, // color
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            vertices: Vec::with_capacity(MAX_DEBUG_VERTICES),
            vertex_buffer,
            pipeline,
            camera_layout,
            vertex_count: 0,
        }
    }

    /// Create a camera bind group for the debug lines.
    pub fn create_camera_bind_group(&self, device: &Device, buffer: &Buffer) -> BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Debug Lines Camera Bind Group"),
            layout: &self.camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }

    /// Clear all lines. Call at the start of each frame.
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.vertex_count = 0;
    }

    /// Draw a line from start to end with the given color.
    pub fn draw_line(&mut self, start: Vec3, end: Vec3, color: Vec3) {
        if self.vertices.len() + 2 > MAX_DEBUG_VERTICES {
            return;
        }

        let color = [color.x, color.y, color.z];
        self.vertices.push(Vertex {
            position: [start.x, start.y, start.z],
            color,
        });
        self.vertices.push(Vertex {
            position: [end.x, end.y, end.z],
            color,
        });
    }

    /// Draw a wireframe sphere.
    pub fn draw_sphere(&mut self, center: Vec3, radius: f32, color: Vec3, segments: u32) {
        let segments = segments.max(8);

        // Draw circles in XY, XZ, YZ planes
        for axis in 0..3 {
            for i in 0..segments {
                let angle1 = (i as f32 / segments as f32) * std::f32::consts::TAU;
                let angle2 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;

                let (p1, p2) = match axis {
                    0 => {
                        // YZ plane
                        (
                            center + radius * Vec3::new(0.0, angle1.cos(), angle1.sin()),
                            center + radius * Vec3::new(0.0, angle2.cos(), angle2.sin()),
                        )
                    }
                    1 => {
                        // XZ plane
                        (
                            center + radius * Vec3::new(angle1.cos(), 0.0, angle1.sin()),
                            center + radius * Vec3::new(angle2.cos(), 0.0, angle2.sin()),
                        )
                    }
                    _ => {
                        // XY plane
                        (
                            center + radius * Vec3::new(angle1.cos(), angle1.sin(), 0.0),
                            center + radius * Vec3::new(angle2.cos(), angle2.sin(), 0.0),
                        )
                    }
                };

                self.draw_line(p1, p2, color);
            }
        }
    }

    /// Draw a wireframe box (OBB) with the given transform.
    pub fn draw_box(&mut self, transform: Mat4, half_extents: Vec3, color: Vec3) {
        let ex = half_extents.x;
        let ey = half_extents.y;
        let ez = half_extents.z;

        // 8 corners of the unit cube, scaled and transformed
        let corners = [
            Vec3::new(-ex, -ey, -ez),
            Vec3::new(ex, -ey, -ez),
            Vec3::new(ex, ey, -ez),
            Vec3::new(-ex, ey, -ez),
            Vec3::new(-ex, -ey, ez),
            Vec3::new(ex, -ey, ez),
            Vec3::new(ex, ey, ez),
            Vec3::new(-ex, ey, ez),
        ];

        // Transform corners
        let corners: Vec<Vec3> = corners
            .iter()
            .map(|c| transform.transform_point3(*c))
            .collect();

        // Draw 12 edges
        // Bottom face
        self.draw_line(corners[0], corners[1], color);
        self.draw_line(corners[1], corners[2], color);
        self.draw_line(corners[2], corners[3], color);
        self.draw_line(corners[3], corners[0], color);

        // Top face
        self.draw_line(corners[4], corners[5], color);
        self.draw_line(corners[5], corners[6], color);
        self.draw_line(corners[6], corners[7], color);
        self.draw_line(corners[7], corners[4], color);

        // Vertical edges
        self.draw_line(corners[0], corners[4], color);
        self.draw_line(corners[1], corners[5], color);
        self.draw_line(corners[2], corners[6], color);
        self.draw_line(corners[3], corners[7], color);
    }

    /// Draw an arrow from origin in the given direction.
    pub fn draw_arrow(&mut self, origin: Vec3, direction: Vec3, color: Vec3, head_size: f32) {
        let end = origin + direction;
        self.draw_line(origin, end, color);

        // Draw arrowhead
        let dir = direction.normalize();
        let head_length = direction.length().min(head_size * 2.0) * 0.3;

        // Create perpendicular vectors for arrowhead
        let up = if dir.y.abs() < 0.99 { Vec3::Y } else { Vec3::X };
        let right = up.cross(dir).normalize();
        let up = dir.cross(right);

        // Arrowhead lines
        let head_base = end - dir * head_length;
        let head_right = head_base + right * head_size * 0.5;
        let head_left = head_base - right * head_size * 0.5;
        let head_up = head_base + up * head_size * 0.5;
        let head_down = head_base - up * head_size * 0.5;

        self.draw_line(end, head_right, color);
        self.draw_line(end, head_left, color);
        self.draw_line(end, head_up, color);
        self.draw_line(end, head_down, color);
    }

    /// Draw a coordinate axis triplet at the given transform.
    pub fn draw_axes(&mut self, transform: Mat4, size: f32) {
        let origin = transform.transform_point3(Vec3::ZERO);

        // X axis (red)
        let x_end = transform.transform_vector3(Vec3::X * size);
        self.draw_arrow(origin, x_end, Vec3::new(1.0, 0.0, 0.0), size * 0.1);

        // Y axis (green)
        let y_end = transform.transform_vector3(Vec3::Y * size);
        self.draw_arrow(origin, y_end, Vec3::new(0.0, 1.0, 0.0), size * 0.1);

        // Z axis (blue)
        let z_end = transform.transform_vector3(Vec3::Z * size);
        self.draw_arrow(origin, z_end, Vec3::new(0.0, 0.0, 1.0), size * 0.1);
    }

    /// Update the GPU buffer with current vertices. Call before rendering.
    pub fn update_buffer(&mut self, queue: &wgpu::Queue) {
        if self.vertices.is_empty() {
            self.vertex_count = 0;
            return;
        }

        self.vertex_count = self.vertices.len() as u32;
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
    }

    /// Render all lines.
    pub fn render<'a>(
        &'a self,
        render_pass: &mut RenderPass<'a>,
        camera_bind_group: &'a BindGroup,
    ) {
        if self.vertex_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }

    /// Get the number of vertices currently queued.
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }
}
