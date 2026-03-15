//! ECS components for the in-house physics runtime.

use glam::Vec3;
use oxide_ecs::Component;

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
}

impl RigidBodyComponent {
    pub fn new(body_type: RigidBodyType) -> Self {
        Self {
            body_type,
            handle: None,
            pending_initial_sync: true,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
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
