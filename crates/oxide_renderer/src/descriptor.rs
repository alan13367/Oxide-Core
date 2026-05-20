//! Material descriptor loading for shader-driven assets

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::shader::{BuiltinShader, ShaderSource};

pub const OXMAT_FORMAT: &str = "oxide.oxmat";
pub const OXMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialType {
    Lit,
    Unlit,
    Basic,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlphaMode {
    /// Render fully opaque with depth writes enabled.
    #[default]
    Opaque,
    /// Discard fragments with alpha below a fixed cutoff while keeping depth writes enabled.
    Mask,
    /// Blend source alpha over the framebuffer with depth writes disabled.
    Blend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ShaderDescriptor {
    Builtin { shader: String },
    File { path: String },
    Inline { wgsl: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialDescriptor {
    pub name: String,
    pub material_type: MaterialType,
    pub shader: ShaderDescriptor,
    pub fallback_shader: Option<String>,
    /// Base material color multiplied with each renderable's per-entity tint.
    #[serde(default = "default_base_color")]
    pub base_color: [f32; 4],
    /// Metallic response factor for lit materials.
    #[serde(default = "default_metallic_factor")]
    pub metallic_factor: f32,
    /// Roughness response factor for lit materials.
    #[serde(default = "default_roughness_factor")]
    pub roughness_factor: f32,
    /// Additive emissive color for lit and unlit materials.
    #[serde(default)]
    pub emissive_color: [f32; 3],
    /// Alpha compositing mode used by renderer-facing pipelines.
    #[serde(default)]
    pub alpha_mode: AlphaMode,
    /// Path to the albedo (diffuse) texture file.
    #[serde(default)]
    pub albedo_texture: Option<String>,
    /// Path to the normal map texture file.
    #[serde(default)]
    pub normal_texture: Option<String>,
    /// Path to the roughness texture file.
    #[serde(default)]
    pub roughness_texture: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OxMaterialDocument {
    pub format: String,
    pub version: u32,
    pub material: MaterialDescriptor,
}

fn default_base_color() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}

fn default_metallic_factor() -> f32 {
    0.0
}

fn default_roughness_factor() -> f32 {
    0.5
}

impl OxMaterialDocument {
    pub fn new(material: MaterialDescriptor) -> Self {
        Self {
            format: OXMAT_FORMAT.to_string(),
            version: OXMAT_VERSION,
            material,
        }
    }

    pub fn validate(self, path: String) -> Result<MaterialDescriptor, MaterialDescriptorError> {
        if self.format != OXMAT_FORMAT || self.version != OXMAT_VERSION {
            return Err(MaterialDescriptorError::UnsupportedVersion {
                path,
                format: self.format,
                version: self.version,
            });
        }

        Ok(self.material)
    }
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
    #[error(
        "Unsupported material descriptor version for '{path}': format '{format}' version {version}"
    )]
    UnsupportedVersion {
        path: String,
        format: String,
        version: u32,
    },
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
        "oxmat" => {
            let document = serde_json::from_str::<OxMaterialDocument>(&raw).map_err(|source| {
                MaterialDescriptorError::ParseJson {
                    path: path.display().to_string(),
                    source,
                }
            })?;
            document.validate(path.display().to_string())
        }
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

pub fn save_material_descriptor(
    path: impl AsRef<Path>,
    descriptor: &MaterialDescriptor,
) -> Result<(), MaterialDescriptorError> {
    let path = path.as_ref();
    let document = OxMaterialDocument::new(descriptor.clone());
    let raw = serde_json::to_string_pretty(&document).map_err(|source| {
        MaterialDescriptorError::ParseJson {
            path: path.display().to_string(),
            source,
        }
    })?;
    std::fs::write(path, raw).map_err(|source| MaterialDescriptorError::Io {
        path: path.display().to_string(),
        source,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn load_material_descriptor_accepts_legacy_json() {
        let path = temp_path("legacy_material", "json");
        fs::write(
            &path,
            r#"{
                "name": "Legacy",
                "material_type": "unlit",
                "shader": { "source": "builtin", "shader": "unlit" }
            }"#,
        )
        .unwrap();

        let material = load_material_descriptor(&path).unwrap();
        assert_eq!(material.name, "Legacy");
        assert_eq!(material.material_type, MaterialType::Unlit);
        assert_eq!(material.base_color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(material.metallic_factor, 0.0);
        assert_eq!(material.roughness_factor, 0.5);
        assert_eq!(material.emissive_color, [0.0, 0.0, 0.0]);
        assert_eq!(material.alpha_mode, AlphaMode::Opaque);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_material_descriptor_accepts_wrapped_oxmat() {
        let path = temp_path("wrapped_material", "oxmat");
        fs::write(
            &path,
            r#"{
                "format": "oxide.oxmat",
                "version": 1,
                "material": {
                    "name": "Wrapped",
                    "material_type": "lit",
                    "base_color": [0.8, 0.7, 0.6, 1.0],
                    "metallic_factor": 0.2,
                    "roughness_factor": 0.75,
                    "emissive_color": [0.1, 0.05, 0.0],
                    "alpha_mode": "mask",
                    "shader": { "source": "builtin", "shader": "lit" }
                }
            }"#,
        )
        .unwrap();

        let material = load_material_descriptor(&path).unwrap();
        assert_eq!(material.name, "Wrapped");
        assert_eq!(material.material_type, MaterialType::Lit);
        assert_eq!(material.base_color, [0.8, 0.7, 0.6, 1.0]);
        assert_eq!(material.metallic_factor, 0.2);
        assert_eq!(material.roughness_factor, 0.75);
        assert_eq!(material.emissive_color, [0.1, 0.05, 0.0]);
        assert_eq!(material.alpha_mode, AlphaMode::Mask);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_material_descriptor_accepts_alpha_blend_mode() {
        let path = temp_path("blend_material", "json");
        fs::write(
            &path,
            r#"{
                "name": "Glass",
                "material_type": "unlit",
                "base_color": [1.0, 1.0, 1.0, 0.35],
                "alpha_mode": "blend",
                "shader": { "source": "builtin", "shader": "unlit" }
            }"#,
        )
        .unwrap();

        let material = load_material_descriptor(&path).unwrap();
        assert_eq!(material.name, "Glass");
        assert_eq!(material.alpha_mode, AlphaMode::Blend);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_material_descriptor_rejects_unsupported_oxmat_version() {
        let path = temp_path("future_material", "oxmat");
        fs::write(
            &path,
            r#"{
                "format": "oxide.oxmat",
                "version": 99,
                "material": {
                    "name": "Future",
                    "material_type": "unlit",
                    "shader": { "source": "builtin", "shader": "unlit" }
                }
            }"#,
        )
        .unwrap();

        let err = load_material_descriptor(&path).unwrap_err();
        assert!(matches!(
            err,
            MaterialDescriptorError::UnsupportedVersion { .. }
        ));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_material_descriptor_writes_wrapped_roundtrip_document() {
        let path = temp_path("roundtrip_material", "oxmat");
        let material = MaterialDescriptor {
            name: "Roundtrip".to_string(),
            material_type: MaterialType::Unlit,
            shader: ShaderDescriptor::Builtin {
                shader: "unlit".to_string(),
            },
            fallback_shader: None,
            base_color: [0.3, 0.4, 0.5, 1.0],
            metallic_factor: 0.4,
            roughness_factor: 0.8,
            emissive_color: [0.0, 0.1, 0.2],
            alpha_mode: AlphaMode::Mask,
            albedo_texture: None,
            normal_texture: None,
            roughness_texture: None,
        };

        save_material_descriptor(&path, &material).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"format\": \"oxide.oxmat\""));
        assert!(raw.contains("\"metallic_factor\": 0.4"));
        assert!(raw.contains("\"alpha_mode\": \"mask\""));

        let loaded = load_material_descriptor(&path).unwrap();
        assert_eq!(loaded, material);
        let _ = fs::remove_file(path);
    }

    fn temp_path(name: &str, extension: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{name}_{stamp}.{extension}"))
    }
}
