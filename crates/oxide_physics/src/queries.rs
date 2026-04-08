//! Physics query API for raycasts, sphere casts, and overlap tests.
//!
//! Provides efficient spatial queries against the physics world.

use glam::{Quat, Vec3};
use oxide_engine::prelude::Entity;

use crate::components::{BodyId, ColliderId, ColliderShape};
use crate::resources::PhysicsWorld;

/// Result of a raycast query.
#[derive(Clone, Debug)]
pub struct RaycastHit {
    /// The entity that was hit.
    pub entity: Entity,
    /// The body that was hit.
    pub body_id: BodyId,
    /// The collider that was hit.
    pub collider_id: ColliderId,
    /// World-space point of intersection.
    pub point: Vec3,
    /// World-space normal at the hit point.
    pub normal: Vec3,
    /// Distance from ray origin to hit point.
    pub distance: f32,
}

/// Result of a shape cast (sweep) query.
#[derive(Clone, Debug)]
pub struct ShapeCastHit {
    /// The entity that was hit.
    pub entity: Entity,
    /// The body that was hit.
    pub body_id: BodyId,
    /// The collider that was hit.
    pub collider_id: ColliderId,
    /// World-space point of intersection.
    pub point: Vec3,
    /// World-space normal at the hit point.
    pub normal: Vec3,
    /// Time of impact (0.0 to 1.0 along the cast direction).
    pub time_of_impact: f32,
}

impl PhysicsWorld {
    /// Cast a ray and find the closest hit.
    ///
    /// # Arguments
    /// * `origin` - Starting point of the ray
    /// * `direction` - Direction of the ray (should be normalized)
    /// * `max_distance` - Maximum distance to check
    ///
    /// # Returns
    /// The closest hit, if any.
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<RaycastHit> {
        let mut closest_hit: Option<RaycastHit> = None;
        let mut closest_dist = max_distance;

        for (collider_id, collider) in &self.colliders {
            let Some(body) = self.body(collider.body_id) else {
                continue;
            };

            // Ray-shape intersection
            if let Some((point, normal, distance)) = ray_shape_intersection(
                origin,
                direction,
                body.position,
                body.rotation,
                collider.shape,
            ) {
                if distance < closest_dist && distance >= 0.0 {
                    closest_dist = distance;
                    closest_hit = Some(RaycastHit {
                        entity: body.entity,
                        body_id: body.id,
                        collider_id: *collider_id,
                        point,
                        normal,
                        distance,
                    });
                }
            }
        }

        closest_hit
    }

