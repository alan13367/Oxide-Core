//! Scene components, descriptors, hierarchy helpers, and automatic rendering.

mod descriptor;
mod mesh_renderer;
mod renderer;
mod sprite;
mod terrain;

pub use descriptor::*;
pub use mesh_renderer::*;
pub use oxide_transform::{
    attach_child, detach_child, mark_subtree_dirty, transform_propagate_system, Children,
    GlobalTransform, Parent, TransformComponent,
};
pub use renderer::*;
pub use sprite::*;
pub use terrain::*;
