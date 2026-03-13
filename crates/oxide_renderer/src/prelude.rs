//! Oxide Core renderer prelude

pub use crate::depth::DepthTexture;
pub use crate::mesh::{Mesh, Mesh3D, Vertex, Vertex3D, triangle_vertices, cube_vertices, cube_indices, sphere_vertices, sphere_indices};
pub use crate::pipeline::{create_basic_pipeline, create_lit_pipeline, create_shader};
pub use crate::shader::{load_wgsl, BASIC_SHADER, LIT_SHADER};
pub use crate::surface::SurfaceState;
pub use crate::Renderer;
pub use wgpu::{
    Adapter, CommandEncoder, Device, Queue, RenderPass, RenderPipeline, SurfaceConfiguration,
    TextureFormat,
};