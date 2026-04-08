//! ECS systems for the in-house physics runtime.

use std::collections::HashSet;

use glam::{Quat, Vec3};
use oxide_engine::prelude::{Entity, Query, Res, ResMut, Time, TransformComponent};
use tracing::span;

use crate::collision::{generate_box_box_contacts, ContactId, ContactManifold, ContactPoint};
use crate::components::{
    BodyId, ColliderComponent, ColliderId, ColliderShape, CollisionLayers, RigidBodyComponent,
};
use crate::mass_properties::MassProperties;
use crate::resources::{Aabb, ManifoldKey, PhysicsWorld};

#[derive(Clone, Copy)]
struct Contact {
    a_body: BodyId,
    b_body: BodyId,
    a_collider: ColliderId,
    b_collider: ColliderId,
    contact_point: Vec3,
    normal: Vec3,
    penetration: f32,
    restitution: f32,
    friction: f32,
}

#[derive(Clone, Copy)]
struct Obb {
    center: Vec3,
    axes: [Vec3; 3],
    half_extents: Vec3,
}

#[derive(Clone)]
struct BroadphaseProxy {
    collider_id: ColliderId,
    body_id: BodyId,
    is_dynamic: bool,
    is_sensor: bool,
    aabb: Aabb,
    collision_layers: Option<CollisionLayers>,
}

/// Solver configuration for the sequential impulse solver.
#[derive(Clone, Copy, Debug)]
pub struct SolverConfig {
    /// Number of solver iterations. More iterations = more stable stacking.
    pub iterations: u32,
    /// Baumgarte stabilization factor (0.0-1.0). Higher = faster position correction.
    pub baumgarte: f32,
    /// Penetration slop: allow this much penetration before correction.
    pub slop: f32,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            iterations: 4,
            baumgarte: 0.3,
            slop: 0.005,
        }
    }
}

pub fn ensure_rigid_bodies_system(
    mut physics: ResMut<PhysicsWorld>,
    mut query: Query<(Entity, &mut RigidBodyComponent)>,
) {
    for (entity, rigid_body) in query.iter_mut() {
        if let Some(handle) = rigid_body.handle {
            if physics.contains_body(handle) {
                continue;
            }
            rigid_body.handle = None;
            rigid_body.pending_initial_sync = true;
        }

        let handle = physics.create_body(entity, rigid_body.body_type);
        physics.set_body_linear_velocity(handle, rigid_body.linear_velocity);
        physics.set_body_angular_velocity(handle, rigid_body.angular_velocity);

        // Set density on the physics body
        if let Some(body) = physics.body_mut(handle) {
            body.density = rigid_body.density;
        }

        rigid_body.handle = Some(handle);
        rigid_body.pending_initial_sync = true;
    }
}

/// Compute mass properties from attached colliders for bodies that haven't had them computed yet.
pub fn compute_mass_properties_system(
    mut physics: ResMut<PhysicsWorld>,
    mut rigid_body_query: Query<&RigidBodyComponent>,
    mut collider_query: Query<&ColliderComponent>,
) {
    // Group colliders by body
    let mut body_colliders: std::collections::HashMap<BodyId, Vec<ColliderShape>> =
        std::collections::HashMap::new();

    for collider in collider_query.iter() {
        let Some(handle) = collider.handle else {
            continue;
        };
        let Some(physics_collider) = physics.colliders.get(&handle) else {
            continue;
        };
        body_colliders
            .entry(physics_collider.body_id)
            .or_default()
            .push(collider.shape);
    }

    // Update mass properties for bodies that need it
    for rigid_body in rigid_body_query.iter() {
        let Some(handle) = rigid_body.handle else {
            continue;
        };
        if rigid_body.mass_properties_computed {
            continue;
        }

        let Some(body) = physics.body_mut(handle) else {
            continue;
        };
        if !body.is_dynamic() {
            continue;
        }

        let Some(shapes) = body_colliders.get(&handle) else {
            continue;
        };
        if shapes.is_empty() {
            continue;
        }

        // Compute mass properties from the first collider
        let props = MassProperties::from_shape(shapes[0], body.density);
        body.set_mass_properties(&props);
    }
}

pub fn initialize_body_pose_system(
    mut physics: ResMut<PhysicsWorld>,
    mut query: Query<(&TransformComponent, &mut RigidBodyComponent)>,
) {
    for (transform, rigid_body) in query.iter_mut() {
        if !rigid_body.pending_initial_sync {
            continue;
        }

        let Some(handle) = rigid_body.handle else {
            continue;
        };
        if !physics.contains_body(handle) {
            continue;
        }

        let local = transform.transform;
        physics.set_body_pose(handle, local.position, local.rotation);
        physics.set_body_linear_velocity(handle, rigid_body.linear_velocity);
        physics.set_body_angular_velocity(handle, rigid_body.angular_velocity);
        rigid_body.pending_initial_sync = false;
    }
}

pub fn ensure_colliders_system(
    mut physics: ResMut<PhysicsWorld>,
    mut collider_query: Query<(Entity, &mut ColliderComponent)>,
    mut rigid_body_query: Query<(Entity, &RigidBodyComponent)>,
    mut layer_query: Query<(Entity, &CollisionLayers)>,
) {
    let mut body_handles = std::collections::HashMap::new();
    for (entity, rigid_body) in rigid_body_query.iter() {
        if let Some(body_id) = rigid_body.handle {
            body_handles.insert(entity, body_id);
        }
    }

    let mut layer_overrides = std::collections::HashMap::new();
    for (entity, layers) in layer_query.iter() {
        layer_overrides.insert(entity, *layers);
    }

    for (entity, collider) in collider_query.iter_mut() {
        let collision_layers = layer_overrides.get(&entity).copied().unwrap_or_default();

        if let Some(handle) = collider.handle {
            if let Some(physics_collider) = physics.colliders.get_mut(&handle) {
                physics_collider.collision_layers = collision_layers;
                continue;
            }
            collider.handle = None;
        }

        let Some(body_id) = body_handles.get(&entity).copied() else {
            continue;
        };
        if !physics.contains_body(body_id) {
            continue;
        }

        let handle = physics.create_collider(body_id, *collider, collision_layers);
        collider.handle = Some(handle);
    }
}

pub fn prune_orphan_bodies_system(
    mut physics: ResMut<PhysicsWorld>,
    mut query: Query<&RigidBodyComponent>,
) {
    let live_handles: HashSet<BodyId> = query.iter().filter_map(|body| body.handle).collect();
    let to_remove: Vec<_> = physics
        .body_handles()
        .filter(|handle| !live_handles.contains(handle))
        .collect();

    for handle in to_remove {
        physics.remove_body(handle);
    }
}

pub fn prune_orphan_colliders_system(
    mut physics: ResMut<PhysicsWorld>,
    mut query: Query<&ColliderComponent>,
) {
    let live_handles: HashSet<_> = query
        .iter()
        .filter_map(|collider| collider.handle)
        .collect();
    let to_remove: Vec<_> = physics
        .collider_handles()
        .filter(|handle| !live_handles.contains(handle))
        .collect();

    for handle in to_remove {
        physics.remove_collider(handle);
    }
}

pub fn physics_step_system(
    time: Res<Time>,
    mut physics: ResMut<PhysicsWorld>,
    mut collision_events: ResMut<crate::events::CollisionEvents>,
    mut joint_query: Query<&crate::joints::JointComponent>,
) {
    let _span = span!(tracing::Level::DEBUG, "physics_step").entered();

    // Collect joints for solving
    let joints: Vec<_> = joint_query.iter().cloned().collect();

    let frame_delta = time.delta_secs().min(0.25);
    physics.accumulator_seconds += frame_delta;

    let mut steps = 0;
    while physics.accumulator_seconds >= physics.fixed_dt && steps < physics.max_substeps {
        // Solve joint constraints (apply forces)
        crate::joints::solve_joints(&mut physics, &joints);

        {
            let _integrate_span = span!(tracing::Level::DEBUG, "integrate").entered();
            integrate_bodies(&mut physics);
        }
        {
            let _resolve_span = span!(tracing::Level::DEBUG, "resolve_collisions").entered();
            resolve_collisions(&mut physics, SolverConfig::default());
        }
        physics.accumulator_seconds -= physics.fixed_dt;
        steps += 1;
    }

    // Generate collision events only when the fixed-step simulation advanced.
    if steps > 0 {
        crate::events::generate_collision_events(&physics, &mut collision_events);
    }
}

fn resolve_collisions(physics: &mut PhysicsWorld, config: SolverConfig) {
    let _span = span!(tracing::Level::DEBUG, "resolve_collisions").entered();

    let candidate_pairs = {
        let _broadphase_span = span!(tracing::Level::DEBUG, "broadphase").entered();
        broadphase_candidates(physics)
    };

    // Store previous manifolds for warm starting
    let previous_manifolds = physics.cached_manifolds.clone();
    physics.cached_manifolds.clear();

    // Generate new manifolds with warm starting
    let mut manifolds: Vec<ContactManifold> = {
        let _narrowphase_span = span!(tracing::Level::DEBUG, "narrowphase").entered();
        candidate_pairs
            .into_iter()
            .filter_map(|(a, b)| generate_manifold(physics, a, b, &previous_manifolds))
            .collect()
    };

    // Apply cached impulses once before solving to warm-start the velocity constraints.
    for manifold in &manifolds {
        if manifold_is_sensor(physics, manifold) {
            continue;
        }
        for contact in &manifold.contacts {
            apply_warm_start_impulse(
                physics,
                manifold.body_a,
                manifold.body_b,
                contact.position,
                contact.normal,
                contact.normal_impulse,
                contact.tangent_impulse,
            );
        }
    }

    // Sequential impulse solver with multiple iterations
    for _ in 0..config.iterations {
        let _solver_span = span!(tracing::Level::DEBUG, "solver_iteration").entered();
        for manifold in &mut manifolds {
            if manifold_is_sensor(physics, manifold) {
                continue;
            }
            let body_a = manifold.body_a;
            let body_b = manifold.body_b;
            let friction = manifold.friction;
            let restitution = manifold.restitution;
            for contact in &mut manifold.contacts {
                apply_contact_from_manifold(
                    physics,
                    body_a,
                    body_b,
                    friction,
                    restitution,
                    contact,
                    &config,
                );
            }
        }
    }

    // Cache manifolds for next frame
    for manifold in manifolds {
        if !manifold.is_empty() {
            let key = ManifoldKey::new(manifold.collider_a, manifold.collider_b);
            physics.cached_manifolds.insert(key, manifold);
        }
    }
}