    /// Cast a ray and find all hits.
    ///
    /// # Arguments
    /// * `origin` - Starting point of the ray
    /// * `direction` - Direction of the ray (should be normalized)
    /// * `max_distance` - Maximum distance to check
    ///
    /// # Returns
    /// All hits sorted by distance.
    pub fn raycast_all(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Vec<RaycastHit> {
        let mut hits = Vec::new();

        for (collider_id, collider) in &self.colliders {
            let Some(body) = self.body(collider.body_id) else {
                continue;
            };

            if let Some((point, normal, distance)) = ray_shape_intersection(
                origin,
                direction,
                body.position,
                body.rotation,
                collider.shape,
            ) {
                if distance >= 0.0 && distance <= max_distance {
                    hits.push(RaycastHit {
                        entity: body.entity,
                        body_id: body.id,
                        collider_id: *collider_id,
                        point,
                        normal,
                        distance,
                    });
                }
            }
        }

        hits.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits
    }

    /// Cast a sphere and find the closest hit.
    ///
    /// # Arguments
    /// * `origin` - Starting point of the sphere center
    /// * `direction` - Direction of the cast (should be normalized)
    /// * `radius` - Radius of the sphere to cast
    /// * `max_distance` - Maximum distance to check
    ///
    /// # Returns
    /// The closest hit, if any.
    pub fn sphere_cast(
        &self,
        origin: Vec3,
        direction: Vec3,
        radius: f32,
        max_distance: f32,
    ) -> Option<ShapeCastHit> {
        let mut closest_hit: Option<ShapeCastHit> = None;
        let mut closest_toi = 1.0;

        for (collider_id, collider) in &self.colliders {
            let Some(body) = self.body(collider.body_id) else {
                continue;
            };

            if let Some((point, normal, toi)) = sphere_cast_shape(
                origin,
                direction,
                radius,
                body.position,
                body.rotation,
                collider.shape,
                max_distance,
            ) {
                if toi < closest_toi && toi >= 0.0 {
                    closest_toi = toi;
                    closest_hit = Some(ShapeCastHit {
                        entity: body.entity,
                        body_id: body.id,
                        collider_id: *collider_id,
                        point,
                        normal,
                        time_of_impact: toi,
                    });
                }
            }
        }

        closest_hit
    }

    /// Find all bodies overlapping a sphere.
    ///
    /// # Arguments
    /// * `center` - Center of the sphere
    /// * `radius` - Radius of the sphere
    ///
    /// # Returns
    /// All bodies whose colliders overlap the sphere.
    pub fn overlaps_sphere(&self, center: Vec3, radius: f32) -> Vec<(Entity, BodyId, ColliderId)> {
        let mut results = Vec::new();

        for (collider_id, collider) in &self.colliders {
            let Some(body) = self.body(collider.body_id) else {
                continue;
            };

            if sphere_shape_overlap(center, radius, body.position, body.rotation, collider.shape) {
                results.push((body.entity, body.id, *collider_id));
            }
        }

        results
    }

    /// Find all bodies overlapping a box.
    ///
    /// # Arguments
    /// * `center` - Center of the box
    /// * `half_extents` - Half-extents of the box
    /// * `rotation` - Rotation of the box
    ///
    /// # Returns
    /// All bodies whose colliders overlap the box.
    pub fn overlaps_box(
        &self,
        center: Vec3,
        half_extents: Vec3,
        rotation: Quat,
    ) -> Vec<(Entity, BodyId, ColliderId)> {
        let mut results = Vec::new();

        for (collider_id, collider) in &self.colliders {
            let Some(body) = self.body(collider.body_id) else {
                continue;
            };

            if box_shape_overlap(
                center,
                half_extents,
                rotation,
                body.position,
                body.rotation,
                collider.shape,
            ) {
                results.push((body.entity, body.id, *collider_id));
            }
        }

        results
    }
}

/// Ray-sphere intersection.
fn ray_sphere_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    sphere_center: Vec3,
    sphere_radius: f32,
) -> Option<(Vec3, Vec3, f32)> {
    let oc = ray_origin - sphere_center;
    let a = ray_dir.dot(ray_dir);
    let b = 2.0 * oc.dot(ray_dir);
    let c = oc.dot(oc) - sphere_radius * sphere_radius;
    let discriminant = b * b - 4.0 * a * c;

    if discriminant < 0.0 {
        return None;
    }

    let t = (-b - discriminant.sqrt()) / (2.0 * a);
    if t < 0.0 {
        return None;
    }

    let point = ray_origin + ray_dir * t;
    let normal = (point - sphere_center).normalize();
    Some((point, normal, t))
}

/// Ray-OBB intersection.
fn ray_obb_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    obb_center: Vec3,
    obb_rotation: Quat,
    obb_half_extents: Vec3,
) -> Option<(Vec3, Vec3, f32)> {
    // Transform ray to OBB local space
    let local_origin = obb_rotation.conjugate() * (ray_origin - obb_center);
    let local_dir = obb_rotation.conjugate() * ray_dir;

    // Ray-AABB intersection in local space
    let mut t_min = 0.0f32;
    let mut t_max = f32::MAX;
    let mut normal_axis = 0usize;
    let mut normal_sign = 1.0f32;

    for i in 0..3 {
        let extent = match i {
            0 => obb_half_extents.x,
            1 => obb_half_extents.y,
            _ => obb_half_extents.z,
        };

        let origin = match i {
            0 => local_origin.x,
            1 => local_origin.y,
            _ => local_origin.z,
        };

        let dir = match i {
            0 => local_dir.x,
            1 => local_dir.y,
            _ => local_dir.z,
        };

        if dir.abs() < 1e-6 {
            // Ray is parallel to slab
            if origin.abs() > extent {
                return None;
            }
        } else {
            let t1 = (-extent - origin) / dir;
            let t2 = (extent - origin) / dir;

            let (t_near, t_far, sign) = if t1 < t2 {
                (t1, t2, -1.0)
            } else {
                (t2, t1, 1.0)
            };

            if t_near > t_min {
                t_min = t_near;
                normal_axis = i;
                normal_sign = sign;
            }
            t_max = t_max.min(t_far);

            if t_min > t_max {
                return None;
            }
        }
    }

    if t_min < 0.0 {
        return None;
    }

    let point = ray_origin + ray_dir * t_min;
    let local_normal = match normal_axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    } * normal_sign;
    let normal = obb_rotation * local_normal;

    Some((point, normal, t_min))
}

