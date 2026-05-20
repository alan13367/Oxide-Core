//! Pipeline creation utilities

use wgpu::{
    BindGroupLayout, BlendState, ColorTargetState, ColorWrites, CompareFunction, DepthStencilState,
    Device, FragmentState, PipelineLayoutDescriptor, PrimitiveState, PrimitiveTopology,
    RenderPipeline, RenderPipelineDescriptor, ShaderModule, ShaderModuleDescriptor, ShaderSource,
    StencilState, TextureFormat, VertexState,
};

use crate::descriptor::AlphaMode;
use crate::mesh::{Vertex, Vertex3D};

pub fn create_shader(device: &Device, source: &str, label: Option<&str>) -> ShaderModule {
    // Parse once to surface syntax issues early in logs before pipeline creation.
    match naga::front::wgsl::parse_str(source) {
        Ok(_) => {}
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
    create_basic_pipeline_with_alpha(device, shader, format, AlphaMode::Opaque)
}

pub fn create_basic_pipeline_with_alpha(
    device: &Device,
    shader: &ShaderModule,
    format: TextureFormat,
    alpha_mode: AlphaMode,
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
                blend: blend_state(alpha_mode),
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
    create_lit_pipeline_with_alpha(
        device,
        shader,
        format,
        camera_layout,
        material_layout,
        light_layout,
        AlphaMode::Opaque,
    )
}

pub fn create_lit_pipeline_with_alpha(
    device: &Device,
    shader: &ShaderModule,
    format: TextureFormat,
    camera_layout: &BindGroupLayout,
    material_layout: &BindGroupLayout,
    light_layout: &BindGroupLayout,
    alpha_mode: AlphaMode,
) -> RenderPipeline {
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Lit Pipeline Layout"),
        bind_group_layouts: &[
            Some(camera_layout),
            Some(material_layout),
            Some(light_layout),
        ],
        immediate_size: 0,
    });

    let depth_stencil = Some(DepthStencilState {
        format: TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(depth_write_enabled(alpha_mode)),
        depth_compare: Some(depth_compare(alpha_mode)),
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
                blend: blend_state(alpha_mode),
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
    create_unlit_pipeline_with_alpha(
        device,
        shader,
        format,
        camera_layout,
        material_layout,
        AlphaMode::Opaque,
    )
}

pub fn create_unlit_pipeline_with_alpha(
    device: &Device,
    shader: &ShaderModule,
    format: TextureFormat,
    camera_layout: &BindGroupLayout,
    material_layout: &BindGroupLayout,
    alpha_mode: AlphaMode,
) -> RenderPipeline {
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Unlit Pipeline Layout"),
        bind_group_layouts: &[Some(camera_layout), Some(material_layout)],
        immediate_size: 0,
    });

    let depth_stencil = Some(DepthStencilState {
        format: TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(depth_write_enabled(alpha_mode)),
        depth_compare: Some(depth_compare(alpha_mode)),
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
                blend: blend_state(alpha_mode),
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

fn blend_state(alpha_mode: AlphaMode) -> Option<BlendState> {
    match alpha_mode {
        AlphaMode::Opaque => None,
        AlphaMode::Blend => Some(BlendState::ALPHA_BLENDING),
    }
}

fn depth_write_enabled(alpha_mode: AlphaMode) -> bool {
    matches!(alpha_mode, AlphaMode::Opaque)
}

fn depth_compare(alpha_mode: AlphaMode) -> CompareFunction {
    match alpha_mode {
        AlphaMode::Opaque => CompareFunction::Less,
        AlphaMode::Blend => CompareFunction::LessEqual,
    }
}
