//! Mesh renderer component

use bevy_ecs::prelude::Component;

use crate::prelude::Mesh3D;

#[derive(Component)]
pub struct MeshRenderer {
    pub mesh: Mesh3D,
}

impl MeshRenderer {
    pub fn new(mesh: Mesh3D) -> Self {
        Self { mesh }
    }
}