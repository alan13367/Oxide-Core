//! glTF scene hierarchy spawning utilities

use oxide_ecs::entity::Entity;
use oxide_ecs::world::World;
use oxide_ecs::Component;
use oxide_renderer::gltf::{GltfNode, GltfScene};
use oxide_transform::{attach_child, GlobalTransform, TransformComponent};

use oxide_math::transform::Transform;

/// Component storing the source glTF mesh index for an entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GltfMeshRef {
    pub mesh_index: usize,
}

/// Spawns glTF nodes into ECS while preserving the node hierarchy.
///
/// Returns the root entities created from the scene.
pub fn spawn_gltf_scene_hierarchy(world: &mut World, scene: &GltfScene) -> Vec<Entity> {
    scene
        .nodes
        .iter()
        .map(|node| spawn_gltf_node(world, node, None))
        .collect()
}

fn spawn_gltf_node(world: &mut World, node: &GltfNode, parent: Option<Entity>) -> Entity {
    let mut entity_builder = world.spawn((
        TransformComponent::new(Transform {
            position: node.translation,
            rotation: node.rotation,
            scale: node.scale,
        }),
        GlobalTransform::default(),
    ));

    if let Some(mesh_index) = node.mesh_index {
        entity_builder.insert(GltfMeshRef { mesh_index });
    }

    let entity = entity_builder.id();

    if let Some(parent_entity) = parent {
        attach_child(world, parent_entity, entity);
    }

    for child in &node.children {
        spawn_gltf_node(world, child, Some(entity));
    }

    entity
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec3};
    use oxide_transform::{Children, Parent};

    #[test]
    fn gltf_hierarchy_spawns_parent_child_relationships() {
        let mut world = World::new();

        let scene = GltfScene {
            meshes: Vec::new(),
            nodes: vec![GltfNode {
                name: Some("root".to_string()),
                mesh_index: None,
                translation: Vec3::new(1.0, 0.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
                children: vec![GltfNode {
                    name: Some("child".to_string()),
                    mesh_index: Some(0),
                    translation: Vec3::new(0.0, 2.0, 0.0),
                    rotation: Quat::IDENTITY,
                    scale: Vec3::ONE,
                    children: Vec::new(),
                }],
            }],
        };

        let roots = spawn_gltf_scene_hierarchy(&mut world, &scene);
        assert_eq!(roots.len(), 1);

        let root = roots[0];
        let children = world.get::<Children>(root).unwrap();
        assert_eq!(children.len(), 1);

        let child = children.iter().next().unwrap();
        let parent = world.get::<Parent>(child).unwrap();
        assert_eq!(parent.0, root);
        assert!(world.get::<GltfMeshRef>(child).is_some());
    }
}
