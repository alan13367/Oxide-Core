//! Pipeline creation utilities

use wgpu::{
    BindGroupLayout, ColorTargetState, ColorWrites, CompareFunction, DepthStencilState, Device,
    FragmentState, PipelineLayoutDescriptor, PrimitiveState, PrimitiveTopology, RenderPipeline,
    RenderPipelineDescriptor, ShaderModule, ShaderModuleDescriptor, ShaderSource, StencilState,
    TextureFormat, VertexState,
};

use crate::mesh::{Vertex, Vertex3D};

pub fn create_shader(device: &Device, source: &str, label: Option<&str>) -> ShaderModule {
    // Basic bind-group validation via naga reflection
    match naga::front::wgsl::parse_str(source) {
        Ok(module) => {
            let mut has_camera_bind_group = false;
            for (_, var) in module.global_variables.iter() {
                if let Some(binding) = &var.binding {
                    if binding.group == 0 {
                        has_camera_bind_group = true;
                        break;
                    }
                }
            }
            if !has_camera_bind_group {
                tracing::warn!(
                    "Shader '{}' validation warning: missing camera bind group (Group 0). It may not render correctly in Lit or Unlit pipelines.",
                    label.unwrap_or("unnamed")
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                "Failed to parse WGSL source for '{}' during validation: {:?}",
                label.unwrap_or("unnamed"),
                e
            );
        }
    }

    device.create_shader_module(ShaderModuleDescriptor {
        label,
        source: ShaderSource::Wgsl(source.into()),
    })
}

pub fn create_basic_pipeline(
    device: &Device,
    shader: &ShaderModule,
    format: TextureFormat,
) -> RenderPipeline {
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Basic Pipeline Layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Basic Pipeline"),
        layout: Some(&layout),
        vertex: VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

pub fn create_lit_pipeline(
    device: &Device,
    shader: &ShaderModule,
    format: TextureFormat,
    camera_layout: &BindGroupLayout,
    material_layout: &BindGroupLayout,
    light_layout: &BindGroupLayout,
) -> RenderPipeline {
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Lit Pipeline Layout"),
        bind_group_layouts: &[camera_layout, material_layout, light_layout],
        immediate_size: 0,
    });

    let depth_stencil = Some(DepthStencilState {
        format: TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: true,
        depth_compare: CompareFunction::Less,
        stencil: StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Lit Pipeline"),
        layout: Some(&layout),
        vertex: VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex3D::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

pub fn create_unlit_pipeline(
    device: &Device,
    shader: &ShaderModule,
    format: TextureFormat,
    camera_layout: &BindGroupLayout,
    material_layout: &BindGroupLayout,
) -> RenderPipeline {
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Unlit Pipeline Layout"),
        bind_group_layouts: &[camera_layout, material_layout],
        immediate_size: 0,
    });

    let depth_stencil = Some(DepthStencilState {
        format: TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: true,
        depth_compare: CompareFunction::Less,
        stencil: StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Unlit Pipeline"),
        layout: Some(&layout),
        vertex: VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex3D::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}