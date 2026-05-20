//! glTF scene hierarchy spawning utilities

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::asset::{
    load_gltf_async, AssetServerResource, GltfSceneAssets, Handle, MeshCache, MeshFilter,
    MeshHandle,
};
use oxide_ecs::entity::Entity;
use oxide_ecs::world::World;
use oxide_ecs::{Component, Resource};
use oxide_renderer::gltf::{GltfNode, GltfScene};
use oxide_transform::{attach_child, GlobalTransform, TransformComponent};

use oxide_math::transform::Transform;
use wgpu::{Device, Queue};

/// Component storing the source glTF mesh index for an entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GltfMeshRef {
    pub mesh_index: usize,
}

/// Resource containing scene handles waiting to be spawned into ECS.
#[derive(Resource, Default)]
pub struct PendingGltfSceneSpawns {
    pub handles: Vec<Handle<GltfScene>>,
}

impl PendingGltfSceneSpawns {
    pub fn queue(&mut self, handle: Handle<GltfScene>) {
        if !self.handles.contains(&handle) {
            self.handles.push(handle);
        }
    }
}

/// Resource storing spawned root entities keyed by scene-handle ID.
#[derive(Resource, Default)]
pub struct SpawnedGltfScenes {
    pub roots_by_scene: HashMap<u64, Vec<Entity>>,
}

/// Resource storing glTF mesh handles keyed by scene-handle ID.
#[derive(Resource, Default)]
pub struct GltfSceneMeshHandles {
    pub handles_by_scene: HashMap<u64, Vec<MeshHandle>>,
}

/// Spawns glTF nodes into ECS while preserving the node hierarchy.
///
/// Returns the root entities created from the scene.
pub fn spawn_gltf_scene_hierarchy(world: &mut World, scene: &GltfScene) -> Vec<Entity> {
    spawn_gltf_scene_hierarchy_with_meshes(world, scene, None)
}

/// Spawns glTF nodes and attaches mesh handles when available.
pub fn spawn_gltf_scene_hierarchy_with_meshes(
    world: &mut World,
    scene: &GltfScene,
    mesh_handles: Option<&[MeshHandle]>,
) -> Vec<Entity> {
    scene
        .nodes
        .iter()
        .map(|node| spawn_gltf_node(world, node, None, mesh_handles))
        .collect()
}

fn spawn_gltf_node(
    world: &mut World,
    node: &GltfNode,
    parent: Option<Entity>,
    mesh_handles: Option<&[MeshHandle]>,
) -> Entity {
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
        if let Some(mesh_handle) = mesh_handles.and_then(|handles| handles.get(mesh_index)) {
            entity_builder.insert(MeshFilter::new(*mesh_handle));
        }
    }

    let entity = entity_builder.id();

    if let Some(parent_entity) = parent {
        attach_child(world, parent_entity, entity);
    }

    for child in &node.children {
        spawn_gltf_node(world, child, Some(entity), mesh_handles);
    }

    entity
}

/// Queues an already requested glTF scene handle for spawn-on-resolve.
pub fn queue_gltf_scene_spawn(world: &mut World, handle: Handle<GltfScene>) {
    if !world.contains_resource::<PendingGltfSceneSpawns>() {
        world.insert_resource(PendingGltfSceneSpawns::default());
    }
    world.resource_mut::<PendingGltfSceneSpawns>().queue(handle);
}

/// Starts async glTF loading and queues the scene for automatic spawn when ready.
pub fn request_gltf_scene_spawn(
    world: &mut World,
    device: Arc<Device>,
    queue: Arc<Queue>,
    path: impl Into<PathBuf>,
) -> Handle<GltfScene> {
    if !world.contains_resource::<AssetServerResource>() {
        world.insert_resource(AssetServerResource::default());
    }
    if !world.contains_resource::<PendingGltfSceneSpawns>() {
        world.insert_resource(PendingGltfSceneSpawns::default());
    }
    if !world.contains_resource::<GltfSceneAssets>() {
        world.insert_resource(GltfSceneAssets::default());
    }
    if !world.contains_resource::<GltfSceneMeshHandles>() {
        world.insert_resource(GltfSceneMeshHandles::default());
    }

    let handle = {
        let server = world.resource_mut::<AssetServerResource>();
        load_gltf_async(&mut server.server, device, queue, path)
    };
    world.resource_mut::<PendingGltfSceneSpawns>().queue(handle);
    handle
}

/// Returns and removes spawned roots for a resolved scene handle.
pub fn take_spawned_scene_roots(
    world: &mut World,
    handle: Handle<GltfScene>,
) -> Option<Vec<Entity>> {
    if !world.contains_resource::<SpawnedGltfScenes>() {
        return None;
    }
    world
        .resource_mut::<SpawnedGltfScenes>()
        .roots_by_scene
        .remove(&handle.id())
}

