//! GPU adapter selection

use wgpu::{Backends, DeviceType, Instance, InstanceDescriptor, RequestAdapterOptions};

pub struct AdapterInfo {
    pub name: String,
    pub device_type: DeviceType,
    pub backend: wgpu::Backend,
}

pub fn create_instance() -> Instance {
    Instance::new(&InstanceDescriptor {
        backends: Backends::METAL,
        ..Default::default()
    })
}

pub async fn request_adapter(
    instance: &Instance,
    compatible_surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, wgpu::RequestAdapterError> {
    let options = RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface,
    };

    instance.request_adapter(&options).await
}

pub fn adapter_info(adapter: &wgpu::Adapter) -> AdapterInfo {
    let info = adapter.get_info();
    AdapterInfo {
        name: info.name.to_string(),
        device_type: info.device_type,
        backend: info.backend,
    }
}
