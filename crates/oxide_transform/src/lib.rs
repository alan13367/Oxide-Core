//! Transform and hierarchy components plus propagation utilities.

use glam::{Mat4, Quat, Vec3};
use oxide_ecs::entity::Entity;
use oxide_ecs::world::World;
use oxide_ecs::{query, Component};
use oxide_math::transform::Transform;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parent(pub Entity);

#[derive(Component, Clone, Debug, Default)]
pub struct Children(pub Vec<Entity>);

impl Children {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn with(entities: Vec<Entity>) -> Self {
        Self(entities)
    }

    pub fn push(&mut self, entity: Entity) {
        self.0.push(entity);
    }

    pub fn remove(&mut self, entity: Entity) {
        self.0.retain(|&e| e != entity);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.iter().copied()
    }
}

#[derive(Component, Clone, Debug)]
pub struct TransformComponent {
    pub transform: Transform,
    pub is_dirty: bool,
}

impl Default for TransformComponent {
    fn default() -> Self {
        Self {
            transform: Transform {
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            is_dirty: true,
        }
    }
}

impl TransformComponent {
    pub fn new(transform: Transform) -> Self {
        Self {
            transform,
            is_dirty: true,
        }
    }

    pub fn from_position(position: Vec3) -> Self {
        Self::new(Transform {
            position,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        })
    }

    pub fn from_position_rotation(position: Vec3, rotation: Quat) -> Self {
        Self::new(Transform {
            position,
            rotation,
            scale: Vec3::ONE,
        })
    }

    pub fn to_matrix(&self) -> Mat4 {
        self.transform.to_matrix()
    }

    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
    }

    pub fn clear_dirty(&mut self) {
        self.is_dirty = false;
    }

    pub fn set_transform(&mut self, transform: Transform) {
        self.transform = transform;
        self.mark_dirty();
    }

    pub fn transform_mut(&mut self) -> &mut Transform {
        self.mark_dirty();
        &mut self.transform
    }
}

impl From<Transform> for TransformComponent {
    fn from(transform: Transform) -> Self {
        Self::new(transform)
    }
}

impl From<TransformComponent> for Transform {
    fn from(component: TransformComponent) -> Self {
        component.transform
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct GlobalTransform {
    pub matrix: Mat4,
}

impl Default for GlobalTransform {
    fn default() -> Self {
        Self {
            matrix: Mat4::IDENTITY,
        }
    }
}

impl GlobalTransform {
    pub fn from_matrix(matrix: Mat4) -> Self {
        Self { matrix }
    }

    pub fn identity() -> Self {
        Self {
            matrix: Mat4::IDENTITY,
        }
    }

    pub fn position(&self) -> Vec3 {
        self.matrix.col(3).truncate()
    }

    pub fn mul(&self, other: &GlobalTransform) -> GlobalTransform {
        GlobalTransform {
            matrix: self.matrix * other.matrix,
        }
    }
}

pub fn attach_child(world: &mut World, parent: Entity, child: Entity) {
    if let Some(existing_parent) = world.get::<Parent>(child).copied() {
        if existing_parent.0 != parent {
            if let Some(old_children) = world.get_mut::<Children>(existing_parent.0) {
                old_children.remove(child);
            }
        }
    }

    world.entity_mut(child).insert(Parent(parent));

    if let Some(children) = world.get_mut::<Children>(parent) {
        if !children.0.contains(&child) {
            children.push(child);
        }
    } else {
        world.entity_mut(parent).insert(Children::with(vec![child]));
    }

    mark_subtree_dirty(world, child);
}

pub fn detach_child(world: &mut World, parent: Entity, child: Entity) {
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.remove(child);
    }
    world.entity_mut(child).remove::<Parent>();
    mark_subtree_dirty(world, child);
}

pub fn mark_subtree_dirty(world: &mut World, root: Entity) {
    if let Some(local) = world.get_mut::<TransformComponent>(root) {
        local.mark_dirty();
    }

    let children = world
        .get::<Children>(root)
        .map(|c| c.0.clone())
        .unwrap_or_default();

    for child in children {
        mark_subtree_dirty(world, child);
    }
}

pub fn transform_propagate_system(world: &mut World) {
    let root_entities: Vec<Entity> = {
        let mut roots = Vec::new();
        let mut query = world
            .query_filtered::<Entity, (query::With<TransformComponent>, query::Without<Parent>)>();
        for entity in query.iter(world) {
            roots.push(entity);
        }
        roots
    };

    for root in root_entities {
        propagate_from_root(world, root, GlobalTransform::identity(), false);
    }
}

fn propagate_from_root(
    world: &mut World,
    entity: Entity,
    parent_global: GlobalTransform,
    parent_changed: bool,
) {
    let (local_matrix, local_dirty) = match world.get::<TransformComponent>(entity) {
        Some(transform) => (transform.to_matrix(), transform.is_dirty),
        None => (Mat4::IDENTITY, false),
    };

    let computed_global = GlobalTransform::from_matrix(parent_global.matrix * local_matrix);
    let existing_global = world.get::<GlobalTransform>(entity).copied();
    let should_update = parent_changed || local_dirty || existing_global.is_none();

    if should_update {
        if let Some(global) = world.get_mut::<GlobalTransform>(entity) {
            *global = computed_global;
        } else {
            world.entity_mut(entity).insert(computed_global);
        }
    }

    if local_dirty {
        if let Some(local) = world.get_mut::<TransformComponent>(entity) {
            local.clear_dirty();
        }
    }

    let current_global = if should_update {
        computed_global
    } else {
        existing_global.unwrap_or(computed_global)
    };

    let children = world
        .get::<Children>(entity)
        .map(|children| children.0.clone())
        .unwrap_or_default();

    for child in children {
        propagate_from_root(world, child, current_global, should_update);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn transform_propagation_works_for_parent_child() {
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
            ))
            .id();

        attach_child(&mut world, root, child);
        transform_propagate_system(&mut world);

        let root_global = world
            .get::<GlobalTransform>(root)
            .expect("root global transform should exist");
        assert_eq!(root_global.position(), Vec3::new(1.0, 0.0, 0.0));

        let child_global = world
            .get::<GlobalTransform>(child)
            .expect("child global transform should exist");
        assert_eq!(child_global.position(), Vec3::new(1.0, 2.0, 0.0));
    }

    #[test]
    fn dirty_transform_marks_and_clears() {
        let mut world = World::new();
        let entity = world
            .spawn((TransformComponent::default(), GlobalTransform::default()))
            .id();

        transform_propagate_system(&mut world);
        assert!(
            !world
                .get::<TransformComponent>(entity)
                .expect("transform component should exist")
                .is_dirty
        );

        let transform = world
            .get_mut::<TransformComponent>(entity)
            .expect("transform component should exist");
        transform.transform_mut().position = Vec3::new(3.0, 0.0, 0.0);

        transform_propagate_system(&mut world);

        let global = world
            .get::<GlobalTransform>(entity)
            .expect("global transform should exist");
        assert_eq!(global.position(), Vec3::new(3.0, 0.0, 0.0));
        assert!(
            !world
                .get::<TransformComponent>(entity)
                .expect("transform component should exist")
                .is_dirty
        );
    }
}
