//! Gameplay-facing scene picking helpers.

use glam::{Mat4, Vec2, Vec3, Vec4};
use oxide_camera::CameraComponent;
use oxide_ecs::entity::Entity;
use oxide_ecs::world::World;
use oxide_transform::{is_visible, GlobalTransform, TransformComponent};

use crate::{MeshFilter, MeshPrimitive, RenderBounds, RenderLayers, RenderMesh};

/// World-space ray used for scene picking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScenePickRay {
    /// Ray origin in world space.
    pub origin: Vec3,
    /// Normalized ray direction in world space.
    pub direction: Vec3,
}

impl ScenePickRay {
    /// Creates a ray when `direction` can be normalized.
    pub fn new(origin: Vec3, direction: Vec3) -> Option<Self> {
        let direction = direction.normalize_or_zero();
        (direction != Vec3::ZERO).then_some(Self { origin, direction })
    }
}

/// Result of a scene pick query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScenePickHit {
    /// Picked entity.
    pub entity: Entity,
    /// Distance from ray origin to the world-space hit point.
    pub distance: f32,
}

/// Converts a cursor position inside a viewport into a world-space pick ray.
pub fn viewport_pick_ray(
    camera: &oxide_math::prelude::Camera,
    cursor: Vec2,
    viewport_size: [f32; 2],
) -> Option<ScenePickRay> {
    if viewport_size[0] <= 0.0 || viewport_size[1] <= 0.0 {
        return None;
    }

    let ndc_x = (cursor.x / viewport_size[0]) * 2.0 - 1.0;
    let ndc_y = 1.0 - (cursor.y / viewport_size[1]) * 2.0;
    let view_proj = camera.view_projection_matrix(viewport_size[0] / viewport_size[1]);
    let inv = view_proj.inverse();
    let near = unproject(inv, Vec3::new(ndc_x, ndc_y, -1.0));
    let far = unproject(inv, Vec3::new(ndc_x, ndc_y, 1.0));

    ScenePickRay::new(camera.position, far - near)
}

/// Picks the nearest renderable entity intersected by `ray`.
///
/// Primitive `RenderMesh` entities use their primitive shape. Mesh-handle
/// entities use explicit `RenderBounds` when present, matching renderer culling
/// metadata. Hidden entities and render layers outside `camera_layers` are
/// skipped.
pub fn pick_scene(
    world: &mut World,
    ray: ScenePickRay,
    camera_layers: RenderLayers,
) -> Option<ScenePickHit> {
    let mut candidates = Vec::new();

    {
        let mut query = world.query::<(Entity, &RenderMesh)>();
        candidates.extend(
            query
                .iter(world)
                .map(|(entity, mesh)| (entity, PickShape::Primitive(mesh.primitive))),
        );
    }

    {
        let mut query = world.query::<(Entity, &MeshFilter)>();
        candidates.extend(query.iter(world).filter_map(|(entity, _)| {
            world
                .get::<RenderBounds>(entity)
                .copied()
                .map(|bounds| (entity, PickShape::Sphere(bounds)))
        }));
    }

    candidates
        .into_iter()
        .filter(|(entity, _)| is_visible(world, *entity))
        .filter(|(entity, _)| render_layers(world, *entity).intersects(camera_layers))
        .filter_map(|(entity, shape)| {
            let model = entity_model_matrix(world, entity);
            intersect_shape(ray, model, shape).map(|distance| ScenePickHit { entity, distance })
        })
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
}

/// Picks with the first active scene camera, returning the hit and source ray.
pub fn pick_scene_from_viewport(
    world: &mut World,
    cursor: Vec2,
    viewport_size: [f32; 2],
) -> Option<(ScenePickRay, ScenePickHit)> {
    let (camera, layers) = active_camera(world)?;
    let ray = viewport_pick_ray(&camera.0, cursor, viewport_size)?;
    pick_scene(world, ray, layers).map(|hit| (ray, hit))
}

#[derive(Clone, Copy, Debug)]
enum PickShape {
    Primitive(MeshPrimitive),
    Sphere(RenderBounds),
}

fn active_camera(world: &mut World) -> Option<(CameraComponent, RenderLayers)> {
    let mut query = world.query::<(Entity, &CameraComponent)>();
    let cameras = query
        .iter(world)
        .map(|(entity, camera)| (entity, *camera))
        .collect::<Vec<_>>();
    cameras
        .into_iter()
        .next()
        .map(|(entity, camera)| (camera, render_layers(world, entity)))
}

fn render_layers(world: &World, entity: Entity) -> RenderLayers {
    world
        .get::<RenderLayers>(entity)
        .copied()
        .unwrap_or_default()
}

fn unproject(inv_view_proj: Mat4, ndc: Vec3) -> Vec3 {
    let point = inv_view_proj * Vec4::new(ndc.x, ndc.y, ndc.z, 1.0);
    point.truncate() / point.w
}

fn entity_model_matrix(world: &World, entity: Entity) -> Mat4 {
    if let Some(global) = world.get::<GlobalTransform>(entity) {
        global.matrix
    } else if let Some(local) = world.get::<TransformComponent>(entity) {
        local.to_matrix()
    } else {
        Mat4::IDENTITY
    }
}

