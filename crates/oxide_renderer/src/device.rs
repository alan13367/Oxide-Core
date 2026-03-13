//! Device and Queue creation

use wgpu::{Device, DeviceDescriptor, ExperimentalFeatures, Features, Limits, Queue};

pub struct DeviceQueue {
    pub device: Device,
    pub queue: Queue,
}

pub async fn request_device(adapter: &wgpu::Adapter) -> Result<DeviceQueue, wgpu::RequestDeviceError> {
    let (device, queue) = adapter
        .request_device(&DeviceDescriptor {
            label: Some("Oxide Core Device"),
            required_features: Features::empty(),
            required_limits: Limits::default(),
            memory_hints: Default::default(),
            trace: Default::default(),
            experimental_features: ExperimentalFeatures::disabled(),
        })
        .await?;

    Ok(DeviceQueue { device, queue })
}