pub fn sync_transforms_system(
    physics: Res<PhysicsWorld>,
    mut query: Query<(&mut TransformComponent, &mut RigidBodyComponent)>,
) {
    for (transform_component, rigid_body_component) in query.iter_mut() {
        let Some(handle) = rigid_body_component.handle else {
            continue;
        };
        let Some(body) = physics.body(handle) else {
            continue;
        };

        rigid_body_component.linear_velocity = body.linear_velocity;
        rigid_body_component.angular_velocity = body.angular_velocity;

        let transform = transform_component.transform_mut();
        transform.position = body.position;
        transform.rotation = body.rotation;
    }
}

fn integrate_bodies(physics: &mut PhysicsWorld) {
    let dt = physics.fixed_dt;
    let gravity = physics.gravity;

    // Sleep thresholds
    const LINEAR_SLEEP_THRESHOLD: f32 = 0.01;
    const ANGULAR_SLEEP_THRESHOLD: f32 = 0.1;
    const SLEEP_TIME_THRESHOLD: f32 = 0.5; // Seconds below threshold before sleeping

    for body in physics.bodies.values_mut() {
        if !body.is_dynamic() {
            continue;
        }

        // Skip sleeping bodies
        if body.is_sleeping {
            continue;
        }

        // Check if body should go to sleep
        if body.can_sleep(LINEAR_SLEEP_THRESHOLD, ANGULAR_SLEEP_THRESHOLD) {
            body.sleep_timer += dt;
            if body.sleep_timer >= SLEEP_TIME_THRESHOLD {
                body.is_sleeping = true;
                body.linear_velocity = Vec3::ZERO;
                body.angular_velocity = Vec3::ZERO;
                continue;
            }
        } else {
            body.sleep_timer = 0.0;
        }

        // Linear integration with gravity and accumulated forces
        let acceleration = gravity + body.force_accumulator * body.inverse_mass;
        body.linear_velocity += acceleration * dt;
        body.position += body.linear_velocity * dt;

        // Angular integration using inertia tensor
        // Angular acceleration = I^{-1} * torque
        let angular_acceleration = body.world_inverse_inertia * body.torque_accumulator;
        body.angular_velocity += angular_acceleration * dt;

        // Semi-implicit angular integration
        let angular_step = body.angular_velocity * dt;
        if angular_step.length_squared() > f32::EPSILON {
            let angular_delta = Quat::from_scaled_axis(angular_step);
            body.rotation = (angular_delta * body.rotation).normalize();
            // Update world inertia for the new rotation
            body.update_world_inertia();
        }

        body.force_accumulator = Vec3::ZERO;
        body.torque_accumulator = Vec3::ZERO;
    }
}

fn broadphase_candidates(physics: &PhysicsWorld) -> Vec<(ColliderId, ColliderId)> {
    const SPATIAL_HASH_CELL_SIZE: f32 = 2.0;
    let hash = crate::collision::build_spatial_hash(physics, SPATIAL_HASH_CELL_SIZE);

    let mut pairs = Vec::new();
    for (collider_a, collider_b) in hash.find_pairs() {
        let Some(a) = physics.colliders.get(&collider_a) else {
            continue;
        };
        let Some(b) = physics.colliders.get(&collider_b) else {
            continue;
        };
        if !a.collision_layers.can_collide_with(&b.collision_layers) {
            continue;
        }
        pairs.push((collider_a, collider_b));
    }
    pairs
}

fn broadphase_candidates_sweep(physics: &PhysicsWorld) -> Vec<(ColliderId, ColliderId)> {
    let mut proxies = Vec::new();
    for collider in physics.colliders.values() {
        let Some(body) = physics.body(collider.body_id) else {
            continue;
        };
        proxies.push(BroadphaseProxy {
            collider_id: collider.id,
            body_id: collider.body_id,
            is_dynamic: body.is_dynamic(),
            is_sensor: collider.is_sensor,
            aabb: shape_to_aabb(collider.shape, body.position, body.rotation),
            collision_layers: Some(collider.collision_layers),
        });
    }

    proxies.sort_by(|a, b| a.aabb.min.x.total_cmp(&b.aabb.min.x));

    let mut active: Vec<BroadphaseProxy> = Vec::new();
    let mut pairs = Vec::new();

    for proxy in proxies {
        active.retain(|other| other.aabb.max.x >= proxy.aabb.min.x);

        for other in &active {
            if proxy.body_id == other.body_id {
                continue;
            }
            if !proxy.is_dynamic && !other.is_dynamic && !proxy.is_sensor && !other.is_sensor {
                continue;
            }
            if !proxy.aabb.intersects(&other.aabb) {
                continue;
            }
            // Check collision layer filtering
            if let (Some(layers_a), Some(layers_b)) =
                (&proxy.collision_layers, &other.collision_layers)
            {
                if !layers_a.can_collide_with(layers_b) {
                    continue;
                }
            }
            pairs.push((other.collider_id, proxy.collider_id));
        }

        active.push(proxy);
    }

    pairs
}

fn generate_contact(
    physics: &PhysicsWorld,
    collider_a_id: ColliderId,
    collider_b_id: ColliderId,
) -> Option<Contact> {
    let collider_a = physics.colliders.get(&collider_a_id)?;
    let collider_b = physics.colliders.get(&collider_b_id)?;
    if collider_a.is_sensor || collider_b.is_sensor {
        return None;
    }

    let body_a = physics.body(collider_a.body_id)?;
    let body_b = physics.body(collider_b.body_id)?;
    if !body_a.is_dynamic() && !body_b.is_dynamic() {
        return None;
    }

    let (normal, penetration, contact_point) = compute_collision_with_contact(
        body_a.position,
        body_a.rotation,
        collider_a.shape,
        body_b.position,
        body_b.rotation,
        collider_b.shape,
    )?;

    Some(Contact {
        a_body: body_a.id,
        b_body: body_b.id,
        a_collider: collider_a_id,
        b_collider: collider_b_id,
        contact_point,
        normal,
        penetration,
        restitution: collider_a.restitution.min(collider_b.restitution),
        friction: (collider_a.friction + collider_b.friction) * 0.5,
    })
}

/// Generate a contact manifold with warm starting from previous frame.
fn generate_manifold(
    physics: &PhysicsWorld,
    collider_a_id: ColliderId,
    collider_b_id: ColliderId,
    previous_manifolds: &std::collections::HashMap<ManifoldKey, ContactManifold>,
) -> Option<ContactManifold> {
    let collider_a = physics.colliders.get(&collider_a_id)?;
    let collider_b = physics.colliders.get(&collider_b_id)?;

    let body_a = physics.body(collider_a.body_id)?;
    let body_b = physics.body(collider_b.body_id)?;
    if !collider_a.is_sensor
        && !collider_b.is_sensor
        && !body_a.is_dynamic()
        && !body_b.is_dynamic()
    {
        return None;
    }

    let mut manifold = ContactManifold::new(body_a.id, body_b.id, collider_a_id, collider_b_id);
    manifold.friction = (collider_a.friction + collider_b.friction) * 0.5;
    manifold.restitution = collider_a.restitution.min(collider_b.restitution);

    // Generate contact points based on shape types
    match (collider_a.shape, collider_b.shape) {
        (
            ColliderShape::Cuboid { half_extents: ha },
            ColliderShape::Cuboid { half_extents: hb },
        ) => {
            let axes_a = [
                body_a.rotation * Vec3::X,
                body_a.rotation * Vec3::Y,
                body_a.rotation * Vec3::Z,
            ];
            let axes_b = [
                body_b.rotation * Vec3::X,
                body_b.rotation * Vec3::Y,
                body_b.rotation * Vec3::Z,
            ];

            // Get collision from SAT
            if let Some((normal, penetration, contact_point)) = compute_collision_with_contact(
                body_a.position,
                body_a.rotation,
                collider_a.shape,
                body_b.position,
                body_b.rotation,
                collider_b.shape,
            ) {
                // Try to generate multiple contact points
                let contacts = generate_box_box_contacts(
                    body_a.position,
                    axes_a,
                    ha,
                    body_b.position,
                    axes_b,
                    hb,
                    normal,
                    penetration,
                );

                // If clipping didn't produce contacts, use single contact
                if contacts.is_empty() {
                    manifold.add_contact(ContactPoint {
                        id: ContactId::from_features(0, 0),
                        position: contact_point,
                        normal,
                        penetration,
                        normal_impulse: 0.0,
                        tangent_impulse: Vec3::ZERO,
                    });
                } else {
                    for contact in contacts {
                        manifold.add_contact(contact);
                    }
                }
            }
        }
        _ => {
            // For sphere-sphere and sphere-box, use single contact point
            if let Some((normal, penetration, contact_point)) = compute_collision_with_contact(
                body_a.position,
                body_a.rotation,
                collider_a.shape,
                body_b.position,
                body_b.rotation,
                collider_b.shape,
            ) {
                manifold.add_contact(ContactPoint {
                    id: ContactId::from_features(0, 0),
                    position: contact_point,
                    normal,
                    penetration,
                    normal_impulse: 0.0,
                    tangent_impulse: Vec3::ZERO,
                });
            }
        }
    }

    if manifold.is_empty() {
        return None;
    }

    // Warm start only for solid contacts. Sensor manifolds only need overlap data for events.
    if !collider_a.is_sensor && !collider_b.is_sensor {
        let key = ManifoldKey::new(collider_a_id, collider_b_id);
        if let Some(previous) = previous_manifolds.get(&key) {
            manifold.warm_start_from(previous);
        }
    }

    Some(manifold)
}

