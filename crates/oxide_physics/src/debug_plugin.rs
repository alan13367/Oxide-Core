//! Physics debug visualization plugin.
//!
//! Provides debug rendering for colliders, contact points, velocities,
//! and other physics visualization. Feature-gated behind "debug-render".

use glam::Mat4;
use oxide_ecs::Resource;
use oxide_engine::prelude::{Query, Res, ResMut};
use oxide_renderer::prelude::DebugLines;

use crate::components::{ColliderComponent, ColliderShape, RigidBodyComponent, RigidBodyType};
use crate::resources::PhysicsWorld;

/// Configuration for physics debug visualization.
#[derive(Clone, Debug, Resource)]
pub struct PhysicsDebugConfig {
    /// Draw collider wireframes.
    pub draw_colliders: bool,
    /// Draw contact points.
    pub draw_contacts: bool,
    /// Draw velocity vectors.
    pub draw_velocities: bool,
    /// Draw sleeping bodies differently.
    pub highlight_sleeping: bool,
    /// Draw AABBs for broadphase debugging.
    pub draw_aabbs: bool,
    /// Number of segments for sphere wireframes.
    pub sphere_segments: u32,
    /// Velocity arrow scale.
    pub velocity_scale: f32,
}

impl Default for PhysicsDebugConfig {
    fn default() -> Self {
        Self {
            draw_colliders: true,
            draw_contacts: true,
            draw_velocities: false,
            highlight_sleeping: true,
            draw_aabbs: false,
            sphere_segments: 16,
            velocity_scale: 0.2,
        }
    }
}

/// Colors for debug visualization.
pub mod debug_colors {
    use glam::Vec3;

    /// Dynamic body color (cyan).
    pub const DYNAMIC: Vec3 = Vec3::new(0.0, 1.0, 1.0);
    /// Static body color (gray).
    pub const STATIC: Vec3 = Vec3::new(0.5, 0.5, 0.5);
    /// Kinematic body color (yellow).
    pub const KINEMATIC: Vec3 = Vec3::new(1.0, 1.0, 0.0);
    /// Sleeping body color (dark cyan).
    pub const SLEEPING: Vec3 = Vec3::new(0.0, 0.3, 0.3);
    /// Contact point color (red).
    pub const CONTACT: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    /// Velocity vector color (green).
    pub const VELOCITY: Vec3 = Vec3::new(0.0, 1.0, 0.0);
    /// AABB color (magenta).
    pub const AABB: Vec3 = Vec3::new(1.0, 0.0, 1.0);
    /// Sensor color (blue).
    pub const SENSOR: Vec3 = Vec3::new(0.0, 0.5, 1.0);
}

/// System to draw physics debug visualization.
///
/// Call this after physics simulation but before rendering.
/// The debug lines will be drawn when `debug_lines.render()` is called.
pub fn physics_debug_render_system(
    config: Res<PhysicsDebugConfig>,
    physics: Res<PhysicsWorld>,
    mut debug_lines: ResMut<DebugLines>,
    mut rigid_body_query: Query<&RigidBodyComponent>,
    mut collider_query: Query<(&ColliderComponent, &RigidBodyComponent)>,
) {
    if !config.draw_colliders
        && !config.draw_contacts
        && !config.draw_velocities
        && !config.draw_aabbs
    {
        return;
    }

    // Draw colliders
    if config.draw_colliders {
        for (collider, rigid_body) in collider_query.iter() {
            let Some(body_id) = rigid_body.handle else {
                continue;
            };
            let Some(body) = physics.body(body_id) else {
                continue;
            };

            // Determine color based on body type and state
            let color = if collider.is_sensor {
                debug_colors::SENSOR
            } else if config.highlight_sleeping && body.is_sleeping {
                debug_colors::SLEEPING
            } else {
                match body.body_type {
                    RigidBodyType::Dynamic => debug_colors::DYNAMIC,
                    RigidBodyType::Static => debug_colors::STATIC,
                    _ => debug_colors::KINEMATIC,
                }
            };

            let transform = Mat4::from_rotation_translation(body.rotation, body.position);

            match collider.shape {
                ColliderShape::Sphere { radius } => {
                    debug_lines.draw_sphere(body.position, radius, color, config.sphere_segments);
                }
                ColliderShape::Cuboid { half_extents } => {
                    debug_lines.draw_box(transform, half_extents, color);
                }
            }
        }
    }

    // Draw contact points
    if config.draw_contacts {
        for manifold in physics.cached_manifolds.values() {
            for contact in &manifold.contacts {
                // Draw contact point as a small sphere
                debug_lines.draw_sphere(contact.position, 0.05, debug_colors::CONTACT, 8);

                // Draw contact normal
                let normal_end = contact.position + contact.normal * 0.2;
                debug_lines.draw_line(contact.position, normal_end, debug_colors::CONTACT);
            }
        }
    }

    // Draw velocity vectors
    if config.draw_velocities {
        for rigid_body in rigid_body_query.iter() {
            let Some(body_id) = rigid_body.handle else {
                continue;
            };
            let Some(body) = physics.body(body_id) else {
                continue;
            };

            if !body.is_dynamic() || body.is_sleeping {
                continue;
            }

            // Linear velocity
            if body.linear_velocity.length_squared() > 0.0001 {
                let vel_scaled = body.linear_velocity * config.velocity_scale;
                debug_lines.draw_arrow(body.position, vel_scaled, debug_colors::VELOCITY, 0.1);
            }
        }
    }

    // Draw AABBs
    if config.draw_aabbs {
        for collider in physics.colliders.values() {
            let Some(body) = physics.body(collider.body_id) else {
                continue;
            };
            let aabb =
                super::collision::shape_to_aabb(collider.shape, body.position, body.rotation);

            let center = (aabb.min + aabb.max) * 0.5;
            let half_extents = (aabb.max - aabb.min) * 0.5;
            let transform = Mat4::from_translation(center);
            debug_lines.draw_box(transform, half_extents, debug_colors::AABB);
        }
    }
}
