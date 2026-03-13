//! Shader loading utilities

use std::sync::Arc;
use wgpu::{Device, ShaderModule};

pub fn load_wgsl(device: &Arc<Device>, source: &'static str) -> ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

pub const BASIC_SHADER: &str = include_str!("../../../shaders/basic.wgsl");
pub const LIT_SHADER: &str = include_str!("../../../shaders/lit.wgsl");