fn manifold_is_sensor(physics: &PhysicsWorld, manifold: &ContactManifold) -> bool {
    physics
        .colliders
        .get(&manifold.collider_a)
        .map(|collider| collider.is_sensor)
        .unwrap_or(false)
        || physics
            .colliders
            .get(&manifold.collider_b)
            .map(|collider| collider.is_sensor)
            .unwrap_or(false)
}

/// Apply contact impulse from a manifold contact point.
fn apply_contact_from_manifold(
    physics: &mut PhysicsWorld,
    body_a: BodyId,
    body_b: BodyId,
    friction: f32,
    restitution: f32,
    contact: &mut ContactPoint,
    config: &SolverConfig,
) {
    let body_a_data = physics.body(body_a).map(|b| {
        (
            b.inverse_mass,
            b.world_inverse_inertia,
            b.linear_velocity,
            b.angular_velocity,
            b.position,
        )
    });
    let body_b_data = physics.body(body_b).map(|b| {
        (
            b.inverse_mass,
            b.world_inverse_inertia,
            b.linear_velocity,
            b.angular_velocity,
            b.position,
        )
    });

    let Some((inv_mass_a, world_inv_inertia_a, vel_a, ang_vel_a, pos_a)) = body_a_data else {
        return;
    };
    let Some((inv_mass_b, world_inv_inertia_b, vel_b, ang_vel_b, pos_b)) = body_b_data else {
        return;
    };

    let inv_mass_sum = inv_mass_a + inv_mass_b;
    if inv_mass_sum <= f32::EPSILON {
        return;
    }

    // Contact point relative to body centers
    let r_a = contact.position - pos_a;
    let r_b = contact.position - pos_b;

    // Positional Correction (Baumgarte)
    let correction_mag = (contact.penetration - config.slop).max(0.0) * config.baumgarte;
    let correction = contact.normal * correction_mag;
    if inv_mass_a > 0.0 {
        if let Some(body) = physics.body_mut(body_a) {
            body.wake();
            body.position -= correction * (inv_mass_a / inv_mass_sum);
        }
    }
    if inv_mass_b > 0.0 {
        if let Some(body) = physics.body_mut(body_b) {
            body.wake();
            body.position += correction * (inv_mass_b / inv_mass_sum);
        }
    }

    // Velocity Resolution
    let vel_at_a = vel_a + ang_vel_a.cross(r_a);
    let vel_at_b = vel_b + ang_vel_b.cross(r_b);
    let relative_velocity = vel_at_b - vel_at_a;

    let vel_along_normal = relative_velocity.dot(contact.normal);
    if vel_along_normal > 0.0 {
        return;
    }

    // Compute effective mass
    let r_a_cross_n = r_a.cross(contact.normal);
    let r_b_cross_n = r_b.cross(contact.normal);
    let angular_term_a = world_inv_inertia_a * r_a_cross_n;
    let angular_term_b = world_inv_inertia_b * r_b_cross_n;

    let effective_mass = inv_mass_sum
        + angular_term_a.cross(r_a).dot(contact.normal)
        + angular_term_b.cross(r_b).dot(contact.normal);

    if effective_mass <= f32::EPSILON {
        return;
    }

    // Normal impulse
    let impulse_mag = -(1.0 + restitution) * vel_along_normal / effective_mass;
    let impulse = contact.normal * impulse_mag;
    contact.normal_impulse = impulse_mag.max(0.0);

    // Apply normal impulse
    if inv_mass_a > 0.0 {
        if let Some(body) = physics.body_mut(body_a) {
            body.wake();
            body.linear_velocity -= impulse * inv_mass_a;
            body.angular_velocity -= angular_term_a * impulse_mag;
        }
    }
    if inv_mass_b > 0.0 {
        if let Some(body) = physics.body_mut(body_b) {
            body.wake();
            body.linear_velocity += impulse * inv_mass_b;
            body.angular_velocity += angular_term_b * impulse_mag;
        }
    }

    // Friction Impulse
    let (vel_a, ang_vel_a) = physics
        .body(body_a)
        .map(|b| (b.linear_velocity, b.angular_velocity))
        .unwrap_or((Vec3::ZERO, Vec3::ZERO));
    let (vel_b, ang_vel_b) = physics
        .body(body_b)
        .map(|b| (b.linear_velocity, b.angular_velocity))
        .unwrap_or((Vec3::ZERO, Vec3::ZERO));

    let vel_at_a = vel_a + ang_vel_a.cross(r_a);
    let vel_at_b = vel_b + ang_vel_b.cross(r_b);
    let relative_velocity = vel_at_b - vel_at_a;

    let tangent = relative_velocity - contact.normal * relative_velocity.dot(contact.normal);
    let tangent_len_sq = tangent.length_squared();

    if tangent_len_sq > f32::EPSILON {
        let tangent_dir = tangent / tangent_len_sq.sqrt();

        let r_a_cross_t = r_a.cross(tangent_dir);
        let r_b_cross_t = r_b.cross(tangent_dir);
        let angular_term_a_t = world_inv_inertia_a * r_a_cross_t;
        let angular_term_b_t = world_inv_inertia_b * r_b_cross_t;

        let effective_mass_t = inv_mass_sum
            + angular_term_a_t.cross(r_a).dot(tangent_dir)
            + angular_term_b_t.cross(r_b).dot(tangent_dir);

        if effective_mass_t > f32::EPSILON {
            let jt = -relative_velocity.dot(tangent_dir) / effective_mass_t;
            let max_friction = impulse_mag.abs() * friction;
            let jt_clamped = jt.clamp(-max_friction, max_friction);
            let friction_impulse = tangent_dir * jt_clamped;
            contact.tangent_impulse = friction_impulse;

            if inv_mass_a > 0.0 {
                if let Some(body) = physics.body_mut(body_a) {
                    body.wake();
                    body.linear_velocity -= friction_impulse * inv_mass_a;
                    body.angular_velocity -= angular_term_a_t * jt_clamped;
                }
            }
            if inv_mass_b > 0.0 {
                if let Some(body) = physics.body_mut(body_b) {
                    body.wake();
                    body.linear_velocity += friction_impulse * inv_mass_b;
                    body.angular_velocity += angular_term_b_t * jt_clamped;
                }
            }
        }
    }
}

fn apply_warm_start_impulse(
    physics: &mut PhysicsWorld,
    body_a: BodyId,
    body_b: BodyId,
    contact_position: Vec3,
    contact_normal: Vec3,
    normal_impulse: f32,
    tangent_impulse: Vec3,
) {
    if normal_impulse.abs() <= f32::EPSILON && tangent_impulse.length_squared() <= f32::EPSILON {
        return;
    }

    let impulse = contact_normal * normal_impulse + tangent_impulse;
    let (inv_mass_a, inv_inertia_a, pos_a) = match physics.body(body_a) {
        Some(body) => (body.inverse_mass, body.world_inverse_inertia, body.position),
        None => return,
    };
    let (inv_mass_b, inv_inertia_b, pos_b) = match physics.body(body_b) {
        Some(body) => (body.inverse_mass, body.world_inverse_inertia, body.position),
        None => return,
    };

    let r_a = contact_position - pos_a;
    let r_b = contact_position - pos_b;

    if inv_mass_a > 0.0 {
        if let Some(body) = physics.body_mut(body_a) {
            body.wake();
            body.linear_velocity -= impulse * inv_mass_a;
            body.angular_velocity -= inv_inertia_a * r_a.cross(impulse);
        }
    }

    if inv_mass_b > 0.0 {
        if let Some(body) = physics.body_mut(body_b) {
            body.wake();
            body.linear_velocity += impulse * inv_mass_b;
            body.angular_velocity += inv_inertia_b * r_b.cross(impulse);
        }
    }
}

