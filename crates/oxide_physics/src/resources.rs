//! Physics world resources for the in-house simulation backend.

use std::collections::HashMap;

use glam::{Mat3, Quat, Vec3};
use oxide_ecs::Resource;
use oxide_engine::prelude::Entity;

use crate::collision::ContactManifold;
use crate::components::{
    BodyId, ColliderComponent, ColliderId, ColliderShape, CollisionLayers, RigidBodyType,
};
use crate::mass_properties::{MassProperties, DEFAULT_DENSITY};

pub const DEFAULT_FIXED_TIMESTEP: f32 = 1.0 / 60.0;
pub const DEFAULT_MAX_SUBSTEPS: u32 = 8;

#[derive(Clone, Debug)]
pub struct PhysicsBody {
    pub id: BodyId,
    pub entity: Entity,
    pub body_type: RigidBodyType,
    pub position: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub force_accumulator: Vec3,
    pub torque_accumulator: Vec3,
    pub mass: f32,
    pub inverse_mass: f32,
    /// Local-space inertia tensor (constant for a given shape).
    pub local_inertia: Mat3,
    /// Inverse of local inertia tensor.
    pub local_inverse_inertia: Mat3,
    /// Cached world-space inverse inertia tensor.
    /// Updated when rotation changes.
    pub world_inverse_inertia: Mat3,
    /// Density in kg/m³.
    pub density: f32,
    /// Whether the body is sleeping (not simulated).
    pub is_sleeping: bool,
    /// Time the body has been below sleep threshold.
    pub sleep_timer: f32,
}

impl PhysicsBody {
    fn new(id: BodyId, entity: Entity, body_type: RigidBodyType) -> Self {
        let (mass, inverse_mass, local_inertia, local_inverse_inertia, world_inverse_inertia) =
            match body_type {
                RigidBodyType::Dynamic => {
                    // Default mass of 1.0 with unit inertia; will be updated from collider
                    (1.0, 1.0, Mat3::IDENTITY, Mat3::IDENTITY, Mat3::IDENTITY)
                }
                _ => (0.0, 0.0, Mat3::ZERO, Mat3::ZERO, Mat3::ZERO),
            };
        Self {
            id,
            entity,
            body_type,
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            force_accumulator: Vec3::ZERO,
            torque_accumulator: Vec3::ZERO,
            mass,
            inverse_mass,
            local_inertia,
            local_inverse_inertia,
            world_inverse_inertia,
            density: DEFAULT_DENSITY,
            is_sleeping: false,
            sleep_timer: 0.0,
        }
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self.body_type, RigidBodyType::Dynamic)
    }

    /// Check if the body can go to sleep.
    pub fn can_sleep(&self, linear_threshold: f32, angular_threshold: f32) -> bool {
        self.linear_velocity.length_squared() < linear_threshold * linear_threshold
            && self.angular_velocity.length_squared() < angular_threshold * angular_threshold
    }

    /// Wake the body from sleep.
    pub fn wake(&mut self) {
        self.is_sleeping = false;
        self.sleep_timer = 0.0;
    }

    /// Set the mass properties for this body.
    pub fn set_mass_properties(&mut self, props: &MassProperties) {
        self.mass = props.mass;
        self.inverse_mass = props.inverse_mass;
        self.local_inertia = props.local_inertia;
        self.local_inverse_inertia = props.local_inverse_inertia;
        self.update_world_inertia();
    }

    /// Update the world-space inverse inertia tensor.
    /// Call this after changing rotation.
    pub fn update_world_inertia(&mut self) {
        if self.inverse_mass <= f32::EPSILON {
            self.world_inverse_inertia = Mat3::ZERO;
            return;
        }
        let rot_mat = Mat3::from_quat(self.rotation);
        self.world_inverse_inertia = rot_mat * self.local_inverse_inertia * rot_mat.transpose();
    }

    /// Apply a force at the center of mass.
    pub fn apply_force(&mut self, force: Vec3) {
        self.force_accumulator += force;
        if force.length_squared() > f32::EPSILON {
            self.wake();
        }
    }

    /// Apply a force at a world-space point.
    /// This generates both linear force and torque.
    pub fn apply_force_at_point(&mut self, force: Vec3, point: Vec3) {
        self.force_accumulator += force;
        let r = point - self.position;
        self.torque_accumulator += r.cross(force);
        if force.length_squared() > f32::EPSILON {
            self.wake();
        }
    }

    /// Apply a torque (angular force).
    pub fn apply_torque(&mut self, torque: Vec3) {
        self.torque_accumulator += torque;
        if torque.length_squared() > f32::EPSILON {
            self.wake();
        }
    }

    /// Apply an impulse at the center of mass.
    pub fn apply_impulse(&mut self, impulse: Vec3) {
        self.linear_velocity += impulse * self.inverse_mass;
        if impulse.length_squared() > f32::EPSILON {
            self.wake();
        }
    }

    /// Apply an impulse at a world-space point.
    pub fn apply_impulse_at_point(&mut self, impulse: Vec3, point: Vec3) {
        self.linear_velocity += impulse * self.inverse_mass;
        let r = point - self.position;
        let torque = r.cross(impulse);
        self.angular_velocity += self.world_inverse_inertia * torque;
        if impulse.length_squared() > f32::EPSILON {
            self.wake();
        }
    }
}