/// Ray-shape intersection dispatcher.
fn ray_shape_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    shape_pos: Vec3,
    shape_rot: Quat,
    shape: ColliderShape,
) -> Option<(Vec3, Vec3, f32)> {
    match shape {
        ColliderShape::Sphere { radius } => {
            ray_sphere_intersection(ray_origin, ray_dir, shape_pos, radius)
        }
        ColliderShape::Cuboid { half_extents } => {
            ray_obb_intersection(ray_origin, ray_dir, shape_pos, shape_rot, half_extents)
        }
    }
}

/// Sphere-sphere sweep (sphere cast against sphere).
/// Returns (hit_point, hit_normal, time_of_impact).
fn sphere_cast_sphere(
    origin: Vec3,
    direction: Vec3,
    cast_radius: f32,
    sphere_center: Vec3,
    sphere_radius: f32,
    max_distance: f32,
) -> Option<(Vec3, Vec3, f32)> {
    // Combined radius
    let combined_radius = cast_radius + sphere_radius;

    // Vector from origin to sphere center
    let m = origin - sphere_center;
    let b = m.dot(direction);
    let c = m.dot(m) - combined_radius * combined_radius;

    // Exit if ray starts outside sphere and points away
    if c > 0.0 && b > 0.0 {
        return None;
    }

    let discriminant = b * b - c;

    // No intersection
    if discriminant < 0.0 {
        return None;
    }

    // Compute time of impact
    let toi = (-b - discriminant.sqrt()).max(0.0);

    if toi > max_distance {
        return None;
    }

    // Compute hit point and normal
    let hit_center = origin + direction * toi;
    let normal = (hit_center - sphere_center).normalize_or(Vec3::Y);
    let hit_point = hit_center - normal * cast_radius;

    Some((hit_point, normal, toi / max_distance))
}

/// Sphere-OBB sweep (sphere cast against OBB).
/// Returns (hit_point, hit_normal, time_of_impact).
fn sphere_cast_obb(
    origin: Vec3,
    direction: Vec3,
    cast_radius: f32,
    obb_center: Vec3,
    obb_rotation: Quat,
    obb_half_extents: Vec3,
    max_distance: f32,
) -> Option<(Vec3, Vec3, f32)> {
    // Expand OBB by cast radius
    let expanded_extents = obb_half_extents + Vec3::splat(cast_radius);

    // Transform ray to OBB local space
    let local_origin = obb_rotation.conjugate() * (origin - obb_center);
    let local_dir = obb_rotation.conjugate() * direction;

    // Ray-AABB intersection with expanded OBB
    let mut t_min = 0.0f32;
    let mut t_max = max_distance;
    let mut normal_axis = 0usize;
    let mut normal_sign = 1.0f32;

    for i in 0..3 {
        let extent = match i {
            0 => expanded_extents.x,
            1 => expanded_extents.y,
            _ => expanded_extents.z,
        };

        let local_origin_i = match i {
            0 => local_origin.x,
            1 => local_origin.y,
            _ => local_origin.z,
        };

        let local_dir_i = match i {
            0 => local_dir.x,
            1 => local_dir.y,
            _ => local_dir.z,
        };

        if local_dir_i.abs() < 1e-6 {
            // Parallel to slab
            if local_origin_i.abs() > extent {
                return None;
            }
        } else {
            let t1 = (-extent - local_origin_i) / local_dir_i;
            let t2 = (extent - local_origin_i) / local_dir_i;

            let (t_near, t_far, sign) = if t1 < t2 {
                (t1, t2, -1.0)
            } else {
                (t2, t1, 1.0)
            };

            if t_near > t_min {
                t_min = t_near;
                normal_axis = i;
                normal_sign = sign;
            }
            t_max = t_max.min(t_far);

            if t_min > t_max {
                return None;
            }
        }
    }

    if t_min < 0.0 {
        return None;
    }

    let toi = t_min / max_distance;
    let hit_center = origin + direction * t_min;
    let local_normal = match normal_axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    } * normal_sign;
    let normal = obb_rotation * local_normal;
    let hit_point = hit_center - normal * cast_radius;

    Some((hit_point, normal, toi))
}