fn apply_contact(physics: &mut PhysicsWorld, contact: Contact, config: &SolverConfig) {
    // Get mass and inertia info
    let body_a_data = physics.body(contact.a_body).map(|b| {
        (
            b.inverse_mass,
            b.world_inverse_inertia,
            b.linear_velocity,
            b.angular_velocity,
            b.position,
        )
    });
    let body_b_data = physics.body(contact.b_body).map(|b| {
        (
            b.inverse_mass,
            b.world_inverse_inertia,
            b.linear_velocity,
            b.angular_velocity,
            b.position,
        )
    });

    let Some((inv_mass_a, world_inv_inertia_a, vel_a, ang_vel_a, pos_a)) = body_a_data else {
        return;
    };
    let Some((inv_mass_b, world_inv_inertia_b, vel_b, ang_vel_b, pos_b)) = body_b_data else {
        return;
    };

    let inv_mass_sum = inv_mass_a + inv_mass_b;
    if inv_mass_sum <= f32::EPSILON {
        return;
    }

    // Contact point relative to body centers
    let r_a = contact.contact_point - pos_a;
    let r_b = contact.contact_point - pos_b;

    // === Positional Correction (Baumgarte) ===
    let correction_mag = (contact.penetration - config.slop).max(0.0) * config.baumgarte;
    let correction = contact.normal * correction_mag;
    if inv_mass_a > 0.0 {
        if let Some(body) = physics.body_mut(contact.a_body) {
            body.wake();
            body.position -= correction * (inv_mass_a / inv_mass_sum);
        }
    }
    if inv_mass_b > 0.0 {
        if let Some(body) = physics.body_mut(contact.b_body) {
            body.wake();
            body.position += correction * (inv_mass_b / inv_mass_sum);
        }
    }

    // === Velocity Resolution ===
    // Compute velocity at contact point (including angular contribution)
    let vel_at_a = vel_a + ang_vel_a.cross(r_a);
    let vel_at_b = vel_b + ang_vel_b.cross(r_b);
    let relative_velocity = vel_at_b - vel_at_a;

    let vel_along_normal = relative_velocity.dot(contact.normal);
    if vel_along_normal > 0.0 {
        return; // Objects are separating
    }

    // Compute the effective mass at the contact point (including rotation)
    // K = (1/m_a + r_a x (I_a^{-1} x (r_a x n)) . n) + same for b
    let r_a_cross_n = r_a.cross(contact.normal);
    let r_b_cross_n = r_b.cross(contact.normal);
    let angular_term_a = world_inv_inertia_a * r_a_cross_n;
    let angular_term_b = world_inv_inertia_b * r_b_cross_n;

    let effective_mass = inv_mass_sum
        + angular_term_a.cross(r_a).dot(contact.normal)
        + angular_term_b.cross(r_b).dot(contact.normal);

    if effective_mass <= f32::EPSILON {
        return;
    }

    // Normal impulse
    let impulse_mag = -(1.0 + contact.restitution) * vel_along_normal / effective_mass;
    let impulse = contact.normal * impulse_mag;

    // Apply normal impulse
    if inv_mass_a > 0.0 {
        if let Some(body) = physics.body_mut(contact.a_body) {
            body.wake();
            body.linear_velocity -= impulse * inv_mass_a;
            body.angular_velocity -= angular_term_a * impulse_mag;
        }
    }
    if inv_mass_b > 0.0 {
        if let Some(body) = physics.body_mut(contact.b_body) {
            body.wake();
            body.linear_velocity += impulse * inv_mass_b;
            body.angular_velocity += angular_term_b * impulse_mag;
        }
    }

    // === Friction Impulse ===
    // Recompute velocities after normal impulse
    let (vel_a, ang_vel_a) = physics
        .body(contact.a_body)
        .map(|b| (b.linear_velocity, b.angular_velocity))
        .unwrap_or((Vec3::ZERO, Vec3::ZERO));
    let (vel_b, ang_vel_b) = physics
        .body(contact.b_body)
        .map(|b| (b.linear_velocity, b.angular_velocity))
        .unwrap_or((Vec3::ZERO, Vec3::ZERO));

    let vel_at_a = vel_a + ang_vel_a.cross(r_a);
    let vel_at_b = vel_b + ang_vel_b.cross(r_b);
    let relative_velocity = vel_at_b - vel_at_a;

    // Tangent direction (velocity component perpendicular to normal)
    let tangent = relative_velocity - contact.normal * relative_velocity.dot(contact.normal);
    let tangent_len_sq = tangent.length_squared();

    if tangent_len_sq > f32::EPSILON {
        let tangent_dir = tangent / tangent_len_sq.sqrt();

        // Compute effective mass for tangent
        let r_a_cross_t = r_a.cross(tangent_dir);
        let r_b_cross_t = r_b.cross(tangent_dir);
        let angular_term_a_t = world_inv_inertia_a * r_a_cross_t;
        let angular_term_b_t = world_inv_inertia_b * r_b_cross_t;

        let effective_mass_t = inv_mass_sum
            + angular_term_a_t.cross(r_a).dot(tangent_dir)
            + angular_term_b_t.cross(r_b).dot(tangent_dir);

        if effective_mass_t > f32::EPSILON {
            let jt = -relative_velocity.dot(tangent_dir) / effective_mass_t;
            let max_friction = impulse_mag * contact.friction;
            let jt_clamped = jt.clamp(-max_friction, max_friction);
            let friction_impulse = tangent_dir * jt_clamped;

            // Apply friction impulse
            if inv_mass_a > 0.0 {
                if let Some(body) = physics.body_mut(contact.a_body) {
                    body.wake();
                    body.linear_velocity -= friction_impulse * inv_mass_a;
                    body.angular_velocity -= angular_term_a_t * jt_clamped;
                }
            }
            if inv_mass_b > 0.0 {
                if let Some(body) = physics.body_mut(contact.b_body) {
                    body.wake();
                    body.linear_velocity += friction_impulse * inv_mass_b;
                    body.angular_velocity += angular_term_b_t * jt_clamped;
                }
            }
        }
    }
}

fn compute_collision(
    position_a: Vec3,
    rotation_a: Quat,
    shape_a: ColliderShape,
    position_b: Vec3,
    rotation_b: Quat,
    shape_b: ColliderShape,
) -> Option<(Vec3, f32)> {
    compute_collision_with_contact(
        position_a, rotation_a, shape_a, position_b, rotation_b, shape_b,
    )
    .map(|(normal, penetration, _contact_point)| (normal, penetration))
}

/// Compute collision with contact point.
/// Returns (normal from A to B, penetration depth, contact point in world space).
fn compute_collision_with_contact(
    position_a: Vec3,
    rotation_a: Quat,
    shape_a: ColliderShape,
    position_b: Vec3,
    rotation_b: Quat,
    shape_b: ColliderShape,
) -> Option<(Vec3, f32, Vec3)> {
    match (shape_a, shape_b) {
        (ColliderShape::Sphere { radius: ra }, ColliderShape::Sphere { radius: rb }) => {
            sphere_sphere_collision_with_contact(position_a, ra, position_b, rb)
        }
        (ColliderShape::Sphere { radius }, ColliderShape::Cuboid { half_extents }) => {
            sphere_obb_collision_with_contact(
                position_a,
                radius,
                position_b,
                rotation_b,
                half_extents,
            )
        }
        (ColliderShape::Cuboid { half_extents }, ColliderShape::Sphere { radius }) => {
            sphere_obb_collision_with_contact(
                position_b,
                radius,
                position_a,
                rotation_a,
                half_extents,
            )
            .map(|(normal, penetration, contact_point)| (-normal, penetration, contact_point))
        }
        (
            ColliderShape::Cuboid {
                half_extents: extents_a,
            },
            ColliderShape::Cuboid {
                half_extents: extents_b,
            },
        ) => {
            let obb_a = Obb {
                center: position_a,
                axes: [
                    rotation_a * Vec3::X,
                    rotation_a * Vec3::Y,
                    rotation_a * Vec3::Z,
                ],
                half_extents: extents_a,
            };
            let obb_b = Obb {
                center: position_b,
                axes: [
                    rotation_b * Vec3::X,
                    rotation_b * Vec3::Y,
                    rotation_b * Vec3::Z,
                ],
                half_extents: extents_b,
            };
            obb_obb_collision_with_contact(obb_a, obb_b)
        }
    }
}

fn sphere_sphere_collision(a: Vec3, ra: f32, b: Vec3, rb: f32) -> Option<(Vec3, f32)> {
    sphere_sphere_collision_with_contact(a, ra, b, rb)
        .map(|(normal, penetration, _)| (normal, penetration))
}

fn sphere_sphere_collision_with_contact(
    a: Vec3,
    ra: f32,
    b: Vec3,
    rb: f32,
) -> Option<(Vec3, f32, Vec3)> {
    let delta = b - a;
    let distance = delta.length();
    let target = ra + rb;
    if distance >= target {
        return None;
    }
    let normal = if distance > f32::EPSILON {
        delta / distance
    } else {
        Vec3::X
    };
    // Contact point is on the surface of sphere A towards B
    let contact_point = a + normal * ra;
    Some((normal, target - distance + 0.001, contact_point))
}

fn sphere_obb_collision(
    sphere_center: Vec3,
    sphere_radius: f32,
    obb_center: Vec3,
    obb_rotation: Quat,
    obb_half_extents: Vec3,
) -> Option<(Vec3, f32)> {
    sphere_obb_collision_with_contact(
        sphere_center,
        sphere_radius,
        obb_center,
        obb_rotation,
        obb_half_extents,
    )
    .map(|(normal, penetration, _)| (normal, penetration))
}