#[derive(Clone, Debug)]
pub struct PhysicsCollider {
    pub id: ColliderId,
    pub body_id: BodyId,
    pub shape: ColliderShape,
    pub friction: f32,
    pub restitution: f32,
    pub is_sensor: bool,
    /// Collision layer configuration for filtering.
    pub collision_layers: CollisionLayers,
}

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }
}

/// Key for identifying a contact manifold between two colliders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManifoldKey {
    pub collider_a: ColliderId,
    pub collider_b: ColliderId,
}

impl ManifoldKey {
    pub fn new(collider_a: ColliderId, collider_b: ColliderId) -> Self {
        // Ensure consistent ordering
        if collider_a.0 <= collider_b.0 {
            Self {
                collider_a,
                collider_b,
            }
        } else {
            Self {
                collider_a: collider_b,
                collider_b: collider_a,
            }
        }
    }
}

#[derive(Resource)]
pub struct PhysicsWorld {
    pub gravity: Vec3,
    pub bodies: HashMap<BodyId, PhysicsBody>,
    pub colliders: HashMap<ColliderId, PhysicsCollider>,
    /// Cached contact manifolds from the previous frame, for warm starting.
    pub cached_manifolds: HashMap<ManifoldKey, ContactManifold>,
    pub fixed_dt: f32,
    pub accumulator_seconds: f32,
    pub max_substeps: u32,
    next_body_id: u64,
    next_collider_id: u64,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            bodies: HashMap::new(),
            colliders: HashMap::new(),
            cached_manifolds: HashMap::new(),
            fixed_dt: DEFAULT_FIXED_TIMESTEP,
            accumulator_seconds: 0.0,
            max_substeps: DEFAULT_MAX_SUBSTEPS,
            next_body_id: 1,
            next_collider_id: 1,
        }
    }
}

impl PhysicsWorld {
    pub fn contains_body(&self, id: BodyId) -> bool {
        self.bodies.contains_key(&id)
    }

    pub fn contains_collider(&self, id: ColliderId) -> bool {
        self.colliders.contains_key(&id)
    }

    pub fn body(&self, id: BodyId) -> Option<&PhysicsBody> {
        self.bodies.get(&id)
    }

    pub fn body_mut(&mut self, id: BodyId) -> Option<&mut PhysicsBody> {
        self.bodies.get_mut(&id)
    }

    pub fn create_body(&mut self, entity: Entity, body_type: RigidBodyType) -> BodyId {
        let id = BodyId(self.next_body_id);
        self.next_body_id = self.next_body_id.saturating_add(1);
        self.bodies
            .insert(id, PhysicsBody::new(id, entity, body_type));
        id
    }

    pub fn create_collider(
        &mut self,
        body_id: BodyId,
        collider: ColliderComponent,
        collision_layers: CollisionLayers,
    ) -> ColliderId {
        let id = ColliderId(self.next_collider_id);
        self.next_collider_id = self.next_collider_id.saturating_add(1);
        self.colliders.insert(
            id,
            PhysicsCollider {
                id,
                body_id,
                shape: collider.shape,
                friction: collider.friction,
                restitution: collider.restitution,
                is_sensor: collider.is_sensor,
                collision_layers,
            },
        );
        id
    }

    pub fn remove_body(&mut self, id: BodyId) {
        self.bodies.remove(&id);
        self.colliders.retain(|_, collider| collider.body_id != id);
    }

    pub fn remove_collider(&mut self, id: ColliderId) {
        self.colliders.remove(&id);
    }

    pub fn set_body_pose(&mut self, id: BodyId, position: Vec3, rotation: Quat) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.position = position;
            body.rotation = rotation;
            body.update_world_inertia();
            body.wake();
        }
    }

    pub fn set_body_linear_velocity(&mut self, id: BodyId, velocity: Vec3) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.linear_velocity = velocity;
            if velocity.length_squared() > f32::EPSILON {
                body.wake();
            }
        }
    }

    pub fn set_body_angular_velocity(&mut self, id: BodyId, velocity: Vec3) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.angular_velocity = velocity;
            if velocity.length_squared() > f32::EPSILON {
                body.wake();
            }
        }
    }

    pub fn body_handles(&self) -> impl Iterator<Item = BodyId> + '_ {
        self.bodies.keys().copied()
    }

    pub fn collider_handles(&self) -> impl Iterator<Item = ColliderId> + '_ {
        self.colliders.keys().copied()
    }
}
