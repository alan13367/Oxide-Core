//! Oxide physics prelude.

pub use crate::character_controller::CharacterControllerComponent;
pub use crate::collision::{
    shape_to_aabb, CellKey, ContactId, ContactManifold, ContactPoint, SpatialHash, SpatialProxy,
};
pub use crate::components::{
    collision_layer, ColliderComponent, ColliderShape, CollisionLayers, RigidBodyComponent,
    RigidBodyType,
};
pub use crate::events::{CollisionEvent, CollisionEvents, EventContact};
pub use crate::joints::{JointComponent, JointType};
pub use crate::mass_properties::{MassProperties, DEFAULT_DENSITY};
pub use crate::plugin::PhysicsPlugin;
pub use crate::queries::{RaycastHit, ShapeCastHit};
pub use crate::resources::{
    ManifoldKey, PhysicsBody, PhysicsWorld, DEFAULT_FIXED_TIMESTEP, DEFAULT_MAX_SUBSTEPS,
};
pub use crate::systems::{
    compute_mass_properties_system, ensure_colliders_system, ensure_rigid_bodies_system,
    initialize_body_pose_system, physics_step_system, prune_orphan_bodies_system,
    prune_orphan_colliders_system, sync_transforms_system, SolverConfig,
};

#[cfg(feature = "debug-render")]
pub use crate::debug_plugin::{debug_colors, physics_debug_render_system, PhysicsDebugConfig};