/// Sphere-cast against shape dispatcher.
fn sphere_cast_shape(
    origin: Vec3,
    direction: Vec3,
    cast_radius: f32,
    shape_pos: Vec3,
    shape_rot: Quat,
    shape: ColliderShape,
    max_distance: f32,
) -> Option<(Vec3, Vec3, f32)> {
    match shape {
        ColliderShape::Sphere { radius } => sphere_cast_sphere(
            origin,
            direction,
            cast_radius,
            shape_pos,
            radius,
            max_distance,
        ),
        ColliderShape::Cuboid { half_extents } => sphere_cast_obb(
            origin,
            direction,
            cast_radius,
            shape_pos,
            shape_rot,
            half_extents,
            max_distance,
        ),
    }
}

/// Sphere-sphere overlap test.
fn sphere_sphere_overlap(center_a: Vec3, radius_a: f32, center_b: Vec3, radius_b: f32) -> bool {
    let dist_sq = (center_b - center_a).length_squared();
    let radius_sum = radius_a + radius_b;
    dist_sq < radius_sum * radius_sum
}

/// Sphere-OBB overlap test.
fn sphere_obb_overlap(
    sphere_center: Vec3,
    sphere_radius: f32,
    obb_center: Vec3,
    obb_rotation: Quat,
    obb_half_extents: Vec3,
) -> bool {
    // Transform sphere center to OBB local space
    let local_center = obb_rotation.conjugate() * (sphere_center - obb_center);

    // Find closest point on OBB to sphere center
    let closest = local_center.clamp(-obb_half_extents, obb_half_extents);

    // Check distance
    let dist_sq = (local_center - closest).length_squared();
    dist_sq < sphere_radius * sphere_radius
}

/// Sphere-shape overlap dispatcher.
fn sphere_shape_overlap(
    sphere_center: Vec3,
    sphere_radius: f32,
    shape_pos: Vec3,
    shape_rot: Quat,
    shape: ColliderShape,
) -> bool {
    match shape {
        ColliderShape::Sphere { radius } => {
            sphere_sphere_overlap(sphere_center, sphere_radius, shape_pos, radius)
        }
        ColliderShape::Cuboid { half_extents } => sphere_obb_overlap(
            sphere_center,
            sphere_radius,
            shape_pos,
            shape_rot,
            half_extents,
        ),
    }
}