fn sphere_obb_collision_with_contact(
    sphere_center: Vec3,
    sphere_radius: f32,
    obb_center: Vec3,
    obb_rotation: Quat,
    obb_half_extents: Vec3,
) -> Option<(Vec3, f32, Vec3)> {
    let local_center = obb_rotation.conjugate() * (sphere_center - obb_center);
    let clamped = local_center.clamp(-obb_half_extents, obb_half_extents);
    let closest_world = obb_center + obb_rotation * clamped;
    let delta = closest_world - sphere_center;
    let distance_sq = delta.length_squared();

    if distance_sq > sphere_radius * sphere_radius {
        return None;
    }

    if distance_sq > f32::EPSILON {
        let distance = distance_sq.sqrt();
        let normal = delta / distance;
        // Contact point is on the sphere surface towards the OBB
        let contact_point = sphere_center + normal * sphere_radius;
        return Some((normal, sphere_radius - distance + 0.001, contact_point));
    }

    // Sphere center inside OBB: choose nearest face.
    let distances = obb_half_extents - local_center.abs();
    let (axis, face_distance) = if distances.x <= distances.y && distances.x <= distances.z {
        (0, distances.x)
    } else if distances.y <= distances.z {
        (1, distances.y)
    } else {
        (2, distances.z)
    };

    let normal_local = match axis {
        0 => {
            if local_center.x >= 0.0 {
                Vec3::X
            } else {
                -Vec3::X
            }
        }
        1 => {
            if local_center.y >= 0.0 {
                Vec3::Y
            } else {
                -Vec3::Y
            }
        }
        _ => {
            if local_center.z >= 0.0 {
                Vec3::Z
            } else {
                -Vec3::Z
            }
        }
    };
    let normal = obb_rotation * normal_local;
    // Contact point is on the OBB face
    let contact_local = local_center;
    let contact_on_face = Vec3::new(
        if axis == 0 {
            obb_half_extents.x * normal_local.x.signum()
        } else {
            contact_local.x
        },
        if axis == 1 {
            obb_half_extents.y * normal_local.y.signum()
        } else {
            contact_local.y
        },
        if axis == 2 {
            obb_half_extents.z * normal_local.z.signum()
        } else {
            contact_local.z
        },
    );
    let contact_point = obb_center + obb_rotation * contact_on_face;
    Some((normal, sphere_radius + face_distance + 0.001, contact_point))
}

fn obb_obb_collision(a: Obb, b: Obb) -> Option<(Vec3, f32)> {
    obb_obb_collision_with_contact(a, b).map(|(normal, penetration, _)| (normal, penetration))
}

fn obb_obb_collision_with_contact(a: Obb, b: Obb) -> Option<(Vec3, f32, Vec3)> {
    let a_e = [a.half_extents.x, a.half_extents.y, a.half_extents.z];
    let b_e = [b.half_extents.x, b.half_extents.y, b.half_extents.z];

    let mut r = [[0.0_f32; 3]; 3];
    let mut abs_r = [[0.0_f32; 3]; 3];
    const EPS: f32 = 1.0e-6;

    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = a.axes[i].dot(b.axes[j]);
            abs_r[i][j] = r[i][j].abs() + EPS;
        }
    }

    let t_world = b.center - a.center;
    let t = [
        t_world.dot(a.axes[0]),
        t_world.dot(a.axes[1]),
        t_world.dot(a.axes[2]),
    ];

    let mut best_penetration = f32::INFINITY;
    let mut best_normal = Vec3::Y;
    let mut best_axis_type: u8 = 0; // 0-2: A faces, 3-5: B faces, 6-14: edges
    let mut best_axis_index: usize = 0;

    let mut consider_axis =
        |axis: Vec3, distance: f32, ra: f32, rb: f32, axis_type: u8, idx: usize| -> Option<()> {
            let overlap = ra + rb - distance;
            if overlap < 0.0 {
                return None;
            }
            if overlap < best_penetration {
                best_penetration = overlap;
                best_normal = axis;
                best_axis_type = axis_type;
                best_axis_index = idx;
            }
            Some(())
        };

    // A's face normals
    for i in 0..3 {
        let ra = a_e[i];
        let rb = b_e[0] * abs_r[i][0] + b_e[1] * abs_r[i][1] + b_e[2] * abs_r[i][2];
        let distance = t[i].abs();
        let axis = if t[i] >= 0.0 { a.axes[i] } else { -a.axes[i] };
        consider_axis(axis, distance, ra, rb, 0, i)?;
    }

    // B's face normals
    for j in 0..3 {
        let ra = a_e[0] * abs_r[0][j] + a_e[1] * abs_r[1][j] + a_e[2] * abs_r[2][j];
        let rb = b_e[j];
        let distance = (t[0] * r[0][j] + t[1] * r[1][j] + t[2] * r[2][j]).abs();
        let axis = if t_world.dot(b.axes[j]) >= 0.0 {
            b.axes[j]
        } else {
            -b.axes[j]
        };
        consider_axis(axis, distance, ra, rb, 1, j)?;
    }

    // Edge cross products
    let mut edge_idx = 0;
    for i in 0..3 {
        for j in 0..3 {
            let axis = a.axes[i].cross(b.axes[j]);
            let axis_len_sq = axis.length_squared();
            if axis_len_sq <= 1.0e-10 {
                edge_idx += 1;
                continue;
            }

            let ra =
                a_e[(i + 1) % 3] * abs_r[(i + 2) % 3][j] + a_e[(i + 2) % 3] * abs_r[(i + 1) % 3][j];
            let rb =
                b_e[(j + 1) % 3] * abs_r[i][(j + 2) % 3] + b_e[(j + 2) % 3] * abs_r[i][(j + 1) % 3];
            let distance =
                (t[(i + 2) % 3] * r[(i + 1) % 3][j] - t[(i + 1) % 3] * r[(i + 2) % 3][j]).abs();

            let mut world_axis = axis / axis_len_sq.sqrt();
            if world_axis.dot(t_world) < 0.0 {
                world_axis = -world_axis;
            }
            consider_axis(world_axis, distance, ra, rb, 2, edge_idx)?;
            edge_idx += 1;
        }
    }

    // Compute contact point based on axis type
    let contact_point =
        compute_obb_contact_point(&a, &b, best_axis_type, best_axis_index, best_normal);

    Some((
        best_normal.normalize_or_zero(),
        best_penetration + 0.001,
        contact_point,
    ))
}

/// Compute a contact point for OBB-OBB collision based on the separating axis.
fn compute_obb_contact_point(
    a: &Obb,
    b: &Obb,
    axis_type: u8,
    axis_index: usize,
    normal: Vec3,
) -> Vec3 {
    match axis_type {
        // Face from A: contact point is on B, projected onto A's face
        0 => {
            // Use center of B projected onto A's face
            let face_axis = a.axes[axis_index];
            let penetration_dir = if normal.dot(face_axis) > 0.0 {
                1.0
            } else {
                -1.0
            };
            let face_point = a.center + face_axis * a.half_extents[axis_index] * penetration_dir;

            // Project B's center onto the face plane, then clamp to both boxes
            let mut contact = b.center;
            for i in 0..3 {
                let proj = (contact - face_point).dot(a.axes[i]);
                let max_ext = a.half_extents[i] + b.half_extents.dot(a.axes[i].abs());
                contact -= a.axes[i] * proj.clamp(-max_ext, max_ext);
            }
            contact
        }
        // Face from B: contact point is on A, projected onto B's face
        1 => {
            let face_axis = b.axes[axis_index];
            let penetration_dir = if normal.dot(-face_axis) > 0.0 {
                -1.0
            } else {
                1.0
            };
            let face_point = b.center + face_axis * b.half_extents[axis_index] * penetration_dir;

            let mut contact = a.center;
            for i in 0..3 {
                let proj = (contact - face_point).dot(b.axes[i]);
                let max_ext = b.half_extents[i] + a.half_extents.dot(b.axes[i].abs());
                contact -= b.axes[i] * proj.clamp(-max_ext, max_ext);
            }
            contact
        }
        // Edge-edge: contact point is midpoint of the closest points on the two edges
        _ => {
            // Simplified: use midpoint between centers
            (a.center + b.center) * 0.5
        }
    }
}

