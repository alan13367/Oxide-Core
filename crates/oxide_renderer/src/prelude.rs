//! Oxide Core renderer prelude

pub use crate::depth::DepthTexture;
pub use crate::descriptor::{
    load_material_descriptor, MaterialDescriptor, MaterialType, ShaderDescriptor,
};
pub use crate::gltf::{GltfError, GltfNode, GltfScene, load_gltf};
pub use crate::material::{get_material_bind_group_layout, MaterialError, MaterialPipeline};
pub use crate::mesh::{
    cube_indices, cube_vertices, sphere_indices, sphere_vertices, triangle_vertices, Mesh, Mesh3D,
    Vertex, Vertex3D,
};
pub use crate::pipeline::{
    create_basic_pipeline, create_lit_pipeline, create_shader, create_unlit_pipeline,
};
pub use crate::shader::{
    builtin_shader_source, load_shader_source, load_shader_source_from_path, load_wgsl,
    BuiltinShader, ShaderSource, ShaderSourceError, BASIC_SHADER, FALLBACK_SHADER, LIT_SHADER,
    SKY_GRADIENT_SHADER, SPRITE_UI_SHADER, UNLIT_SHADER,
};
pub use crate::surface::SurfaceState;
pub use crate::texture::{FallbackTexture, SamplerDescriptor, Texture, TextureError};
pub use crate::Renderer;
pub use wgpu::{
    Adapter, CommandEncoder, Device, Queue, RenderPass, RenderPipeline, SurfaceConfiguration,
    TextureFormat,
};
