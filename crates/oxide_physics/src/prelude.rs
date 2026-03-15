//! Oxide physics prelude.

pub use crate::components::{ColliderComponent, ColliderShape, RigidBodyComponent, RigidBodyType};
pub use crate::math::{NaQuat, NaVec3};
pub use crate::plugin::PhysicsPlugin;
pub use crate::resources::{PhysicsWorld, DEFAULT_FIXED_TIMESTEP, DEFAULT_MAX_SUBSTEPS};
pub use crate::systems::{
    ensure_colliders_system, ensure_rigid_bodies_system, initialize_body_pose_system,
    physics_step_system, prune_orphan_bodies_system, prune_orphan_colliders_system,
    sync_transforms_system,
};