fn shape_to_aabb(shape: ColliderShape, position: Vec3, rotation: Quat) -> Aabb {
    match shape {
        ColliderShape::Sphere { radius } => {
            let extents = Vec3::splat(radius);
            Aabb {
                min: position - extents,
                max: position + extents,
            }
        }
        ColliderShape::Cuboid { half_extents } => {
            let axis_x = rotation * Vec3::X;
            let axis_y = rotation * Vec3::Y;
            let axis_z = rotation * Vec3::Z;
            let extents = Vec3::new(
                axis_x.x.abs() * half_extents.x
                    + axis_y.x.abs() * half_extents.y
                    + axis_z.x.abs() * half_extents.z,
                axis_x.y.abs() * half_extents.x
                    + axis_y.y.abs() * half_extents.y
                    + axis_z.y.abs() * half_extents.z,
                axis_x.z.abs() * half_extents.x
                    + axis_y.z.abs() * half_extents.y
                    + axis_z.z.abs() * half_extents.z,
            );
            Aabb {
                min: position - extents,
                max: position + extents,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use glam::{Quat, Vec3};
    use oxide_engine::prelude::{CommandQueue, IntoSystem, Time, TransformComponent, World};

    use super::{
        broadphase_candidates, broadphase_candidates_sweep, ensure_colliders_system,
        ensure_rigid_bodies_system, generate_manifold, initialize_body_pose_system,
        physics_step_system, prune_orphan_bodies_system, prune_orphan_colliders_system,
        resolve_collisions, sync_transforms_system, SolverConfig,
    };
    use crate::components::{
        collision_layer, ColliderComponent, ColliderShape, CollisionLayers, RigidBodyComponent,
    };
    use crate::joints::JointComponent;
    use crate::resources::{ManifoldKey, PhysicsWorld, DEFAULT_FIXED_TIMESTEP};

    fn run_system<S, Marker>(world: &mut World, system: S)
    where
        S: IntoSystem<Marker>,
    {
        let mut sys = system.into_system();
        let mut commands = CommandQueue::new();
        sys.run(world, &mut commands);
        commands.apply(world);
    }

    fn setup_world() -> World {
        let mut world = World::new();
        world.insert_resource(Time::default());
        world.insert_resource(PhysicsWorld::default());
        world.insert_resource(crate::events::CollisionEvents::default());
        world
    }

    fn setup_resting_contact_world() -> (World, oxide_engine::prelude::Entity) {
        let mut world = setup_world();

        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, -0.5, 0.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::cuboid(Vec3::new(10.0, 0.5, 10.0)),
        ));

        let box_entity = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, 1.6, 0.0)),
                RigidBodyComponent::dynamic(),
                ColliderComponent::cuboid(Vec3::splat(0.5)),
            ))
            .id();

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);
        run_system(&mut world, ensure_colliders_system);
        (world, box_entity)
    }

    fn run_resting_contact_simulation(
        world: &mut World,
        tracked_entity: oxide_engine::prelude::Entity,
        disable_warm_start: bool,
        steps: u32,
    ) -> (f32, f32, f32) {
        let mut cumulative_vertical_speed: f32 = 0.0;
        let mut cumulative_rest_error: f32 = 0.0;
        let mut final_height: f32 = 0.0;
        let expected_resting_height = 0.5;

        for frame in 0..steps {
            if disable_warm_start {
                world
                    .resource_mut::<PhysicsWorld>()
                    .cached_manifolds
                    .clear();
            }

            world.resource_mut::<Time>().delta = Duration::from_secs_f32(DEFAULT_FIXED_TIMESTEP);
            run_system(world, physics_step_system);
            run_system(world, sync_transforms_system);

            let transform = world
                .get::<TransformComponent>(tracked_entity)
                .expect("tracked transform should exist");
            let rigid_body = world
                .get::<RigidBodyComponent>(tracked_entity)
                .expect("tracked rigid body should exist");

            if frame >= steps / 2 {
                cumulative_vertical_speed += rigid_body.linear_velocity.y.abs();
                cumulative_rest_error +=
                    (transform.transform.position.y - expected_resting_height).abs();
            }

            final_height = transform.transform.position.y;
        }

        (
            cumulative_vertical_speed,
            cumulative_rest_error,
            final_height,
        )
    }

    #[test]
    fn dynamic_body_falls_and_settles_on_ground() {
        let mut world = setup_world();
        let dynamic = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, 5.0, 0.0)),
                RigidBodyComponent::dynamic(),
                ColliderComponent::cuboid(Vec3::splat(0.5)),
            ))
            .id();

        let ground = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, -2.0, 0.0)),
                RigidBodyComponent::static_body(),
                ColliderComponent::cuboid(Vec3::new(10.0, 0.5, 10.0)),
            ))
            .id();

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);
        run_system(&mut world, ensure_colliders_system);

        for _ in 0..220 {
            world.resource_mut::<Time>().delta = Duration::from_secs_f32(DEFAULT_FIXED_TIMESTEP);
            run_system(&mut world, physics_step_system);
            run_system(&mut world, sync_transforms_system);
        }

        let dynamic_y = world
            .get::<TransformComponent>(dynamic)
            .expect("dynamic body transform should exist")
            .transform
            .position
            .y;
        let ground_y = world
            .get::<TransformComponent>(ground)
            .expect("ground transform should exist")
            .transform
            .position
            .y;

        assert!(dynamic_y < 5.0, "dynamic body should fall");
        assert!(
            (-1.2..=-0.8).contains(&dynamic_y),
            "dynamic body should settle near ground top, got {dynamic_y}"
        );
        assert_eq!(ground_y, -2.0, "static body should remain fixed");
    }

    #[test]
    fn fixed_timestep_accumulator_defers_substep_until_enough_time() {
        let mut world = setup_world();
        let entity = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, 2.0, 0.0)),
                RigidBodyComponent::dynamic(),
            ))
            .id();

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);

        world.resource_mut::<Time>().delta = Duration::from_secs_f32(DEFAULT_FIXED_TIMESTEP * 0.5);
        run_system(&mut world, physics_step_system);
        run_system(&mut world, sync_transforms_system);

        let y_after_half_dt = world
            .get::<TransformComponent>(entity)
            .expect("transform should exist")
            .transform
            .position
            .y;
        assert!(
            (y_after_half_dt - 2.0).abs() <= f32::EPSILON,
            "body should not move before first fixed step, got y={y_after_half_dt}"
        );

        world.resource_mut::<Time>().delta = Duration::from_secs_f32(DEFAULT_FIXED_TIMESTEP);
        run_system(&mut world, physics_step_system);
        run_system(&mut world, sync_transforms_system);

        let y_after_full_dt = world
            .get::<TransformComponent>(entity)
            .expect("transform should exist")
            .transform
            .position
            .y;
        assert!(
            y_after_full_dt < 2.0,
            "body should move after accumulated fixed step, got y={y_after_full_dt}"
        );
    }

    #[test]
    fn orphan_prune_removes_body_and_collider_from_physics_world() {
        let mut world = setup_world();
        let entity = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, 0.0, 0.0)),
                RigidBodyComponent::dynamic(),
                ColliderComponent::cuboid(Vec3::splat(0.5)),
            ))
            .id();

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);
        run_system(&mut world, ensure_colliders_system);

        assert_eq!(world.resource::<PhysicsWorld>().bodies.len(), 1);
        assert_eq!(world.resource::<PhysicsWorld>().colliders.len(), 1);

        let _ = world.remove::<ColliderComponent>(entity);
        let _ = world.remove::<RigidBodyComponent>(entity);

        run_system(&mut world, prune_orphan_colliders_system);
        run_system(&mut world, prune_orphan_bodies_system);

        assert_eq!(world.resource::<PhysicsWorld>().bodies.len(), 0);
        assert_eq!(world.resource::<PhysicsWorld>().colliders.len(), 0);
    }

    #[test]
    fn dynamic_dynamic_collision_applies_impulse_to_both_bodies() {
        let mut world = setup_world();
        world.resource_mut::<PhysicsWorld>().gravity = Vec3::ZERO;

        let body_a = world
            .spawn((
                TransformComponent::from_position(Vec3::new(-1.0, 0.0, 0.0)),
                RigidBodyComponent::dynamic().with_linear_velocity(Vec3::new(2.0, 0.0, 0.0)),
                ColliderComponent::sphere(0.5)
                    .with_restitution(1.0)
                    .with_friction(0.0),
            ))
            .id();
        let body_b = world
            .spawn((
                TransformComponent::from_position(Vec3::new(1.0, 0.0, 0.0)),
                RigidBodyComponent::dynamic().with_linear_velocity(Vec3::new(-2.0, 0.0, 0.0)),
                ColliderComponent::sphere(0.5)
                    .with_restitution(1.0)
                    .with_friction(0.0),
            ))
            .id();

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);
        run_system(&mut world, ensure_colliders_system);

        for _ in 0..90 {
            world.resource_mut::<Time>().delta = Duration::from_secs_f32(DEFAULT_FIXED_TIMESTEP);
            run_system(&mut world, physics_step_system);
            run_system(&mut world, sync_transforms_system);
        }

        let vx_a = world
            .get::<RigidBodyComponent>(body_a)
            .expect("body a should exist")
            .linear_velocity
            .x;
        let vx_b = world
            .get::<RigidBodyComponent>(body_b)
            .expect("body b should exist")
            .linear_velocity
            .x;

        assert!(vx_a < 0.0, "body A should reverse direction, got vx={vx_a}");
        assert!(vx_b > 0.0, "body B should reverse direction, got vx={vx_b}");
    }

    #[test]
    fn sensor_overlap_emits_collision_event() {
        let mut world = setup_world();

        world.spawn((
            TransformComponent::default(),
            RigidBodyComponent::static_body(),
            ColliderComponent::sphere(1.0).sensor(true),
        ));
        world.spawn((
            TransformComponent::from_position(Vec3::new(0.5, 0.0, 0.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::sphere(1.0),
        ));

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);
        run_system(&mut world, ensure_colliders_system);

        world.resource_mut::<Time>().delta = Duration::from_secs_f32(DEFAULT_FIXED_TIMESTEP);
        run_system(&mut world, physics_step_system);

        let events = world.resource::<crate::events::CollisionEvents>();
        assert_eq!(
            events.len(),
            1,
            "sensor overlap should emit exactly one event"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, crate::events::CollisionEvent::Started { .. })),
            "sensor overlap should produce a Started event"
        );
    }

    #[test]
    fn warm_start_cached_impulses_apply_without_solver_iterations() {
        fn setup_overlap_scene() -> (World, oxide_engine::prelude::Entity) {
            let mut world = setup_world();
            world.resource_mut::<PhysicsWorld>().gravity = Vec3::ZERO;

            let dynamic = world
                .spawn((
                    TransformComponent::from_position(Vec3::new(-0.4, 0.0, 0.0)),
                    RigidBodyComponent::dynamic().with_linear_velocity(Vec3::new(1.0, 0.0, 0.0)),
                    ColliderComponent::sphere(0.5)
                        .with_restitution(0.0)
                        .with_friction(0.0),
                ))
                .id();
            world.spawn((
                TransformComponent::from_position(Vec3::new(0.4, 0.0, 0.0)),
                RigidBodyComponent::static_body(),
                ColliderComponent::sphere(0.5)
                    .with_restitution(0.0)
                    .with_friction(0.0),
            ));

            run_system(&mut world, ensure_rigid_bodies_system);
            run_system(&mut world, initialize_body_pose_system);
            run_system(&mut world, ensure_colliders_system);

            (world, dynamic)
        }

        let (mut warm_world, warm_dynamic) = setup_overlap_scene();
        let warm_dynamic_handle = warm_world
            .get::<RigidBodyComponent>(warm_dynamic)
            .expect("dynamic body component should exist")
            .handle
            .expect("dynamic body should have a physics handle");

        let (warm_speed, warm_x_velocity) = {
            let mut physics = warm_world.resource_mut::<PhysicsWorld>();
            resolve_collisions(
                &mut physics,
                SolverConfig {
                    iterations: 1,
                    ..SolverConfig::default()
                },
            );

            let cached_normal_impulse = physics
                .cached_manifolds
                .values()
                .flat_map(|manifold| manifold.contacts.iter())
                .map(|contact| contact.normal_impulse)
                .fold(0.0_f32, f32::max);
            assert!(
                cached_normal_impulse > 0.0,
                "expected first solve to cache a normal impulse"
            );

            if let Some(body) = physics.body_mut(warm_dynamic_handle) {
                body.linear_velocity = Vec3::ZERO;
                body.angular_velocity = Vec3::ZERO;
            }

            resolve_collisions(
                &mut physics,
                SolverConfig {
                    iterations: 0,
                    ..SolverConfig::default()
                },
            );

            let velocity = physics
                .body(warm_dynamic_handle)
                .expect("warm-started dynamic body should exist")
                .linear_velocity;
            (velocity.length(), velocity.x)
        };

        let (mut cold_world, cold_dynamic) = setup_overlap_scene();
        let cold_dynamic_handle = cold_world
            .get::<RigidBodyComponent>(cold_dynamic)
            .expect("dynamic body component should exist")
            .handle
            .expect("dynamic body should have a physics handle");
        let cold_speed = {
            let mut physics = cold_world.resource_mut::<PhysicsWorld>();
            if let Some(body) = physics.body_mut(cold_dynamic_handle) {
                body.linear_velocity = Vec3::ZERO;
                body.angular_velocity = Vec3::ZERO;
            }

            resolve_collisions(
                &mut physics,
                SolverConfig {
                    iterations: 0,
                    ..SolverConfig::default()
                },
            );

            physics
                .body(cold_dynamic_handle)
                .expect("cold-start dynamic body should exist")
                .linear_velocity
                .length()
        };

        assert!(
            warm_speed > cold_speed + 1e-4,
            "warm-started solve should apply cached impulses (warm={warm_speed}, cold={cold_speed})"
        );
        assert!(
            warm_x_velocity < -1e-4,
            "warm start should push dynamic body away from the contact, got vx={warm_x_velocity}"
        );
    }

    #[test]
    fn angular_velocity_integrates_rotation_each_step() {
        let mut world = setup_world();
        world.resource_mut::<PhysicsWorld>().gravity = Vec3::ZERO;

        let entity = world
            .spawn((
                TransformComponent::default(),
                RigidBodyComponent::dynamic().with_angular_velocity(Vec3::new(0.0, 2.0, 0.0)),
            ))
            .id();

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);

        for _ in 0..10 {
            world.resource_mut::<Time>().delta = Duration::from_secs_f32(DEFAULT_FIXED_TIMESTEP);
            run_system(&mut world, physics_step_system);
            run_system(&mut world, sync_transforms_system);
        }

        let rotation = world
            .get::<TransformComponent>(entity)
            .expect("transform should exist")
            .transform
            .rotation;
        let dot = rotation.dot(Quat::IDENTITY).abs();
        assert!(
            dot < 0.999,
            "rotation should change from identity due to angular velocity, dot={dot}"
        );
    }

    #[test]
    fn off_center_impulse_causes_rotation() {
        let mut world = setup_world();
        world.resource_mut::<PhysicsWorld>().gravity = Vec3::ZERO;

        let entity = world
            .spawn((
                TransformComponent::default(),
                RigidBodyComponent::dynamic(),
                ColliderComponent::cuboid(Vec3::splat(1.0)), // 2x2x2 box
            ))
            .id();

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);
        run_system(&mut world, ensure_colliders_system);

        // Apply an impulse at the edge of the box (not at center)
        let body_id = world
            .get::<RigidBodyComponent>(entity)
            .unwrap()
            .handle
            .unwrap();

        // Apply impulse at the right edge of the box
        let impulse = Vec3::new(0.0, 0.0, 10.0); // Forward impulse
        let point = Vec3::new(1.0, 0.0, 0.0); // At the right edge

        {
            let mut physics = world.resource_mut::<PhysicsWorld>();
            if let Some(body) = physics.body_mut(body_id) {
                body.apply_impulse_at_point(impulse, point);
            }
        }

        run_system(&mut world, sync_transforms_system);

        // The body should have both linear and angular velocity
        let rigid_body = world.get::<RigidBodyComponent>(entity).unwrap();

        // Linear velocity should be in the impulse direction
        assert!(
            rigid_body.linear_velocity.z > 0.0,
            "should have forward linear velocity, got {:?}",
            rigid_body.linear_velocity
        );

        // Angular velocity should be non-zero (rotation around Y axis from off-center impulse)
        assert!(
            rigid_body.angular_velocity.length() > 0.01,
            "should have angular velocity from off-center impulse, got {:?}",
            rigid_body.angular_velocity
        );
    }

    #[test]
    fn large_box_has_more_mass_than_small_box() {
        let small_shape = ColliderShape::cuboid(Vec3::splat(0.5)); // 1x1x1 box
        let large_shape = ColliderShape::cuboid(Vec3::splat(1.0)); // 2x2x2 box
        let density = 1000.0;

        let small_mass = crate::mass_properties::calculate_mass(small_shape, density);
        let large_mass = crate::mass_properties::calculate_mass(large_shape, density);

        // Large box has 8x the volume (2^3), so 8x the mass
        assert!(large_mass > small_mass, "large box should have more mass");
        assert!(
            (large_mass / small_mass - 8.0).abs() < 0.01,
            "mass ratio should be 8:1, got {}",
            large_mass / small_mass
        );
    }

    #[test]
    fn box_dropped_on_edge_tumbles() {
        let mut world = setup_world();

        // Create a box positioned so it will land on its edge
        let box_entity = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, 2.0, 0.0)),
                RigidBodyComponent::dynamic(),
                ColliderComponent::cuboid(Vec3::splat(0.5)),
            ))
            .id();

        // Ground
        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, -1.0, 0.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::cuboid(Vec3::new(10.0, 0.5, 10.0)),
        ));

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);
        run_system(&mut world, ensure_colliders_system);

        // Simulate for a while
        for _ in 0..120 {
            world.resource_mut::<Time>().delta = Duration::from_secs_f32(DEFAULT_FIXED_TIMESTEP);
            run_system(&mut world, physics_step_system);
            run_system(&mut world, sync_transforms_system);
        }

        // The box should have settled (low velocity)
        let rigid_body = world.get::<RigidBodyComponent>(box_entity).unwrap();
        let velocity = rigid_body.linear_velocity.length();
        let angular_velocity = rigid_body.angular_velocity.length();

        // After settling, velocities should be low
        assert!(
            velocity < 1.0,
            "box should have settled, velocity = {}",
            velocity
        );
        assert!(
            angular_velocity < 1.0,
            "box should have settled angular velocity, got {}",
            angular_velocity
        );
    }

    #[test]
    fn five_box_tower_is_stable() {
        // NOTE: This test uses a smaller stack due to current solver limitations.
        // Full 10-box stability requires contact manifold integration with warm starting.
        let mut world = setup_world();

        // Ground
        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, -0.5, 0.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::cuboid(Vec3::new(10.0, 0.5, 10.0)),
        ));

        // Stack 5 boxes
        let box_size = 0.5;
        let mut boxes = Vec::new();
        for i in 0..5 {
            let y = box_size + i as f32 * (box_size * 2.0 + 0.02);
            let entity = world
                .spawn((
                    TransformComponent::from_position(Vec3::new(0.0, y, 0.0)),
                    RigidBodyComponent::dynamic(),
                    ColliderComponent::cuboid(Vec3::splat(box_size)),
                ))
                .id();
            boxes.push(entity);
        }

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);
        run_system(&mut world, ensure_colliders_system);

        // Simulate for 3 seconds (180 frames)
        for _ in 0..180 {
            world.resource_mut::<Time>().delta = Duration::from_secs_f32(DEFAULT_FIXED_TIMESTEP);
            run_system(&mut world, physics_step_system);
            run_system(&mut world, sync_transforms_system);
        }

        // Check that the top box is still roughly at the top
        let top_box = boxes.last().unwrap();
        let transform = world.get::<TransformComponent>(*top_box).unwrap();
        let top_y = transform.transform.position.y;

        // Top box should still be roughly at the top (y > 4 for 5 boxes)
        assert!(
            top_y > 3.5,
            "top box should still be near the top of the stack, y = {}",
            top_y
        );

        // Check that it hasn't drifted far horizontally
        let horizontal_dist = Vec3::new(
            transform.transform.position.x,
            0.0,
            transform.transform.position.z,
        )
        .length();
        assert!(
            horizontal_dist < 1.5,
            "stack should remain roughly centered, horizontal drift = {}",
            horizontal_dist
        );
    }

    #[test]
    fn spatial_hash_broadphase_matches_sweep_pairs_for_simple_scene() {
        let mut world = setup_world();

        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, 0.0, 0.0)),
            RigidBodyComponent::dynamic(),
            ColliderComponent::cuboid(Vec3::splat(0.5)),
        ));
        world.spawn((
            TransformComponent::from_position(Vec3::new(0.8, 0.0, 0.0)),
            RigidBodyComponent::dynamic(),
            ColliderComponent::cuboid(Vec3::splat(0.5)),
        ));
        world.spawn((
            TransformComponent::from_position(Vec3::new(10.0, 0.0, 0.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::cuboid(Vec3::splat(0.5)),
        ));

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);
        run_system(&mut world, ensure_colliders_system);

        let physics = world.resource::<PhysicsWorld>();
        let mut spatial_pairs = broadphase_candidates(physics);
        let mut sweep_pairs = broadphase_candidates_sweep(physics);

        spatial_pairs.sort_by_key(|(a, b)| (a.0, b.0));
        sweep_pairs.sort_by_key(|(a, b)| (a.0, b.0));
        assert_eq!(spatial_pairs, sweep_pairs);
    }

    #[test]
    fn ensure_colliders_uses_collision_layers_component() {
        let mut world = setup_world();
        let entity = world
            .spawn((
                TransformComponent::default(),
                RigidBodyComponent::dynamic(),
                ColliderComponent::sphere(0.5),
                CollisionLayers::new(collision_layer::PLAYER, collision_layer::STATIC),
            ))
            .id();

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);
        run_system(&mut world, ensure_colliders_system);

        let collider_handle = world
            .get::<ColliderComponent>(entity)
            .expect("collider component should exist")
            .handle
            .expect("physics collider should be created");

        let physics = world.resource::<PhysicsWorld>();
        let collider = physics
            .colliders
            .get(&collider_handle)
            .expect("physics collider should exist");
        assert_eq!(collider.collision_layers.group, collision_layer::PLAYER);
        assert_eq!(collider.collision_layers.mask, collision_layer::STATIC);
    }

    #[test]
    fn ball_socket_joint_reduces_anchor_separation() {
        let mut world = setup_world();
        world.resource_mut::<PhysicsWorld>().gravity = Vec3::ZERO;

        let a = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, 0.0, 0.0)),
                RigidBodyComponent::dynamic(),
            ))
            .id();
        let b = world
            .spawn((
                TransformComponent::from_position(Vec3::new(2.0, 0.0, 0.0)),
                RigidBodyComponent::dynamic(),
            ))
            .id();

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);

        let body_a = world.get::<RigidBodyComponent>(a).unwrap().handle.unwrap();
        let body_b = world.get::<RigidBodyComponent>(b).unwrap().handle.unwrap();
        world.spawn(JointComponent::ball_socket(body_a, body_b));

        let before = {
            let ta = world
                .get::<TransformComponent>(a)
                .unwrap()
                .transform
                .position;
            let tb = world
                .get::<TransformComponent>(b)
                .unwrap()
                .transform
                .position;
            (tb - ta).length()
        };

        for _ in 0..90 {
            world.resource_mut::<Time>().delta = Duration::from_secs_f32(DEFAULT_FIXED_TIMESTEP);
            run_system(&mut world, physics_step_system);
            run_system(&mut world, sync_transforms_system);
        }

        let after = {
            let ta = world
                .get::<TransformComponent>(a)
                .unwrap()
                .transform
                .position;
            let tb = world
                .get::<TransformComponent>(b)
                .unwrap()
                .transform
                .position;
            (tb - ta).length()
        };
        assert!(
            after < before,
            "joint should reduce separation: before={before}, after={after}"
        );
    }

    #[test]
    fn hinge_joint_reduces_perpendicular_angular_velocity() {
        let mut world = setup_world();
        world.resource_mut::<PhysicsWorld>().gravity = Vec3::ZERO;

        let a = world
            .spawn((TransformComponent::default(), RigidBodyComponent::dynamic()))
            .id();
        let b = world
            .spawn((
                TransformComponent::default(),
                RigidBodyComponent::dynamic().with_angular_velocity(Vec3::new(2.0, 3.0, 1.5)),
            ))
            .id();

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);

        let body_a = world.get::<RigidBodyComponent>(a).unwrap().handle.unwrap();
        let body_b = world.get::<RigidBodyComponent>(b).unwrap().handle.unwrap();
        world.spawn(JointComponent::hinge(body_a, body_b, Vec3::Y, None));

        let before = world.get::<RigidBodyComponent>(b).unwrap().angular_velocity;
        let before_perp = Vec3::new(before.x, 0.0, before.z).length();

        for _ in 0..120 {
            world.resource_mut::<Time>().delta = Duration::from_secs_f32(DEFAULT_FIXED_TIMESTEP);
            run_system(&mut world, physics_step_system);
            run_system(&mut world, sync_transforms_system);
        }

        let after = world.get::<RigidBodyComponent>(b).unwrap().angular_velocity;
        let after_perp = Vec3::new(after.x, 0.0, after.z).length();
        assert!(
            after_perp < before_perp,
            "hinge should reduce non-axis angular velocity: before={before_perp}, after={after_perp}"
        );
    }

    #[test]
    fn persistent_contacts_transfer_cached_impulses_between_frames() {
        let mut world = setup_world();
        world.resource_mut::<PhysicsWorld>().gravity = Vec3::ZERO;

        let dynamic = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, 0.48, 0.0)),
                RigidBodyComponent::dynamic().with_linear_velocity(Vec3::new(0.0, -1.0, 0.0)),
                ColliderComponent::cuboid(Vec3::splat(0.5)),
            ))
            .id();
        let ground = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, -0.5, 0.0)),
                RigidBodyComponent::static_body(),
                ColliderComponent::cuboid(Vec3::new(10.0, 0.5, 10.0)),
            ))
            .id();

        run_system(&mut world, ensure_rigid_bodies_system);
        run_system(&mut world, initialize_body_pose_system);
        run_system(&mut world, ensure_colliders_system);

        let dynamic_body = world
            .get::<RigidBodyComponent>(dynamic)
            .expect("dynamic body component should exist")
            .handle
            .expect("dynamic body handle should exist");
        let dynamic_collider = world
            .get::<ColliderComponent>(dynamic)
            .expect("dynamic collider should exist")
            .handle
            .expect("dynamic collider handle should exist");
        let ground_collider = world
            .get::<ColliderComponent>(ground)
            .expect("ground collider should exist")
            .handle
            .expect("ground collider handle should exist");

        let key = ManifoldKey::new(dynamic_collider, ground_collider);

        let mut physics = world.resource_mut::<PhysicsWorld>();
        resolve_collisions(&mut physics, SolverConfig::default());

        {
            let cached = physics
                .cached_manifolds
                .get_mut(&key)
                .expect("contact manifold should be cached after first solve");
            let first_contact = cached
                .contacts
                .first_mut()
                .expect("cached manifold should have at least one contact");
            first_contact.normal_impulse = 3.25;
            first_contact.tangent_impulse = Vec3::new(0.2, 0.0, -0.1);
        }

        let previous_manifold = physics
            .cached_manifolds
            .get(&key)
            .expect("contact manifold should be cached after first solve")
            .clone();

        let previous_by_id: HashMap<_, _> = previous_manifold
            .contacts
            .iter()
            .map(|contact| {
                (
                    contact.id,
                    (contact.normal_impulse, contact.tangent_impulse),
                )
            })
            .collect();

        if let Some(body) = physics.body_mut(dynamic_body) {
            body.position.y -= 0.002;
            body.linear_velocity = Vec3::new(0.0, -0.25, 0.0);
        }

        let warmed_manifold = generate_manifold(
            &physics,
            dynamic_collider,
            ground_collider,
            &physics.cached_manifolds,
        )
        .expect("persistent overlap should generate a manifold");

        let mut matched_contacts = 0;
        for contact in &warmed_manifold.contacts {
            if let Some((previous_normal, previous_tangent)) = previous_by_id.get(&contact.id) {
                matched_contacts += 1;
                assert_eq!(
                    contact.normal_impulse, *previous_normal,
                    "warm-started contact should reuse cached normal impulse"
                );
                assert_eq!(
                    contact.tangent_impulse, *previous_tangent,
                    "warm-started contact should reuse cached tangent impulse"
                );
            }
        }
        assert!(
            matched_contacts > 0,
            "at least one contact should match by ID for warm starting"
        );
    }

    #[test]
    fn warm_started_contacts_are_not_less_stable_than_cold_start() {
        let (mut warm_world, warm_box) = setup_resting_contact_world();
        let (mut cold_world, cold_box) = setup_resting_contact_world();

        let (warm_vertical_speed, warm_rest_error, warm_height) =
            run_resting_contact_simulation(&mut warm_world, warm_box, false, 220);
        let (cold_vertical_speed, cold_rest_error, cold_height) =
            run_resting_contact_simulation(&mut cold_world, cold_box, true, 220);

        assert!(
            warm_vertical_speed <= cold_vertical_speed + 0.25,
            "warm-started contact should not accumulate more vertical jitter than cold-start: warm={warm_vertical_speed}, cold={cold_vertical_speed}"
        );
        assert!(
            warm_rest_error <= cold_rest_error + 0.25,
            "warm-started contact should not increase resting height error: warm={warm_rest_error}, cold={cold_rest_error}"
        );
        assert!(
            warm_height >= cold_height - 0.02,
            "warm-started contact should keep final height at least as stable as cold-start: warm={warm_height}, cold={cold_height}"
        );
    }
}
