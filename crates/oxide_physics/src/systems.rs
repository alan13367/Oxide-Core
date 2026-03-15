//! ECS systems for the in-house physics runtime.

use std::collections::HashSet;

use glam::{Quat, Vec3};
use oxide_engine::prelude::{Entity, Query, Res, ResMut, Time, TransformComponent};

use crate::components::{BodyId, ColliderComponent, ColliderId, ColliderShape, RigidBodyComponent};
use crate::resources::{Aabb, PhysicsWorld};

#[derive(Clone, Copy)]
struct Contact {
    a_body: BodyId,
    b_body: BodyId,
    normal: Vec3,      // From A to B.
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
    aabb: Aabb,
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
        rigid_body.handle = Some(handle);
        rigid_body.pending_initial_sync = true;
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
    mut query: Query<(&mut ColliderComponent, &RigidBodyComponent)>,
) {
    for (collider, rigid_body) in query.iter_mut() {
        if let Some(handle) = collider.handle {
            if physics.contains_collider(handle) {
                continue;
            }
            collider.handle = None;
        }

        let Some(body_id) = rigid_body.handle else {
            continue;
        };
        if !physics.contains_body(body_id) {
            continue;
        }

        let handle = physics.create_collider(body_id, *collider);
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
    let live_handles: HashSet<_> = query.iter().filter_map(|collider| collider.handle).collect();
    let to_remove: Vec<_> = physics
        .collider_handles()
        .filter(|handle| !live_handles.contains(handle))
        .collect();

    for handle in to_remove {
        physics.remove_collider(handle);
    }
}

pub fn physics_step_system(time: Res<Time>, mut physics: ResMut<PhysicsWorld>) {
    let frame_delta = time.delta_secs().min(0.25);
    physics.accumulator_seconds += frame_delta;

    let mut steps = 0;
    while physics.accumulator_seconds >= physics.fixed_dt && steps < physics.max_substeps {
        integrate_bodies(&mut physics);
        resolve_collisions(&mut physics);
        physics.accumulator_seconds -= physics.fixed_dt;
        steps += 1;
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
    for body in physics.bodies.values_mut() {
        if !body.is_dynamic() {
            continue;
        }

        let acceleration = gravity + body.force_accumulator * body.inverse_mass;
        body.linear_velocity += acceleration * dt;
        body.position += body.linear_velocity * dt;

        // Semi-implicit angular integration.
        let angular_step = body.angular_velocity * dt;
        if angular_step.length_squared() > f32::EPSILON {
            let angular_delta = Quat::from_scaled_axis(angular_step);
            body.rotation = (angular_delta * body.rotation).normalize();
        }

        body.force_accumulator = Vec3::ZERO;
    }
}

fn resolve_collisions(physics: &mut PhysicsWorld) {
    let candidate_pairs = broadphase_candidates(physics);
    let contacts: Vec<_> = candidate_pairs
        .into_iter()
        .filter_map(|(a, b)| generate_contact(physics, a, b))
        .collect();

    for contact in contacts {
        apply_contact(physics, contact);
    }
}

fn broadphase_candidates(physics: &PhysicsWorld) -> Vec<(ColliderId, ColliderId)> {
    let mut proxies = Vec::new();
    for collider in physics.colliders.values() {
        let Some(body) = physics.body(collider.body_id) else {
            continue;
        };
        proxies.push(BroadphaseProxy {
            collider_id: collider.id,
            body_id: collider.body_id,
            is_dynamic: body.is_dynamic(),
            aabb: shape_to_aabb(collider.shape, body.position, body.rotation),
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
            if !proxy.is_dynamic && !other.is_dynamic {
                continue;
            }
            if !proxy.aabb.intersects(&other.aabb) {
                continue;
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

    let (normal, penetration) = compute_collision(
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
        normal,
        penetration,
        restitution: collider_a.restitution.min(collider_b.restitution),
        friction: (collider_a.friction + collider_b.friction) * 0.5,
    })
}

fn apply_contact(physics: &mut PhysicsWorld, contact: Contact) {
    let inv_mass_a = physics
        .body(contact.a_body)
        .map(|body| body.inverse_mass)
        .unwrap_or(0.0);
    let inv_mass_b = physics
        .body(contact.b_body)
        .map(|body| body.inverse_mass)
        .unwrap_or(0.0);
    let inv_mass_sum = inv_mass_a + inv_mass_b;
    if inv_mass_sum <= f32::EPSILON {
        return;
    }

    // Positional correction.
    let slop = 0.001;
    let percent = 0.8;
    let correction_mag = (contact.penetration - slop).max(0.0) * percent;
    let correction = contact.normal * correction_mag;
    if inv_mass_a > 0.0 {
        if let Some(body) = physics.body_mut(contact.a_body) {
            body.position -= correction * (inv_mass_a / inv_mass_sum);
        }
    }
    if inv_mass_b > 0.0 {
        if let Some(body) = physics.body_mut(contact.b_body) {
            body.position += correction * (inv_mass_b / inv_mass_sum);
        }
    }

    let velocity_a = physics
        .body(contact.a_body)
        .map(|body| body.linear_velocity)
        .unwrap_or(Vec3::ZERO);
    let velocity_b = physics
        .body(contact.b_body)
        .map(|body| body.linear_velocity)
        .unwrap_or(Vec3::ZERO);

    let relative_velocity = velocity_b - velocity_a;
    let vel_along_normal = relative_velocity.dot(contact.normal);
    if vel_along_normal > 0.0 {
        return;
    }

    // Normal impulse.
    let impulse_mag = -(1.0 + contact.restitution) * vel_along_normal / inv_mass_sum;
    let impulse = contact.normal * impulse_mag;
    if inv_mass_a > 0.0 {
        if let Some(body) = physics.body_mut(contact.a_body) {
            body.linear_velocity -= impulse * inv_mass_a;
        }
    }
    if inv_mass_b > 0.0 {
        if let Some(body) = physics.body_mut(contact.b_body) {
            body.linear_velocity += impulse * inv_mass_b;
        }
    }

    // Tangential friction impulse.
    let velocity_a = physics
        .body(contact.a_body)
        .map(|body| body.linear_velocity)
        .unwrap_or(Vec3::ZERO);
    let velocity_b = physics
        .body(contact.b_body)
        .map(|body| body.linear_velocity)
        .unwrap_or(Vec3::ZERO);
    let relative_velocity = velocity_b - velocity_a;
    let tangent = relative_velocity - contact.normal * relative_velocity.dot(contact.normal);
    let tangent_len_sq = tangent.length_squared();
    if tangent_len_sq > f32::EPSILON {
        let tangent_dir = tangent / tangent_len_sq.sqrt();
        let jt = -relative_velocity.dot(tangent_dir) / inv_mass_sum;
        let max_friction = impulse_mag * contact.friction;
        let jt_clamped = jt.clamp(-max_friction, max_friction);
        let friction_impulse = tangent_dir * jt_clamped;
        if inv_mass_a > 0.0 {
            if let Some(body) = physics.body_mut(contact.a_body) {
                body.linear_velocity -= friction_impulse * inv_mass_a;
            }
        }
        if inv_mass_b > 0.0 {
            if let Some(body) = physics.body_mut(contact.b_body) {
                body.linear_velocity += friction_impulse * inv_mass_b;
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
    match (shape_a, shape_b) {
        (ColliderShape::Sphere { radius: ra }, ColliderShape::Sphere { radius: rb }) => {
            sphere_sphere_collision(position_a, ra, position_b, rb)
        }
        (ColliderShape::Sphere { radius }, ColliderShape::Cuboid { half_extents }) => {
            sphere_obb_collision(position_a, radius, position_b, rotation_b, half_extents)
        }
        (ColliderShape::Cuboid { half_extents }, ColliderShape::Sphere { radius }) => {
            sphere_obb_collision(position_b, radius, position_a, rotation_a, half_extents)
                .map(|(normal, penetration)| (-normal, penetration))
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
                axes: [rotation_a * Vec3::X, rotation_a * Vec3::Y, rotation_a * Vec3::Z],
                half_extents: extents_a,
            };
            let obb_b = Obb {
                center: position_b,
                axes: [rotation_b * Vec3::X, rotation_b * Vec3::Y, rotation_b * Vec3::Z],
                half_extents: extents_b,
            };
            obb_obb_collision(obb_a, obb_b)
        }
    }
}

fn sphere_sphere_collision(a: Vec3, ra: f32, b: Vec3, rb: f32) -> Option<(Vec3, f32)> {
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
    Some((normal, target - distance + 0.001))
}

fn sphere_obb_collision(
    sphere_center: Vec3,
    sphere_radius: f32,
    obb_center: Vec3,
    obb_rotation: Quat,
    obb_half_extents: Vec3,
) -> Option<(Vec3, f32)> {
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
        return Some((delta / distance, sphere_radius - distance + 0.001));
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
    Some((obb_rotation * normal_local, sphere_radius + face_distance + 0.001))
}

fn obb_obb_collision(a: Obb, b: Obb) -> Option<(Vec3, f32)> {
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

    let mut consider_axis = |axis: Vec3, distance: f32, ra: f32, rb: f32| -> Option<()> {
        let overlap = ra + rb - distance;
        if overlap < 0.0 {
            return None;
        }
        if overlap < best_penetration {
            best_penetration = overlap;
            best_normal = axis;
        }
        Some(())
    };

    for i in 0..3 {
        let ra = a_e[i];
        let rb = b_e[0] * abs_r[i][0] + b_e[1] * abs_r[i][1] + b_e[2] * abs_r[i][2];
        let distance = t[i].abs();
        let axis = if t[i] >= 0.0 { a.axes[i] } else { -a.axes[i] };
        consider_axis(axis, distance, ra, rb)?;
    }

    for j in 0..3 {
        let ra = a_e[0] * abs_r[0][j] + a_e[1] * abs_r[1][j] + a_e[2] * abs_r[2][j];
        let rb = b_e[j];
        let distance = (t[0] * r[0][j] + t[1] * r[1][j] + t[2] * r[2][j]).abs();
        let axis = if t_world.dot(b.axes[j]) >= 0.0 {
            b.axes[j]
        } else {
            -b.axes[j]
        };
        consider_axis(axis, distance, ra, rb)?;
    }

    for i in 0..3 {
        for j in 0..3 {
            let axis = a.axes[i].cross(b.axes[j]);
            let axis_len_sq = axis.length_squared();
            if axis_len_sq <= 1.0e-10 {
                continue;
            }

            let ra = a_e[(i + 1) % 3] * abs_r[(i + 2) % 3][j]
                + a_e[(i + 2) % 3] * abs_r[(i + 1) % 3][j];
            let rb = b_e[(j + 1) % 3] * abs_r[i][(j + 2) % 3]
                + b_e[(j + 2) % 3] * abs_r[i][(j + 1) % 3];
            let distance =
                (t[(i + 2) % 3] * r[(i + 1) % 3][j] - t[(i + 1) % 3] * r[(i + 2) % 3][j]).abs();

            let mut world_axis = axis / axis_len_sq.sqrt();
            if world_axis.dot(t_world) < 0.0 {
                world_axis = -world_axis;
            }
            consider_axis(world_axis, distance, ra, rb)?;
        }
    }

    Some((best_normal.normalize_or_zero(), best_penetration + 0.001))
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
    use std::time::Duration;

    use glam::{Quat, Vec3};
    use oxide_engine::prelude::{CommandQueue, IntoSystem, Time, TransformComponent, World};

    use super::{
        ensure_colliders_system, ensure_rigid_bodies_system, initialize_body_pose_system,
        physics_step_system, prune_orphan_bodies_system, prune_orphan_colliders_system,
        sync_transforms_system,
    };
    use crate::components::{ColliderComponent, RigidBodyComponent};
    use crate::resources::{PhysicsWorld, DEFAULT_FIXED_TIMESTEP};

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
        world
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
}
