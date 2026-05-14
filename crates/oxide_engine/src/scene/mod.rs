//! Scene module - hierarchy, transforms, and rendering components

#[cfg(feature = "gltf-import")]
mod gltf_hierarchy;
mod mesh_renderer;

#[cfg(feature = "gltf-import")]
pub use gltf_hierarchy::*;
pub use mesh_renderer::*;
pub use oxide_transform::{
    attach_child, detach_child, mark_subtree_dirty, transform_propagate_system, Children,
    GlobalTransform, Parent, TransformComponent,
};
