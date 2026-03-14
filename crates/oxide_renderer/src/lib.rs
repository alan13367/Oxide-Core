//! Oxide Core renderer - wgpu-based rendering abstraction

pub mod adapter;
pub mod depth;
pub mod descriptor;
pub mod device;
pub mod gltf;
pub mod material;
pub mod mesh;
pub mod pipeline;
pub mod prelude;
pub mod shader;
pub mod surface;
pub mod texture;

pub use wgpu;

use std::sync::Arc;

use adapter::create_instance;
use device::DeviceQueue;
use surface::SurfaceState;

#[derive(thiserror::Error, Debug)]
pub enum RendererError {
    #[error("Failed to request adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("Failed to request device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("Failed to acquire surface texture: {0}")]
    Surface(#[from] wgpu::SurfaceError),
}

pub struct Renderer {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface: SurfaceState,
    #[allow(dead_code)]
    instance: Arc<wgpu::Instance>,
    #[allow(dead_code)]
    adapter: Arc<wgpu::Adapter>,
}

impl Renderer {
    pub async fn new(window: Arc<winit::window::Window>) -> Result<Self, RendererError> {
        let instance = Arc::new(create_instance());

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");

        let adapter = Arc::new(adapter::request_adapter(&instance, Some(&surface)).await?);

        tracing::info!(
            "Using GPU: {} ({:?})",
            adapter::adapter_info(&adapter).name,
            adapter::adapter_info(&adapter).device_type
        );

        let DeviceQueue { device, queue } = device::request_device(&adapter).await?;

        let size = window.inner_size();
        let surface = SurfaceState::new(surface, &adapter, &device, size.width, size.height);

        Ok(Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            surface,
            instance,
            adapter,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface.resize(&self.device, width, height);
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.surface.format()
    }

    pub fn width(&self) -> u32 {
        self.surface.width()
    }

    pub fn height(&self) -> u32 {
        self.surface.height()
    }

    pub fn begin_frame(&self) -> Result<wgpu::SurfaceTexture, RendererError> {
        self.surface.acquire().map_err(RendererError::from)
    }

    pub fn submit(&self, command_buffers: Vec<wgpu::CommandBuffer>) {
        self.queue.submit(command_buffers);
    }
}
