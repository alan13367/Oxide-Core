//! ECS systems for the in-house physics runtime.

use std::collections::HashSet;

use glam::Vec3;
use oxide_engine::prelude::{Entity, Query, Res, ResMut, Time, TransformComponent};

use crate::components::{BodyId, ColliderComponent, ColliderShape, RigidBodyComponent};
use crate::resources::{Aabb, PhysicsWorld};

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
        body.force_accumulator = Vec3::ZERO;
    }
}

fn resolve_collisions(physics: &mut PhysicsWorld) {
    let dynamic_ids: Vec<_> = physics
        .bodies
        .values()
        .filter(|body| body.is_dynamic())
        .map(|body| body.id)
        .collect();

    let static_collider_ids: Vec<_> = physics
        .colliders
        .values()
        .filter(|collider| {
            physics
                .body(collider.body_id)
                .map(|body| !body.is_dynamic())
                .unwrap_or(false)
        })
        .map(|collider| collider.id)
        .collect();

    for dynamic_body_id in dynamic_ids {
        let dynamic_collider_ids: Vec<_> = physics
            .colliders
            .values()
            .filter(|collider| collider.body_id == dynamic_body_id)
            .map(|collider| collider.id)
            .collect();

        for dyn_collider_id in dynamic_collider_ids {
            for static_collider_id in &static_collider_ids {
                resolve_dynamic_static_pair(physics, dynamic_body_id, dyn_collider_id, *static_collider_id);
            }
        }
    }
}

fn resolve_dynamic_static_pair(
    physics: &mut PhysicsWorld,
    dynamic_body_id: BodyId,
    dynamic_collider_id: crate::components::ColliderId,
    static_collider_id: crate::components::ColliderId,
) {
    let (dyn_collider, dyn_position) = {
        let Some(collider) = physics.colliders.get(&dynamic_collider_id) else {
            return;
        };
        let Some(body) = physics.body(collider.body_id) else {
            return;
        };
        (collider.clone(), body.position)
    };

    let (static_collider, static_position) = {
        let Some(collider) = physics.colliders.get(&static_collider_id) else {
            return;
        };
        let Some(body) = physics.body(collider.body_id) else {
            return;
        };
        (collider.clone(), body.position)
    };

    if dyn_collider.is_sensor || static_collider.is_sensor {
        return;
    }

    let Some((normal, penetration)) = compute_collision(
        dyn_position,
        dyn_collider.shape,
        static_position,
        static_collider.shape,
    ) else {
        return;
    };

    let Some(dynamic_body) = physics.body_mut(dynamic_body_id) else {
        return;
    };
    if penetration <= 0.0 {
        return;
    }

    dynamic_body.position += normal * penetration;

    let vn = dynamic_body.linear_velocity.dot(normal);
    if vn < 0.0 {
        let restitution = dyn_collider.restitution.min(static_collider.restitution);
        dynamic_body.linear_velocity -= (1.0 + restitution) * vn * normal;

        let tangent_velocity = dynamic_body.linear_velocity - normal * dynamic_body.linear_velocity.dot(normal);
        let friction = (dyn_collider.friction + static_collider.friction) * 0.5;
        dynamic_body.linear_velocity -= tangent_velocity * friction.clamp(0.0, 1.0) * 0.2;
    }
}

fn compute_collision(
    dynamic_pos: Vec3,
    dynamic_shape: ColliderShape,
    static_pos: Vec3,
    static_shape: ColliderShape,
) -> Option<(Vec3, f32)> {
    match (dynamic_shape, static_shape) {
        (ColliderShape::Sphere { radius: ra }, ColliderShape::Sphere { radius: rb }) => {
            let delta = dynamic_pos - static_pos;
            let distance = delta.length();
            let target = ra + rb;
            if distance >= target {
                return None;
            }
            let normal = if distance > f32::EPSILON {
                delta / distance
            } else {
                Vec3::Y
            };
            Some((normal, target - distance))
        }
        _ => {
            let a = shape_to_aabb(dynamic_shape, dynamic_pos);
            let b = shape_to_aabb(static_shape, static_pos);
            if !a.intersects(&b) {
                return None;
            }

            let overlap_x = (a.max.x.min(b.max.x) - a.min.x.max(b.min.x)).max(0.0);
            let overlap_y = (a.max.y.min(b.max.y) - a.min.y.max(b.min.y)).max(0.0);
            let overlap_z = (a.max.z.min(b.max.z) - a.min.z.max(b.min.z)).max(0.0);
            let (penetration, axis) = if overlap_x <= overlap_y && overlap_x <= overlap_z {
                (overlap_x, 0)
            } else if overlap_y <= overlap_z {
                (overlap_y, 1)
            } else {
                (overlap_z, 2)
            };
            if penetration <= 0.0 {
                return None;
            }

            let dynamic_center = (a.min + a.max) * 0.5;
            let static_center = (b.min + b.max) * 0.5;
            let normal = match axis {
                0 => {
                    if dynamic_center.x >= static_center.x {
                        Vec3::X
                    } else {
                        -Vec3::X
                    }
                }
                1 => {
                    if dynamic_center.y >= static_center.y {
                        Vec3::Y
                    } else {
                        -Vec3::Y
                    }
                }
                _ => {
                    if dynamic_center.z >= static_center.z {
                        Vec3::Z
                    } else {
                        -Vec3::Z
                    }
                }
            };

            Some((normal, penetration + 0.001))
        }
    }
}

fn shape_to_aabb(shape: ColliderShape, position: Vec3) -> Aabb {
    match shape {
        ColliderShape::Cuboid { half_extents } => Aabb {
            min: position - half_extents,
            max: position + half_extents,
        },
        ColliderShape::Sphere { radius } => {
            let extents = Vec3::splat(radius);
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

    use glam::Vec3;
    use oxide_engine::prelude::{
        CommandQueue, IntoSystem, Time, TransformComponent, World,
    };

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
}
