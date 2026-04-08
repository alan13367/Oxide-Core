//! ECS components for the in-house physics runtime.

use glam::Vec3;
use oxide_ecs::Component;

use crate::mass_properties::DEFAULT_DENSITY;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColliderId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RigidBodyType {
    Dynamic,
    Static,
    KinematicPositionBased,
    KinematicVelocityBased,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct RigidBodyComponent {
    pub body_type: RigidBodyType,
    pub handle: Option<BodyId>,
    pub pending_initial_sync: bool,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    /// Density in kg/m³. Default is 1000.0 (approximately water).
    /// Used to calculate mass from collider volume.
    pub density: f32,
    /// Whether mass properties have been computed from collider(s).
    /// If false, mass will be auto-calculated from attached colliders.
    /// If true, mass is manually set and won't be recalculated.
    pub mass_properties_computed: bool,
}

impl RigidBodyComponent {
    pub fn new(body_type: RigidBodyType) -> Self {
        Self {
            body_type,
            handle: None,
            pending_initial_sync: true,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            density: DEFAULT_DENSITY,
            mass_properties_computed: false,
        }
    }

    pub fn dynamic() -> Self {
        Self::new(RigidBodyType::Dynamic)
    }

    pub fn static_body() -> Self {
        Self::new(RigidBodyType::Static)
    }

    pub fn kinematic_position_based() -> Self {
        Self::new(RigidBodyType::KinematicPositionBased)
    }

    pub fn kinematic_velocity_based() -> Self {
        Self::new(RigidBodyType::KinematicVelocityBased)
    }

    pub fn with_linear_velocity(mut self, velocity: Vec3) -> Self {
        self.linear_velocity = velocity;
        self
    }

    pub fn with_angular_velocity(mut self, velocity: Vec3) -> Self {
        self.angular_velocity = velocity;
        self
    }

    /// Set the density for mass calculation.
    /// Density is in kg/m³. Water is approximately 1000 kg/m³.
    pub fn with_density(mut self, density: f32) -> Self {
        self.density = density;
        self
    }
}

/// Collision layer configuration for filtering collisions.
///
/// Bodies can belong to multiple layers (group) and can collide with
/// specific layers (mask). This enables gameplay features like:
/// - Player-only areas
/// - Enemy-vs-player collision
/// - Trigger volumes
/// - Projectile filtering
#[derive(Component, Clone, Copy, Debug)]
pub struct CollisionLayers {
    /// Which layer(s) this body belongs to (bit flags).
    pub group: u16,
    /// Which layer(s) this body can collide with (bit flags).
    pub mask: u16,
}

impl Default for CollisionLayers {
    fn default() -> Self {
        // Default: belong to layer 0, collide with everything
        Self {
            group: collision_layer::DEFAULT,
            mask: collision_layer::ALL,
        }
    }
}

impl CollisionLayers {
    /// Create collision layers with the given group and mask.
    pub fn new(group: u16, mask: u16) -> Self {
        Self { group, mask }
    }

    /// Create a body in a single layer that collides with everything.
    pub fn in_layer(layer: u16) -> Self {
        Self {
            group: layer,
            mask: u16::MAX,
        }
    }

    /// Create a body that only collides with specific layers.
    pub fn collides_with(mut self, mask: u16) -> Self {
        self.mask = mask;
        self
    }

    /// Check if this body can collide with another based on their layers.
    pub fn can_collide_with(&self, other: &CollisionLayers) -> bool {
        // A collides with B if A's mask includes B's group AND B's mask includes A's group
        (self.mask & other.group) != 0 && (other.mask & self.group) != 0
    }
}

/// Common collision layer constants.
pub mod collision_layer {
    /// Default layer (all bodies).
    pub const DEFAULT: u16 = 1 << 0;
    /// Player layer.
    pub const PLAYER: u16 = 1 << 1;
    /// Enemy layer.
    pub const ENEMY: u16 = 1 << 2;
    /// Static geometry layer.
    pub const STATIC: u16 = 1 << 3;
    /// Trigger/sensor layer.
    pub const TRIGGER: u16 = 1 << 4;
    /// Projectile layer.
    pub const PROJECTILE: u16 = 1 << 5;
    /// Debris/particle layer.
    pub const DEBRIS: u16 = 1 << 6;
    /// All layers.
    pub const ALL: u16 = u16::MAX;
}

#[derive(Clone, Copy, Debug)]
pub enum ColliderShape {
    Cuboid { half_extents: Vec3 },
    Sphere { radius: f32 },
}

impl ColliderShape {
    pub fn cuboid(half_extents: Vec3) -> Self {
        Self::Cuboid { half_extents }
    }

    pub fn sphere(radius: f32) -> Self {
        Self::Sphere { radius }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ColliderComponent {
    pub shape: ColliderShape,
    pub handle: Option<ColliderId>,
    pub friction: f32,
    pub restitution: f32,
    pub is_sensor: bool,
}

impl ColliderComponent {
    pub fn new(shape: ColliderShape) -> Self {
        Self {
            shape,
            handle: None,
            friction: 0.7,
            restitution: 0.0,
            is_sensor: false,
        }
    }

    pub fn cuboid(half_extents: Vec3) -> Self {
        Self::new(ColliderShape::cuboid(half_extents))
    }

    pub fn sphere(radius: f32) -> Self {
        Self::new(ColliderShape::sphere(radius))
    }

    pub fn with_friction(mut self, friction: f32) -> Self {
        self.friction = friction;
        self
    }

    pub fn with_restitution(mut self, restitution: f32) -> Self {
        self.restitution = restitution;
        self
    }

    pub fn sensor(mut self, is_sensor: bool) -> Self {
        self.is_sensor = is_sensor;
        self
    }
}