/// Polls async glTF loads and spawns queued scenes once available.
pub fn gltf_scene_spawn_system(world: &mut World) {
    if !world.contains_resource::<AssetServerResource>()
        || !world.contains_resource::<GltfSceneAssets>()
        || !world.contains_resource::<PendingGltfSceneSpawns>()
        || !world.contains_resource::<SpawnedGltfScenes>()
    {
        return;
    }
    if !world.contains_resource::<MeshCache>() {
        world.insert_resource(MeshCache::default());
    }
    if !world.contains_resource::<GltfSceneMeshHandles>() {
        world.insert_resource(GltfSceneMeshHandles::default());
    }

    let completed = {
        let server = world.resource_mut::<AssetServerResource>();
        server.server.poll_ready::<GltfScene>()
    };

    if !completed.is_empty() {
        let mut ready_handles = Vec::new();
        for result in completed {
            match result {
                Ok((handle, mut scene)) => {
                    let mesh_handles = register_gltf_scene_meshes(world, handle, &mut scene);
                    world
                        .resource_mut::<GltfSceneAssets>()
                        .assets
                        .insert(handle, scene);
                    if !mesh_handles.is_empty() {
                        world
                            .resource_mut::<GltfSceneMeshHandles>()
                            .handles_by_scene
                            .insert(handle.id(), mesh_handles);
                    }
                    ready_handles.push(handle);
                }
                Err(err) => tracing::warn!("Failed to load glTF scene: {err}"),
            }
        }

        if !ready_handles.is_empty() {
            let pending = world.resource_mut::<PendingGltfSceneSpawns>();
            for handle in ready_handles {
                pending.queue(handle);
            }
        }
    }

    let queued_handles = world.resource::<PendingGltfSceneSpawns>().handles.clone();
    let mut spawned = Vec::new();
    for handle in queued_handles {
        let scene = {
            let scene_assets = world.resource_mut::<GltfSceneAssets>();
            scene_assets.assets.remove(&handle)
        };

        if let Some(scene) = scene {
            let mesh_handles = world
                .resource::<GltfSceneMeshHandles>()
                .handles_by_scene
                .get(&handle.id())
                .cloned();
            let roots =
                spawn_gltf_scene_hierarchy_with_meshes(world, &scene, mesh_handles.as_deref());
            spawned.push((handle, roots));
        }
    }

    if spawned.is_empty() {
        return;
    }

    {
        let pending = world.resource_mut::<PendingGltfSceneSpawns>();
        pending
            .handles
            .retain(|handle| !spawned.iter().any(|(done, _)| done == handle));
    }

    {
        let results = world.resource_mut::<SpawnedGltfScenes>();
        for (handle, roots) in spawned {
            results.roots_by_scene.insert(handle.id(), roots);
        }
    }
}

fn register_gltf_scene_meshes(
    world: &mut World,
    handle: Handle<GltfScene>,
    scene: &mut GltfScene,
) -> Vec<MeshHandle> {
    if scene.meshes.is_empty() {
        return Vec::new();
    }

    let mut mesh_cache = world.remove_resource::<MeshCache>().unwrap_or_default();
    let mut mesh_handles = Vec::with_capacity(scene.meshes.len());
    {
        let server = world.resource_mut::<AssetServerResource>();
        let scene_source = server.server.asset_source(&handle);
        for (mesh_name, mesh) in std::mem::take(&mut scene.meshes) {
            let mesh_handle = if let Some(source) = &scene_source {
                server.server.insert_loaded_labeled_path(
                    mesh_cache.assets_mut(),
                    source.path().to_path_buf(),
                    Some(mesh_name.as_str()),
                    mesh,
                )
            } else {
                let mesh_handle = server.server.allocate_handle();
                mesh_cache.insert(mesh_handle, mesh);
                mesh_handle
            };
            mesh_handles.push(mesh_handle);
        }
    }
    world.insert_resource(mesh_cache);
    mesh_handles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::AssetServerResource;
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

    #[test]
    fn gltf_hierarchy_attaches_mesh_filter_handles() {
        let mut world = World::new();
        let mesh_handle = Handle::new(42);

        let scene = GltfScene {
            meshes: Vec::new(),
            nodes: vec![GltfNode {
                name: Some("mesh_node".to_string()),
                mesh_index: Some(0),
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
                children: Vec::new(),
            }],
        };

        let roots =
            spawn_gltf_scene_hierarchy_with_meshes(&mut world, &scene, Some(&[mesh_handle]));

        assert_eq!(roots.len(), 1);
        assert_eq!(
            world.get::<GltfMeshRef>(roots[0]),
            Some(&GltfMeshRef { mesh_index: 0 })
        );
        assert_eq!(
            world.get::<MeshFilter>(roots[0]).map(|filter| filter.mesh),
            Some(mesh_handle)
        );
    }

    #[test]
    fn queued_gltf_scene_spawns_when_asset_is_available() {
        let mut world = World::new();
        world.insert_resource(AssetServerResource::default());
        world.insert_resource(GltfSceneAssets::default());
        world.insert_resource(PendingGltfSceneSpawns::default());
        world.insert_resource(SpawnedGltfScenes::default());

        let handle = {
            let server = world.resource_mut::<AssetServerResource>();
            server.server.allocate_handle::<GltfScene>()
        };

        let scene = GltfScene {
            meshes: Vec::new(),
            nodes: vec![GltfNode {
                name: Some("root".to_string()),
                mesh_index: None,
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
                children: Vec::new(),
            }],
        };

        world
            .resource_mut::<GltfSceneAssets>()
            .assets
            .insert(handle, scene);
        queue_gltf_scene_spawn(&mut world, handle);
        gltf_scene_spawn_system(&mut world);

        let spawned_roots =
            take_spawned_scene_roots(&mut world, handle).expect("scene should have spawned");
        assert_eq!(spawned_roots.len(), 1);
        assert!(world.contains(spawned_roots[0]));
    }
}
