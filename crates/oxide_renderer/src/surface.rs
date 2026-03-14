//! Surface configuration and management

use wgpu::{
    PresentMode, Surface, SurfaceConfiguration, SurfaceError, SurfaceTexture, TextureFormat,
    TextureUsages,
};

pub struct SurfaceState {
    surface: Surface<'static>,
    config: SurfaceConfiguration,
}

impl SurfaceState {
    pub fn new(
        surface: Surface<'static>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Self {
        let caps = surface.get_capabilities(adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| matches!(f, TextureFormat::Rgba8Unorm | TextureFormat::Bgra8Unorm))
            .unwrap_or(caps.formats[0]);

        let present_mode = caps
            .present_modes
            .iter()
            .copied()
            .find(|m| *m == PresentMode::Mailbox)
            .unwrap_or(PresentMode::Fifo);

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(device, &config);

        Self { surface, config }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(device, &self.config);
        }
    }

    pub fn format(&self) -> TextureFormat {
        self.config.format
    }

    pub fn width(&self) -> u32 {
        self.config.width
    }

    pub fn height(&self) -> u32 {
        self.config.height
    }

    pub fn acquire(&self) -> Result<SurfaceTexture, SurfaceError> {
        self.surface.get_current_texture()
    }
}
