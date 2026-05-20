//! glTF scene hierarchy spawning utilities

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::asset::{
    load_gltf_async, AssetServerResource, GltfSceneAssets, Handle, MaterialDescriptorAssets,
    MaterialDescriptorHandle, MaterialFilter, MeshCache, MeshFilter, MeshHandle,
};
use crate::scene::SceneMaterialLibrary;
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

/// Component storing the source glTF material index for an entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GltfMaterialRef {
    pub material_index: usize,
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

/// Resource storing glTF material handles keyed by scene-handle ID.
#[derive(Resource, Default)]
pub struct GltfSceneMaterialHandles {
    pub handles_by_scene: HashMap<u64, Vec<MaterialDescriptorHandle>>,
}

/// Spawns glTF nodes into ECS while preserving the node hierarchy.
///
/// Returns the root entities created from the scene.
pub fn spawn_gltf_scene_hierarchy(world: &mut World, scene: &GltfScene) -> Vec<Entity> {
    spawn_gltf_scene_hierarchy_with_assets(world, scene, None, None)
}

/// Spawns glTF nodes and attaches mesh handles when available.
pub fn spawn_gltf_scene_hierarchy_with_meshes(
    world: &mut World,
    scene: &GltfScene,
    mesh_handles: Option<&[MeshHandle]>,
) -> Vec<Entity> {
    spawn_gltf_scene_hierarchy_with_assets(world, scene, mesh_handles, None)
}

/// Spawns glTF nodes and attaches imported mesh and material handles when available.
pub fn spawn_gltf_scene_hierarchy_with_assets(
    world: &mut World,
    scene: &GltfScene,
    mesh_handles: Option<&[MeshHandle]>,
    material_handles: Option<&[MaterialDescriptorHandle]>,
) -> Vec<Entity> {
    scene
        .nodes
        .iter()
        .map(|node| spawn_gltf_node(world, scene, node, None, mesh_handles, material_handles))
        .collect()
}

