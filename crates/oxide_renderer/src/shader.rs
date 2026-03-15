//! Shader loading utilities

use std::path::{Path, PathBuf};
use std::sync::Arc;
use wgpu::{Device, ShaderModule};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinShader {
    Basic,
    Lit,
    Unlit,
    SkyGradient,
    SpriteUi,
    Fallback,
}

#[derive(Debug, Clone)]
pub enum ShaderSource {
    Builtin(BuiltinShader),
    Wgsl(&'static str),
    WgslOwned(String),
    File(PathBuf),
}

#[derive(thiserror::Error, Debug)]
pub enum ShaderSourceError {
    #[error("Failed to read shader file '{path}': {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

pub fn builtin_shader_source(shader: BuiltinShader) -> &'static str {
    match shader {
        BuiltinShader::Basic => BASIC_SHADER,
        BuiltinShader::Lit => LIT_SHADER,
        BuiltinShader::Unlit => UNLIT_SHADER,
        BuiltinShader::SkyGradient => SKY_GRADIENT_SHADER,
        BuiltinShader::SpriteUi => SPRITE_UI_SHADER,
        BuiltinShader::Fallback => FALLBACK_SHADER,
    }
}

pub fn load_shader_source(source: &ShaderSource) -> Result<String, ShaderSourceError> {
    match source {
        ShaderSource::Builtin(kind) => Ok(builtin_shader_source(*kind).to_string()),
        ShaderSource::Wgsl(src) => Ok((*src).to_string()),
        ShaderSource::WgslOwned(src) => Ok(src.clone()),
        ShaderSource::File(path) => {
            std::fs::read_to_string(path).map_err(|source| ShaderSourceError::Io {
                path: path.display().to_string(),
                source,
            })
        }
    }
}

pub fn load_shader_source_from_path(path: impl AsRef<Path>) -> Result<String, ShaderSourceError> {
    load_shader_source(&ShaderSource::File(path.as_ref().to_path_buf()))
}

pub fn load_wgsl(device: &Arc<Device>, source: &'static str) -> ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

pub const BASIC_SHADER: &str = include_str!("../shaders/basic.wgsl");
pub const LIT_SHADER: &str = include_str!("../shaders/lit.wgsl");
pub const UNLIT_SHADER: &str = include_str!("../shaders/unlit.wgsl");
pub const SKY_GRADIENT_SHADER: &str = include_str!("../shaders/sky_gradient.wgsl");
pub const SPRITE_UI_SHADER: &str = include_str!("../shaders/sprite_ui.wgsl");
pub const FALLBACK_SHADER: &str = include_str!("../shaders/fallback.wgsl");
