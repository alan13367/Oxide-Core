//! Material descriptor loading for shader-driven assets

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::shader::{BuiltinShader, ShaderSource};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialType {
    Lit,
    Unlit,
    Basic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ShaderDescriptor {
    Builtin { shader: String },
    File { path: String },
    Inline { wgsl: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialDescriptor {
    pub name: String,
    pub material_type: MaterialType,
    pub shader: ShaderDescriptor,
    pub fallback_shader: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum MaterialDescriptorError {
    #[error("Failed to read descriptor '{path}': {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("Failed to parse JSON descriptor '{path}': {source}")]
    ParseJson {
        path: String,
        source: serde_json::Error,
    },
    #[error("Failed to parse RON descriptor '{path}': {source}")]
    ParseRon {
        path: String,
        source: Box<ron::error::SpannedError>,
    },
    #[error("Failed to parse TOML descriptor '{path}': {source}")]
    ParseToml {
        path: String,
        source: toml::de::Error,
    },
    #[error("Unsupported descriptor format '{ext}' for file '{path}'")]
    UnsupportedFormat { path: String, ext: String },
    #[error("Unknown builtin shader '{name}'")]
    UnknownBuiltinShader { name: String },
}

impl MaterialDescriptor {
    pub fn shader_source(&self) -> Result<ShaderSource, MaterialDescriptorError> {
        match &self.shader {
            ShaderDescriptor::Builtin { shader } => {
                Ok(ShaderSource::Builtin(parse_builtin_shader(shader)?))
            }
            ShaderDescriptor::File { path } => Ok(ShaderSource::File(path.into())),
            ShaderDescriptor::Inline { wgsl } => Ok(ShaderSource::WgslOwned(wgsl.clone())),
        }
    }

    pub fn fallback_shader(&self) -> Result<BuiltinShader, MaterialDescriptorError> {
        match &self.fallback_shader {
            Some(name) => parse_builtin_shader(name),
            None => Ok(BuiltinShader::Fallback),
        }
    }
}

pub fn load_material_descriptor(
    path: impl AsRef<Path>,
) -> Result<MaterialDescriptor, MaterialDescriptorError> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path).map_err(|source| MaterialDescriptorError::Io {
        path: path.display().to_string(),
        source,
    })?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "json" => serde_json::from_str(&raw).map_err(|source| MaterialDescriptorError::ParseJson {
            path: path.display().to_string(),
            source,
        }),
        "ron" => ron::from_str(&raw).map_err(|source| MaterialDescriptorError::ParseRon {
            path: path.display().to_string(),
            source: Box::new(source),
        }),
        "toml" => toml::from_str(&raw).map_err(|source| MaterialDescriptorError::ParseToml {
            path: path.display().to_string(),
            source,
        }),
        _ => Err(MaterialDescriptorError::UnsupportedFormat {
            path: path.display().to_string(),
            ext,
        }),
    }
}

fn parse_builtin_shader(name: &str) -> Result<BuiltinShader, MaterialDescriptorError> {
    match name.trim().to_ascii_lowercase().as_str() {
        "basic" => Ok(BuiltinShader::Basic),
        "lit" => Ok(BuiltinShader::Lit),
        "unlit" => Ok(BuiltinShader::Unlit),
        "sky_gradient" | "skygradient" | "sky" => Ok(BuiltinShader::SkyGradient),
        "sprite_ui" | "spriteui" | "ui" => Ok(BuiltinShader::SpriteUi),
        "fallback" => Ok(BuiltinShader::Fallback),
        _ => Err(MaterialDescriptorError::UnknownBuiltinShader {
            name: name.to_string(),
        }),
    }
}
