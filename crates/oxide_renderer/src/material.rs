//! Material and shader pipeline abstraction

use std::sync::OnceLock;

use wgpu::{BindGroup, BindGroupLayout, Device, RenderPipeline, TextureFormat};

use crate::descriptor::{MaterialDescriptor, MaterialDescriptorError, MaterialType};
use crate::pipeline::{create_lit_pipeline, create_shader, create_unlit_pipeline};
use crate::shader::{
    builtin_shader_source, load_shader_source, BuiltinShader, ShaderSource, ShaderSourceError,
};
use crate::texture::{FallbackTexture, Texture};

pub struct MaterialPipeline {
    pub name: String,
    pub pipeline: RenderPipeline,
    pub material_type: MaterialType,
    /// Bind group for material textures (albedo, normal, roughness).
    /// Uses fallback white texture if no texture was provided.
    /// None for Basic materials that don't use textures.
    pub bind_group: Option<BindGroup>,
}

/// Cached bind group layout for materials.
static MATERIAL_BIND_GROUP_LAYOUT: OnceLock<BindGroupLayout> = OnceLock::new();

#[derive(thiserror::Error, Debug)]
pub enum MaterialError {
    #[error(transparent)]
    ShaderSource(#[from] ShaderSourceError),
    #[error(transparent)]
    Descriptor(#[from] MaterialDescriptorError),
    #[error("Failed to load texture '{path}': {source}")]
    TextureLoad {
        path: String,
        source: crate::texture::TextureError,
    },
}

/// Returns the cached material bind group layout, creating it if necessary.
pub fn get_material_bind_group_layout(device: &Device) -> &'static BindGroupLayout {
    MATERIAL_BIND_GROUP_LAYOUT.get_or_init(|| {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Material Bind Group Layout"),
            entries: &[
                // Binding 0: Albedo texture
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // Binding 1: Albedo sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    })
}

impl MaterialPipeline {
    #[allow(clippy::too_many_arguments)]
    pub fn from_builtin(
        device: &Device,
        queue: &wgpu::Queue,
        format: TextureFormat,
        camera_layout: &BindGroupLayout,
        light_layout: &BindGroupLayout,
        shader: BuiltinShader,
        material_type: MaterialType,
        name: impl Into<String>,
    ) -> Self {
        let shader_src = builtin_shader_source(shader);
        let shader_module = create_shader(device, shader_src, Some("Builtin Material Shader"));

        let material_layout = get_material_bind_group_layout(device);

        let (pipeline, bind_group) = match material_type {
            MaterialType::Lit => {
                let pipeline = create_lit_pipeline(
                    device,
                    &shader_module,
                    format,
                    camera_layout,
                    material_layout,
                    light_layout,
                );
                let fallback = FallbackTexture::new(device, queue);
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Material Bind Group (Fallback)"),
                    layout: material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&fallback.texture.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&fallback.texture.sampler),
                        },
                    ],
                });
                (pipeline, Some(bind_group))
            }
            MaterialType::Unlit => {
                let pipeline = create_unlit_pipeline(
                    device,
                    &shader_module,
                    format,
                    camera_layout,
                    material_layout,
                );
                let fallback = FallbackTexture::new(device, queue);
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Material Bind Group (Fallback)"),
                    layout: material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&fallback.texture.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&fallback.texture.sampler),
                        },
                    ],
                });
                (pipeline, Some(bind_group))
            }
            MaterialType::Basic => {
                let pipeline =
                    crate::pipeline::create_basic_pipeline(device, &shader_module, format);
                (pipeline, None)
            }
        };

        Self {
            name: name.into(),
            pipeline,
            material_type,
            bind_group,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_source(
        device: &Device,
        queue: &wgpu::Queue,
        format: TextureFormat,
        camera_layout: &BindGroupLayout,
        light_layout: &BindGroupLayout,
        source: &ShaderSource,
        material_type: MaterialType,
        albedo_texture: Option<&Texture>,
        name: impl Into<String>,
    ) -> Result<Self, MaterialError> {
        let shader_src = load_shader_source(source)?;
        let shader_module = create_shader(device, &shader_src, Some("Custom Material Shader"));

        let material_layout = get_material_bind_group_layout(device);

        let (pipeline, bind_group) = match material_type {
            MaterialType::Lit => {
                let pipeline = create_lit_pipeline(
                    device,
                    &shader_module,
                    format,
                    camera_layout,
                    material_layout,
                    light_layout,
                );
                let bind_group =
                    create_material_bind_group(device, queue, material_layout, albedo_texture);
                (pipeline, Some(bind_group))
            }
            MaterialType::Unlit => {
                let pipeline = create_unlit_pipeline(
                    device,
                    &shader_module,
                    format,
                    camera_layout,
                    material_layout,
                );
                let bind_group =
                    create_material_bind_group(device, queue, material_layout, albedo_texture);
                (pipeline, Some(bind_group))
            }
            MaterialType::Basic => {
                let pipeline =
                    crate::pipeline::create_basic_pipeline(device, &shader_module, format);
                (pipeline, None)
            }
        };

        Ok(Self {
            name: name.into(),
            pipeline,
            material_type,
            bind_group,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_source_with_fallback(
        device: &Device,
        queue: &wgpu::Queue,
        format: TextureFormat,
        camera_layout: &BindGroupLayout,
        light_layout: &BindGroupLayout,
        source: &ShaderSource,
        fallback_shader: BuiltinShader,
        material_type: MaterialType,
        albedo_texture: Option<&Texture>,
        name: impl Into<String>,
    ) -> Self {
        let name = name.into();

        match Self::from_source(
            device,
            queue,
            format,
            camera_layout,
            light_layout,
            source,
            material_type,
            albedo_texture,
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
                    queue,
                    format,
                    camera_layout,
                    light_layout,
                    fallback_shader,
                    material_type,
                    name,
                )
            }
        }
    }

    pub fn from_descriptor(
        device: &Device,
        queue: &wgpu::Queue,
        format: TextureFormat,
        camera_layout: &BindGroupLayout,
        light_layout: &BindGroupLayout,
        descriptor: &MaterialDescriptor,
    ) -> Result<Self, MaterialError> {
        let source = descriptor.shader_source()?;
        let fallback = descriptor.fallback_shader()?;

        // Load albedo texture if provided
        let albedo_texture = if let Some(path) = &descriptor.albedo_texture {
            Some(Texture::from_file(device, queue, path).map_err(|source| {
                MaterialError::TextureLoad {
                    path: path.clone(),
                    source,
                }
            })?)
        } else {
            None
        };

        Ok(Self::from_source_with_fallback(
            device,
            queue,
            format,
            camera_layout,
            light_layout,
            &source,
            fallback,
            descriptor.material_type,
            albedo_texture.as_ref(),
            descriptor.name.clone(),
        ))
    }
}

/// Helper function to create a material bind group with optional texture.
fn create_material_bind_group(
    device: &Device,
    queue: &wgpu::Queue,
    layout: &BindGroupLayout,
    texture: Option<&Texture>,
) -> BindGroup {
    match texture {
        Some(tex) => device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&tex.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&tex.sampler),
                },
            ],
        }),
        None => {
            let fallback = FallbackTexture::new(device, queue);
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Material Bind Group (Fallback)"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&fallback.texture.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&fallback.texture.sampler),
                    },
                ],
            })
        }
    }
}
