//! Depth texture for depth buffering

use wgpu::{Device, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages};

pub struct DepthTexture {
    pub texture: Texture,
    pub view: wgpu::TextureView,
}

impl DepthTexture {
    pub fn new(device: &Device, width: u32, height: u32, label: Option<&str>) -> Self {
        let texture = device.create_texture(&TextureDescriptor {
            label,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth24PlusStencil8,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self { texture, view }
    }

    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        *self = Self::new(device, width, height, Some("Depth Texture"));
    }
}
