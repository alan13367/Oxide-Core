//! Small post-processing and fullscreen-pass building blocks.

use wgpu::{
    BindGroup, BindGroupLayout, BlendState, ColorTargetState, ColorWrites, CommandEncoder, Device,
    FilterMode, FragmentState, LoadOp, Operations, PipelineLayoutDescriptor, PrimitiveState,
    PrimitiveTopology, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderModule,
    ShaderModuleDescriptor, ShaderSource, StoreOp, Texture, TextureDescriptor, TextureDimension,
    TextureFormat, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor,
    TextureViewDimension, VertexState,
};

/// Built-in fullscreen texture blit shader.
pub const FULLSCREEN_BLIT_SHADER: &str = include_str!("../shaders/fullscreen_blit.wgsl");

/// Descriptor for an offscreen color target that can also be sampled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderTextureDescriptor {
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Texture format used by the color target.
    pub format: TextureFormat,
    /// Additional usages beyond render attachment and texture binding.
    pub extra_usage: TextureUsages,
    /// Optional debug label.
    pub label: Option<String>,
}

impl RenderTextureDescriptor {
    /// Creates a descriptor for a sampled color render target.
    pub fn new(width: u32, height: u32, format: TextureFormat) -> Self {
        Self {
            width,
            height,
            format,
            extra_usage: TextureUsages::empty(),
            label: None,
        }
    }

    /// Adds additional texture usages, such as `COPY_SRC` for screenshots.
    pub fn with_extra_usage(mut self, usage: TextureUsages) -> Self {
        self.extra_usage = usage;
        self
    }

    /// Sets a debug label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Returns the actual texture usages used for allocation.
    pub fn usage(&self) -> TextureUsages {
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | self.extra_usage
    }

    /// Returns dimensions clamped to wgpu's non-zero texture requirement.
    pub fn clamped_size(&self) -> (u32, u32) {
        (self.width.max(1), self.height.max(1))
    }
}

/// GPU color target that can be rendered into and sampled by later passes.
pub struct RenderTexture {
    pub texture: Texture,
    pub view: TextureView,
    pub sampler: Sampler,
    descriptor: RenderTextureDescriptor,
}

impl RenderTexture {
    /// Allocates a new sampled render texture.
    pub fn new(device: &Device, descriptor: RenderTextureDescriptor) -> Self {
        let (width, height) = descriptor.clamped_size();
        let texture = device.create_texture(&TextureDescriptor {
            label: descriptor.label.as_deref(),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: descriptor.format,
            usage: descriptor.usage(),
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: descriptor
                .label
                .as_ref()
                .map(|label| format!("{label} Sampler"))
                .as_deref(),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            descriptor,
        }
    }

    /// Returns the descriptor used to allocate this texture.
    pub fn descriptor(&self) -> &RenderTextureDescriptor {
        &self.descriptor
    }

    /// Returns the allocated width and height.
    pub fn size(&self) -> (u32, u32) {
        self.descriptor.clamped_size()
    }

    /// Returns the texture format.
    pub fn format(&self) -> TextureFormat {
        self.descriptor.format
    }

    /// Reallocates the texture when size or format changed.
    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        if self.descriptor.width == width && self.descriptor.height == height {
            return;
        }
        let descriptor = RenderTextureDescriptor {
            width,
            height,
            ..self.descriptor.clone()
        };
        *self = Self::new(device, descriptor);
    }

    /// Reallocates the texture when the descriptor changed.
    pub fn reconfigure(&mut self, device: &Device, descriptor: RenderTextureDescriptor) {
        if self.descriptor == descriptor {
            return;
        }
        *self = Self::new(device, descriptor);
    }

    /// Builds a color attachment descriptor for this texture.
    pub fn color_attachment(&self, load: LoadOp<wgpu::Color>) -> RenderPassColorAttachment<'_> {
        RenderPassColorAttachment {
            view: &self.view,
            resolve_target: None,
            ops: Operations {
                load,
                store: StoreOp::Store,
            },
            depth_slice: None,
        }
    }
}

/// Pipeline that draws a sampled texture over a full-screen triangle.
pub struct FullscreenBlitPipeline {
    pipeline: RenderPipeline,
    bind_group_layout: BindGroupLayout,
    sampler: Sampler,
}

impl FullscreenBlitPipeline {
    /// Creates the built-in fullscreen blit pipeline for `format`.
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Fullscreen Blit Shader"),
            source: ShaderSource::Wgsl(FULLSCREEN_BLIT_SHADER.into()),
        });
        Self::with_shader(device, format, &shader, None)
    }

    /// Creates a fullscreen pipeline with a custom shader using `vs_main` and
    /// `fs_main` entry points and the same source texture bind group layout.
    pub fn with_shader(
        device: &Device,
        format: TextureFormat,
        shader: &ShaderModule,
        label: Option<&str>,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Fullscreen Source Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Fullscreen Blit Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label,
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
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
        });
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("Fullscreen Blit Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
        }
    }

    /// Returns the source texture bind group layout.
    pub fn bind_group_layout(&self) -> &BindGroupLayout {
        &self.bind_group_layout
    }

    /// Creates a bind group for a source texture view.
    pub fn bind_group(&self, device: &Device, source: &TextureView) -> BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Fullscreen Blit Source Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Encodes a full-screen draw into `target`.
    pub fn render(
        &self,
        encoder: &mut CommandEncoder,
        target: &TextureView,
        source: &BindGroup,
        load: LoadOp<wgpu::Color>,
    ) {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Fullscreen Blit Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: Operations {
                    load,
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, source, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_texture_descriptor_clamps_zero_size_and_combines_usage() {
        let descriptor = RenderTextureDescriptor::new(0, 0, TextureFormat::Rgba8Unorm)
            .with_extra_usage(TextureUsages::COPY_SRC)
            .with_label("capture");

        assert_eq!(descriptor.clamped_size(), (1, 1));
        assert!(descriptor
            .usage()
            .contains(TextureUsages::RENDER_ATTACHMENT));
        assert!(descriptor.usage().contains(TextureUsages::TEXTURE_BINDING));
        assert!(descriptor.usage().contains(TextureUsages::COPY_SRC));
        assert_eq!(descriptor.label.as_deref(), Some("capture"));
    }

    #[test]
    fn fullscreen_blit_shader_parses() {
        naga::front::wgsl::parse_str(FULLSCREEN_BLIT_SHADER)
            .expect("builtin fullscreen blit shader should parse");
    }
}
