//! Engine-managed render frame utilities.

pub struct RenderFrame {
    pub view: wgpu::TextureView,
    pub encoder: wgpu::CommandEncoder,
    surface_texture: wgpu::SurfaceTexture,
}

impl RenderFrame {
    pub fn new(device: &wgpu::Device, surface_texture: wgpu::SurfaceTexture) -> Self {
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        Self {
            view,
            encoder,
            surface_texture,
        }
    }

    pub fn present(self, queue: &wgpu::Queue) {
        queue.submit(std::iter::once(self.encoder.finish()));
        self.surface_texture.present();
    }
}
