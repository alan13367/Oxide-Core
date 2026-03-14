//! Material and shader pipeline abstraction

use wgpu::{BindGroupLayout, Device, RenderPipeline, TextureFormat};

use crate::descriptor::{MaterialDescriptor, MaterialDescriptorError, MaterialType};
use crate::pipeline::{create_lit_pipeline, create_shader, create_unlit_pipeline};
use crate::shader::{
    builtin_shader_source, load_shader_source, BuiltinShader, ShaderSource, ShaderSourceError,
};

pub struct MaterialPipeline {
    pub name: String,
    pub pipeline: RenderPipeline,
    pub material_type: MaterialType,
}

#[derive(thiserror::Error, Debug)]
pub enum MaterialError {
    #[error(transparent)]
    ShaderSource(#[from] ShaderSourceError),
    #[error(transparent)]
    Descriptor(#[from] MaterialDescriptorError),
}

impl MaterialPipeline {
    pub fn from_builtin(
        device: &Device,
        format: TextureFormat,
        camera_layout: &BindGroupLayout,
        shader: BuiltinShader,
        material_type: MaterialType,
        name: impl Into<String>,
    ) -> Self {
        let shader_src = builtin_shader_source(shader);
        let shader_module = create_shader(device, shader_src, Some("Builtin Material Shader"));

        let pipeline = match material_type {
            MaterialType::Lit => create_lit_pipeline(device, &shader_module, format, camera_layout),
            MaterialType::Unlit => {
                create_unlit_pipeline(device, &shader_module, format, camera_layout)
            }
            MaterialType::Basic => {
                crate::pipeline::create_basic_pipeline(device, &shader_module, format)
            }
        };

        Self {
            name: name.into(),
            pipeline,
            material_type,
        }
    }

    pub fn from_source(
        device: &Device,
        format: TextureFormat,
        camera_layout: &BindGroupLayout,
        source: &ShaderSource,
        material_type: MaterialType,
        name: impl Into<String>,
    ) -> Result<Self, MaterialError> {
        let shader_src = load_shader_source(source)?;
        let shader_module = create_shader(device, &shader_src, Some("Custom Material Shader"));

        let pipeline = match material_type {
            MaterialType::Lit => create_lit_pipeline(device, &shader_module, format, camera_layout),
            MaterialType::Unlit => {
                create_unlit_pipeline(device, &shader_module, format, camera_layout)
            }
            MaterialType::Basic => {
                crate::pipeline::create_basic_pipeline(device, &shader_module, format)
            }
        };

        Ok(Self {
            name: name.into(),
            pipeline,
            material_type,
        })
    }

    pub fn from_source_with_fallback(
        device: &Device,
        format: TextureFormat,
        camera_layout: &BindGroupLayout,
        source: &ShaderSource,
        fallback_shader: BuiltinShader,
        material_type: MaterialType,
        name: impl Into<String>,
    ) -> Self {
        let name = name.into();

        match Self::from_source(
            device,
            format,
            camera_layout,
            source,
            material_type,
            name.clone(),
        ) {
            Ok(material) => material,
            Err(err) => {
                tracing::warn!(
                    "Material '{}' failed to load custom shader, using fallback {:?}: {}",
                    name,
                    fallback_shader,
                    err
                );

                Self::from_builtin(
                    device,
                    format,
                    camera_layout,
                    fallback_shader,
                    material_type,
                    name,
                )
            }
        }
    }

    pub fn from_descriptor(
        device: &Device,
        format: TextureFormat,
        camera_layout: &BindGroupLayout,
        descriptor: &MaterialDescriptor,
    ) -> Result<Self, MaterialError> {
        let source = descriptor.shader_source()?;
        let fallback = descriptor.fallback_shader()?;

        Ok(Self::from_source_with_fallback(
            device,
            format,
            camera_layout,
            &source,
            fallback,
            descriptor.material_type,
            descriptor.name.clone(),
        ))
    }
}
