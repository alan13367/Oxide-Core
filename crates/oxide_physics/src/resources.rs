//! Physics world resources for the in-house simulation backend.

use std::collections::HashMap;

use glam::{Quat, Vec3};
use oxide_ecs::Resource;
use oxide_engine::prelude::Entity;

use crate::components::{BodyId, ColliderComponent, ColliderId, ColliderShape, RigidBodyType};

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
    pub mass: f32,
    pub inverse_mass: f32,
}

impl PhysicsBody {
    fn new(id: BodyId, entity: Entity, body_type: RigidBodyType) -> Self {
        let (mass, inverse_mass) = match body_type {
            RigidBodyType::Dynamic => (1.0, 1.0),
            _ => (0.0, 0.0),
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
            mass,
            inverse_mass,
        }
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self.body_type, RigidBodyType::Dynamic)
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

#[derive(Resource)]
pub struct PhysicsWorld {
    pub gravity: Vec3,
    pub bodies: HashMap<BodyId, PhysicsBody>,
    pub colliders: HashMap<ColliderId, PhysicsCollider>,
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
        self.bodies.insert(id, PhysicsBody::new(id, entity, body_type));
        id
    }

    pub fn create_collider(&mut self, body_id: BodyId, collider: ColliderComponent) -> ColliderId {
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
        }
    }

    pub fn set_body_linear_velocity(&mut self, id: BodyId, velocity: Vec3) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.linear_velocity = velocity;
        }
    }

    pub fn body_handles(&self) -> impl Iterator<Item = BodyId> + '_ {
        self.bodies.keys().copied()
    }

    pub fn collider_handles(&self) -> impl Iterator<Item = ColliderId> + '_ {
        self.colliders.keys().copied()
    }
}