/// OBB-OBB overlap test using SAT.
fn obb_obb_overlap(
    center_a: Vec3,
    half_extents_a: Vec3,
    rotation_a: Quat,
    center_b: Vec3,
    half_extents_b: Vec3,
    rotation_b: Quat,
) -> bool {
    let axes_a = [
        rotation_a * Vec3::X,
        rotation_a * Vec3::Y,
        rotation_a * Vec3::Z,
    ];
    let axes_b = [
        rotation_b * Vec3::X,
        rotation_b * Vec3::Y,
        rotation_b * Vec3::Z,
    ];

    let a_e = [half_extents_a.x, half_extents_a.y, half_extents_a.z];
    let b_e = [half_extents_b.x, half_extents_b.y, half_extents_b.z];

    let mut r = [[0.0f32; 3]; 3];
    let mut abs_r = [[0.0f32; 3]; 3];
    const EPS: f32 = 1.0e-6;

    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = axes_a[i].dot(axes_b[j]);
            abs_r[i][j] = r[i][j].abs() + EPS;
        }
    }

    let t = center_b - center_a;
    let t_a = [t.dot(axes_a[0]), t.dot(axes_a[1]), t.dot(axes_a[2])];

    // Test A's face normals
    for i in 0..3 {
        let ra = a_e[i];
        let rb = b_e[0] * abs_r[i][0] + b_e[1] * abs_r[i][1] + b_e[2] * abs_r[i][2];
        if t_a[i].abs() > ra + rb {
            return false;
        }
    }

    // Test B's face normals
    for j in 0..3 {
        let ra = a_e[0] * abs_r[0][j] + a_e[1] * abs_r[1][j] + a_e[2] * abs_r[2][j];
        let rb = b_e[j];
        let t_b = t_a[0] * r[0][j] + t_a[1] * r[1][j] + t_a[2] * r[2][j];
        if t_b.abs() > ra + rb {
            return false;
        }
    }

    // Test edge cross products
    for i in 0..3 {
        for j in 0..3 {
            let ra =
                a_e[(i + 1) % 3] * abs_r[(i + 2) % 3][j] + a_e[(i + 2) % 3] * abs_r[(i + 1) % 3][j];
            let rb =
                b_e[(j + 1) % 3] * abs_r[i][(j + 2) % 3] + b_e[(j + 2) % 3] * abs_r[i][(j + 1) % 3];
            let t_edge =
                (t_a[(i + 2) % 3] * r[(i + 1) % 3][j] - t_a[(i + 1) % 3] * r[(i + 2) % 3][j]).abs();
            if t_edge > ra + rb {
                return false;
            }
        }
    }

    true
}

