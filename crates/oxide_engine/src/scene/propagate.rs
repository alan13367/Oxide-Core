//! Transform propagation system for scene graph hierarchy

use oxide_ecs::world::World;

use super::{Children, GlobalTransform, Parent, TransformComponent};

/// Propagates transforms through the scene hierarchy.
/// Call this in PostUpdate after modifying transforms.
pub fn transform_propagate_system(world: &mut World) {
    let root_entities: Vec<oxide_ecs::entity::Entity> = {
        let mut roots = Vec::new();

        let mut transform_query = world.query_filtered::<oxide_ecs::entity::Entity, (
            oxide_ecs::query::With<TransformComponent>,
            oxide_ecs::query::Without<Parent>,
        )>();

        for entity in transform_query.iter(world) {
            roots.push(entity);
        }
        roots
    };

    for root_entity in root_entities {
        propagate_from_root(world, root_entity, GlobalTransform::identity());
    }
}

fn propagate_from_root(
    world: &mut World,
    entity: oxide_ecs::entity::Entity,
    parent_global: GlobalTransform,
) {
    let local_matrix = {
        let maybe_transform = world.get::<TransformComponent>(entity);
        match maybe_transform {
            Some(transform) => transform.to_matrix(),
            None => glam::Mat4::IDENTITY,
        }
    };

    let global = GlobalTransform::from_matrix(parent_global.matrix * local_matrix);

    if let Some(global_transform) = world.get_mut::<GlobalTransform>(entity) {
        *global_transform = global;
    }

    let children: Vec<oxide_ecs::entity::Entity> = {
        let maybe_children = world.get::<Children>(entity);
        match maybe_children {
            Some(children) => children.0.clone(),
            None => Vec::new(),
        }
    };

    for child in children {
        propagate_from_root(world, child, global);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn test_transform_propagation() {
        let mut world = World::new();

        let root = world
            .spawn((
                TransformComponent::from_position(Vec3::new(1.0, 0.0, 0.0)),
                GlobalTransform::default(),
            ))
            .id();

        let child = world
            .spawn((
                TransformComponent::from_position(Vec3::new(0.0, 2.0, 0.0)),
                GlobalTransform::default(),
                Parent(root),
            ))
            .id();

        world.entity_mut(root).insert(Children::with(vec![child]));

        transform_propagate_system(&mut world);

        let root_global = world.get::<GlobalTransform>(root).unwrap();
        assert_eq!(root_global.position(), Vec3::new(1.0, 0.0, 0.0));

        let child_global = world.get::<GlobalTransform>(child).unwrap();
        assert_eq!(child_global.position(), Vec3::new(1.0, 2.0, 0.0));
    }
}