fn spawn_gltf_node(
    world: &mut World,
    scene: &GltfScene,
    node: &GltfNode,
    parent: Option<Entity>,
    mesh_handles: Option<&[MeshHandle]>,
    material_handles: Option<&[MaterialDescriptorHandle]>,
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
        if let Some(material_index) = scene
            .mesh_material_indices
            .get(mesh_index)
            .copied()
            .flatten()
        {
            entity_builder.insert(GltfMaterialRef { material_index });
            if let Some(material_handle) =
                material_handles.and_then(|handles| handles.get(material_index))
            {
                entity_builder.insert(MaterialFilter::new(*material_handle));
            }
        }
    }

    let entity = entity_builder.id();

    if let Some(parent_entity) = parent {
        attach_child(world, parent_entity, entity);
    }

    for child in &node.children {
        spawn_gltf_node(
            world,
            scene,
            child,
            Some(entity),
            mesh_handles,
            material_handles,
        );
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
    if !world.contains_resource::<GltfSceneMaterialHandles>() {
        world.insert_resource(GltfSceneMaterialHandles::default());
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
    if !world.contains_resource::<MaterialDescriptorAssets>() {
        world.insert_resource(MaterialDescriptorAssets::default());
    }
    if !world.contains_resource::<SceneMaterialLibrary>() {
        world.insert_resource(SceneMaterialLibrary::default());
    }
    if !world.contains_resource::<GltfSceneMeshHandles>() {
        world.insert_resource(GltfSceneMeshHandles::default());
    }
    if !world.contains_resource::<GltfSceneMaterialHandles>() {
        world.insert_resource(GltfSceneMaterialHandles::default());
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
                    let material_handles = register_gltf_scene_materials(world, handle, &mut scene);
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
                    if !material_handles.is_empty() {
                        world
                            .resource_mut::<GltfSceneMaterialHandles>()
                            .handles_by_scene
                            .insert(handle.id(), material_handles);
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
            let material_handles = world
                .resource::<GltfSceneMaterialHandles>()
                .handles_by_scene
                .get(&handle.id())
                .cloned();
            let roots = spawn_gltf_scene_hierarchy_with_assets(
                world,
                &scene,
                mesh_handles.as_deref(),
                material_handles.as_deref(),
            );
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

fn register_gltf_scene_materials(
    world: &mut World,
    handle: Handle<GltfScene>,
    scene: &mut GltfScene,
) -> Vec<MaterialDescriptorHandle> {
    if scene.materials.is_empty() {
        return Vec::new();
    }

    let mut material_assets = world
        .remove_resource::<MaterialDescriptorAssets>()
        .unwrap_or_default();
    let mut material_handles = Vec::with_capacity(scene.materials.len());
    let mut scene_materials = Vec::with_capacity(scene.materials.len());
    {
        let server = world.resource_mut::<AssetServerResource>();
        let scene_source = server.server.asset_source(&handle);
        for (material_name, descriptor) in std::mem::take(&mut scene.materials) {
            let material_handle = if let Some(source) = &scene_source {
                server.server.insert_loaded_labeled_path(
                    &mut material_assets.assets,
                    source.path().to_path_buf(),
                    Some(material_name.as_str()),
                    descriptor.clone(),
                )
            } else {
                let material_handle = server.server.allocate_handle();
                material_assets
                    .assets
                    .insert(material_handle, descriptor.clone());
                material_handle
            };
            scene_materials.push(descriptor);
            material_handles.push(material_handle);
        }
    }
    world.insert_resource(material_assets);

    let library = world.resource_mut::<SceneMaterialLibrary>();
    for descriptor in scene_materials {
        library.register_descriptor(&descriptor);
    }

    material_handles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::AssetServerResource;
    use glam::{Quat, Vec3};
    use oxide_renderer::descriptor::{MaterialDescriptor, MaterialType, ShaderDescriptor};
    use oxide_transform::{Children, Parent};

    fn test_material(name: &str, base_color: [f32; 4]) -> MaterialDescriptor {
        MaterialDescriptor {
            name: name.to_string(),
            material_type: MaterialType::Lit,
            shader: ShaderDescriptor::Builtin {
                shader: "lit".to_string(),
            },
            fallback_shader: Some("lit".to_string()),
            base_color,
            albedo_texture: None,
            normal_texture: None,
            roughness_texture: None,
        }
    }

    #[test]
    fn gltf_hierarchy_spawns_parent_child_relationships() {
        let mut world = World::new();

        let scene = GltfScene {
            meshes: Vec::new(),
            materials: Vec::new(),
            mesh_material_indices: Vec::new(),
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
            materials: Vec::new(),
            mesh_material_indices: Vec::new(),
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
    fn gltf_hierarchy_attaches_material_filter_handles() {
        let mut world = World::new();
        let material_handle = Handle::new(7);

        let scene = GltfScene {
            meshes: Vec::new(),
            materials: Vec::new(),
            mesh_material_indices: vec![Some(0)],
            nodes: vec![GltfNode {
                name: Some("material_node".to_string()),
                mesh_index: Some(0),
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
                children: Vec::new(),
            }],
        };

        let roots = spawn_gltf_scene_hierarchy_with_assets(
            &mut world,
            &scene,
            None,
            Some(&[material_handle]),
        );

        assert_eq!(roots.len(), 1);
        assert_eq!(
            world.get::<GltfMaterialRef>(roots[0]),
            Some(&GltfMaterialRef { material_index: 0 })
        );
        assert_eq!(
            world
                .get::<MaterialFilter>(roots[0])
                .map(|filter| filter.material),
            Some(material_handle)
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
            materials: Vec::new(),
            mesh_material_indices: Vec::new(),
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

    #[test]
    fn gltf_materials_register_as_labeled_descriptor_assets() {
        let mut world = World::new();
        world.insert_resource(AssetServerResource::default());
        world.insert_resource(MaterialDescriptorAssets::default());
        world.insert_resource(SceneMaterialLibrary::default());

        let handle = {
            let server = world.resource_mut::<AssetServerResource>();
            server
                .server
                .register_loaded_path::<GltfScene>("assets/models/level.gltf")
        };
        let mut scene = GltfScene {
            meshes: Vec::new(),
            materials: vec![(
                "material_0".to_string(),
                test_material("imported_red", [1.0, 0.0, 0.0, 1.0]),
            )],
            mesh_material_indices: vec![Some(0)],
            nodes: Vec::new(),
        };

        let handles = register_gltf_scene_materials(&mut world, handle, &mut scene);

        assert_eq!(handles.len(), 1);
        assert!(scene.materials.is_empty());
        assert_eq!(
            world
                .resource::<AssetServerResource>()
                .server
                .asset_label(&handles[0]),
            Some("material_0")
        );
        assert!(world
            .resource::<MaterialDescriptorAssets>()
            .assets
            .get(&handles[0])
            .is_some());
        assert!(world
            .resource::<SceneMaterialLibrary>()
            .contains("imported_red"));
    }
}