/// Box-shape overlap dispatcher.
fn box_shape_overlap(
    box_center: Vec3,
    box_half_extents: Vec3,
    box_rotation: Quat,
    shape_pos: Vec3,
    shape_rot: Quat,
    shape: ColliderShape,
) -> bool {
    match shape {
        ColliderShape::Sphere { radius } => sphere_obb_overlap(
            shape_pos,
            radius,
            box_center,
            box_rotation,
            box_half_extents,
        ),
        ColliderShape::Cuboid { half_extents } => obb_obb_overlap(
            box_center,
            box_half_extents,
            box_rotation,
            shape_pos,
            half_extents,
            shape_rot,
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::components::{
        BodyId, ColliderComponent, ColliderId, ColliderShape, CollisionLayers, RigidBodyComponent,
    };
    use crate::resources::{PhysicsCollider, PhysicsWorld};
    use glam::Vec3;
    use oxide_engine::prelude::{CommandQueue, IntoSystem, Time, TransformComponent, World};

    fn setup_world() -> World {
        let mut world = World::new();
        world.insert_resource(Time::default());
        world.insert_resource(PhysicsWorld::default());
        world
    }

    fn run_system<S, Marker>(world: &mut World, system: S)
    where
        S: IntoSystem<Marker>,
    {
        let mut sys = system.into_system();
        let mut commands = CommandQueue::new();
        sys.run(world, &mut commands);
        commands.apply(world);
    }

    #[test]
    fn raycast_hits_sphere() {
        let mut world = setup_world();

        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, 0.0, 5.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::sphere(1.0),
        ));

        run_system(&mut world, super::super::ensure_rigid_bodies_system);
        run_system(&mut world, super::super::initialize_body_pose_system);
        run_system(&mut world, super::super::ensure_colliders_system);

        let physics = world.resource::<PhysicsWorld>();
        let hit = physics.raycast(Vec3::ZERO, Vec3::Z, 100.0);

        assert!(hit.is_some(), "raycast should hit the sphere");
        let hit = hit.unwrap();
        assert!(
            (hit.distance - 4.0).abs() < 0.1,
            "distance should be ~4, got {}",
            hit.distance
        );
    }

    #[test]
    fn raycast_misses_sphere() {
        let mut world = setup_world();

        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, 10.0, 0.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::sphere(1.0),
        ));

        run_system(&mut world, super::super::ensure_rigid_bodies_system);
        run_system(&mut world, super::super::initialize_body_pose_system);
        run_system(&mut world, super::super::ensure_colliders_system);

        let physics = world.resource::<PhysicsWorld>();
        let hit = physics.raycast(Vec3::ZERO, Vec3::Z, 100.0);

        assert!(hit.is_none(), "raycast should miss the sphere");
    }

    #[test]
    fn overlaps_sphere_finds_bodies() {
        let mut world = setup_world();

        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, 0.0, 0.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::sphere(1.0),
        ));

        world.spawn((
            TransformComponent::from_position(Vec3::new(5.0, 0.0, 0.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::sphere(1.0),
        ));

        run_system(&mut world, super::super::ensure_rigid_bodies_system);
        run_system(&mut world, super::super::initialize_body_pose_system);
        run_system(&mut world, super::super::ensure_colliders_system);

        let physics = world.resource::<PhysicsWorld>();
        let overlaps = physics.overlaps_sphere(Vec3::new(0.0, 0.0, 0.0), 2.0);

        assert_eq!(overlaps.len(), 1, "should find one overlapping sphere");
    }

    #[test]
    fn raycast_skips_dangling_colliders_without_early_return() {
        let mut world = setup_world();

        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, 0.0, 5.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::sphere(1.0),
        ));

        run_system(&mut world, super::super::ensure_rigid_bodies_system);
        run_system(&mut world, super::super::initialize_body_pose_system);
        run_system(&mut world, super::super::ensure_colliders_system);

        {
            let physics = world.resource_mut::<PhysicsWorld>();
            physics.colliders.insert(
                ColliderId(999_999),
                PhysicsCollider {
                    id: ColliderId(999_999),
                    body_id: BodyId(999_998),
                    shape: ColliderShape::sphere(0.5),
                    friction: 0.5,
                    restitution: 0.0,
                    is_sensor: false,
                    collision_layers: CollisionLayers::default(),
                },
            );
        }

        let physics = world.resource::<PhysicsWorld>();
        let hit = physics.raycast(Vec3::ZERO, Vec3::Z, 100.0);
        assert!(hit.is_some(), "raycast should still hit valid collider");
    }

    #[test]
    fn sphere_cast_skips_dangling_colliders_without_early_return() {
        let mut world = setup_world();

        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, 0.0, 5.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::sphere(1.0),
        ));

        run_system(&mut world, super::super::ensure_rigid_bodies_system);
        run_system(&mut world, super::super::initialize_body_pose_system);
        run_system(&mut world, super::super::ensure_colliders_system);

        {
            let physics = world.resource_mut::<PhysicsWorld>();
            physics.colliders.insert(
                ColliderId(888_888),
                PhysicsCollider {
                    id: ColliderId(888_888),
                    body_id: BodyId(888_887),
                    shape: ColliderShape::sphere(0.25),
                    friction: 0.5,
                    restitution: 0.0,
                    is_sensor: false,
                    collision_layers: CollisionLayers::default(),
                },
            );
        }

        let physics = world.resource::<PhysicsWorld>();
        let hit = physics.sphere_cast(Vec3::ZERO, Vec3::Z, 0.25, 100.0);
        assert!(hit.is_some(), "sphere cast should still hit valid collider");
    }

    #[test]
    fn sphere_cast_returns_surface_point() {
        let mut world = setup_world();

        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, 0.0, 5.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::sphere(1.0),
        ));

        run_system(&mut world, super::super::ensure_rigid_bodies_system);
        run_system(&mut world, super::super::initialize_body_pose_system);
        run_system(&mut world, super::super::ensure_colliders_system);

        let physics = world.resource::<PhysicsWorld>();
        let hit = physics
            .sphere_cast(Vec3::ZERO, Vec3::Z, 0.5, 100.0)
            .expect("sphere cast should hit the sphere");

        assert!(
            (hit.point.z - 4.0).abs() < 0.05,
            "expected surface point near z=4.0, got {}",
            hit.point.z
        );
        assert!(
            hit.point.z > 3.5,
            "sphere cast point should be on the swept sphere surface"
        );
    }
}