fn intersect_shape(ray: ScenePickRay, model: Mat4, shape: PickShape) -> Option<f32> {
    match shape {
        PickShape::Primitive(primitive) => intersect_primitive(ray, model, primitive),
        PickShape::Sphere(bounds) => intersect_render_bounds(ray, model, bounds),
    }
}

fn intersect_primitive(ray: ScenePickRay, model: Mat4, primitive: MeshPrimitive) -> Option<f32> {
    let inv_model = model.inverse();
    let local_origin = inv_model.transform_point3(ray.origin);
    let local_direction = inv_model
        .transform_vector3(ray.direction)
        .normalize_or_zero();
    if local_direction == Vec3::ZERO {
        return None;
    }

    let local_t = match primitive {
        MeshPrimitive::Cube => intersect_unit_cube(local_origin, local_direction),
        MeshPrimitive::Sphere { .. } => intersect_unit_sphere(local_origin, local_direction),
    }?;
    let local_hit = local_origin + local_direction * local_t;
    let world_hit = model.transform_point3(local_hit);
    Some((world_hit - ray.origin).length())
}

fn intersect_render_bounds(ray: ScenePickRay, model: Mat4, bounds: RenderBounds) -> Option<f32> {
    let center = model.transform_point3(bounds.center);
    let radius = bounds.radius * model_scale_radius(model);
    intersect_sphere(ray.origin - center, ray.direction, radius)
}

fn intersect_unit_sphere(origin: Vec3, direction: Vec3) -> Option<f32> {
    intersect_sphere(origin, direction, 0.5)
}

fn intersect_sphere(origin: Vec3, direction: Vec3, radius: f32) -> Option<f32> {
    let a = direction.length_squared();
    let b = 2.0 * origin.dot(direction);
    let c = origin.length_squared() - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }

    let sqrt = discriminant.sqrt();
    let near = (-b - sqrt) / (2.0 * a);
    let far = (-b + sqrt) / (2.0 * a);
    [near, far]
        .into_iter()
        .filter(|t| *t >= 0.0)
        .min_by(|a, b| a.total_cmp(b))
}

fn intersect_unit_cube(origin: Vec3, direction: Vec3) -> Option<f32> {
    let mut t_min = 0.0f32;
    let mut t_max = f32::INFINITY;

    for axis in 0..3 {
        let origin_axis = origin[axis];
        let direction_axis = direction[axis];
        if direction_axis.abs() < f32::EPSILON {
            if !(-0.5..=0.5).contains(&origin_axis) {
                return None;
            }
            continue;
        }

        let inv = 1.0 / direction_axis;
        let mut t0 = (-0.5 - origin_axis) * inv;
        let mut t1 = (0.5 - origin_axis) * inv;
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        t_min = t_min.max(t0);
        t_max = t_max.min(t1);
        if t_max < t_min {
            return None;
        }
    }

    Some(t_min)
}

fn model_scale_radius(model: Mat4) -> f32 {
    model
        .x_axis
        .truncate()
        .length()
        .max(model.y_axis.truncate().length())
        .max(model.z_axis.truncate().length())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        transform_propagate_system, visibility_propagate_system, RenderMaterial,
        TransformComponent, Visibility,
    };

    #[test]
    fn pick_scene_selects_nearest_visible_layered_mesh() {
        let mut world = World::new();
        let near = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, 0.0, -2.0)),
                GlobalTransform::default(),
                RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
            ))
            .id();
        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, 0.0, -5.0)),
            GlobalTransform::default(),
            RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
        ));
        world.spawn((
            Visibility::Hidden,
            TransformComponent::from_position(Vec3::new(0.0, 0.0, -1.0)),
            GlobalTransform::default(),
            RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
        ));
        transform_propagate_system(&mut world);
        visibility_propagate_system(&mut world);

        let hit = pick_scene(
            &mut world,
            ScenePickRay::new(Vec3::ZERO, Vec3::NEG_Z).unwrap(),
            RenderLayers::default(),
        )
        .expect("ray should hit");

        assert_eq!(hit.entity, near);
    }

    #[test]
    fn pick_scene_filters_by_render_layers() {
        let mut world = World::new();
        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, 0.0, -1.0)),
            GlobalTransform::default(),
            RenderLayers::layer(2),
            RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
        ));
        transform_propagate_system(&mut world);

        let ray = ScenePickRay::new(Vec3::ZERO, Vec3::NEG_Z).unwrap();
        assert!(pick_scene(&mut world, ray, RenderLayers::default()).is_none());
        assert!(pick_scene(&mut world, ray, RenderLayers::layer(2)).is_some());
    }

    #[test]
    fn pick_scene_uses_mesh_filter_render_bounds() {
        let mut world = World::new();
        let entity = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, 0.0, -3.0)),
                GlobalTransform::default(),
                MeshFilter::new(oxide_asset::Handle::new(7)),
                RenderBounds::from_radius(1.0),
            ))
            .id();
        transform_propagate_system(&mut world);

        let hit = pick_scene(
            &mut world,
            ScenePickRay::new(Vec3::ZERO, Vec3::NEG_Z).unwrap(),
            RenderLayers::default(),
        )
        .expect("bounds should be pickable");

        assert_eq!(hit.entity, entity);
    }
}
