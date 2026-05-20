//! Native Oxide scene asset loading and spawn utilities.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::asset::{AssetServerError, AssetServerResource, Assets, Handle};
use oxide_ecs::entity::Entity;
use oxide_ecs::world::World;
use oxide_ecs::Resource;
use oxide_scene::{load_scene_descriptor, spawn_scene_descriptor, SceneDescriptor};

#[derive(Resource, Default)]
pub struct SceneDescriptorAssets {
    pub assets: Assets<SceneDescriptor>,
}

#[derive(Resource, Default)]
pub struct PendingOxSceneSpawns {
    pub handles: Vec<Handle<SceneDescriptor>>,
}

impl PendingOxSceneSpawns {
    pub fn queue(&mut self, handle: Handle<SceneDescriptor>) {
        if !self.handles.contains(&handle) {
            self.handles.push(handle);
        }
    }
}

#[derive(Resource, Default)]
pub struct SpawnedOxScenes {
    pub roots_by_scene: HashMap<u64, Vec<Entity>>,
}

pub fn queue_oxscene_spawn(world: &mut World, handle: Handle<SceneDescriptor>) {
    ensure_oxscene_resources(world);
    world.resource_mut::<PendingOxSceneSpawns>().queue(handle);
}

pub fn request_oxscene_spawn(
    world: &mut World,
    path: impl Into<PathBuf>,
) -> Handle<SceneDescriptor> {
    ensure_oxscene_resources(world);

    let handle = {
        let server = world.resource_mut::<AssetServerResource>();
        server.server.load_path_async(path.into(), |path| {
            load_scene_descriptor(&path).map_err(|err| AssetServerError::Message(err.to_string()))
        })
    };
    world.resource_mut::<PendingOxSceneSpawns>().queue(handle);
    handle
}

pub fn take_spawned_oxscene_roots(
    world: &mut World,
    handle: Handle<SceneDescriptor>,
) -> Option<Vec<Entity>> {
    if !world.contains_resource::<SpawnedOxScenes>() {
        return None;
    }

    world
        .resource_mut::<SpawnedOxScenes>()
        .roots_by_scene
        .remove(&handle.id())
}

pub fn oxscene_spawn_system(world: &mut World) {
    if !world.contains_resource::<AssetServerResource>()
        || !world.contains_resource::<SceneDescriptorAssets>()
        || !world.contains_resource::<PendingOxSceneSpawns>()
        || !world.contains_resource::<SpawnedOxScenes>()
    {
        return;
    }

    let completed = {
        let server = world.resource_mut::<AssetServerResource>();
        server.server.poll_ready::<SceneDescriptor>()
    };

    if !completed.is_empty() {
        let mut ready_handles = Vec::new();
        {
            let scene_assets = world.resource_mut::<SceneDescriptorAssets>();
            for result in completed {
                match result {
                    Ok((handle, scene)) => {
                        scene_assets.assets.insert(handle, scene);
                        ready_handles.push(handle);
                    }
                    Err(err) => tracing::warn!("Failed to load Oxide scene: {err}"),
                }
            }
        }

        if !ready_handles.is_empty() {
            let pending = world.resource_mut::<PendingOxSceneSpawns>();
            for handle in ready_handles {
                pending.queue(handle);
            }
        }
    }

    let queued_handles = world.resource::<PendingOxSceneSpawns>().handles.clone();
    let mut spawned = Vec::new();
    for handle in queued_handles {
        let scene = {
            let scene_assets = world.resource::<SceneDescriptorAssets>();
            scene_assets.assets.get(&handle).cloned()
        };

        if let Some(scene) = scene {
            let roots = spawn_scene_descriptor(world, &scene);
            spawned.push((handle, roots));
        }
    }

    if spawned.is_empty() {
        return;
    }

    {
        let pending = world.resource_mut::<PendingOxSceneSpawns>();
        pending
            .handles
            .retain(|handle| !spawned.iter().any(|(done, _)| done == handle));
    }

    {
        let results = world.resource_mut::<SpawnedOxScenes>();
        for (handle, roots) in spawned {
            results.roots_by_scene.insert(handle.id(), roots);
        }
    }
}

pub fn ensure_oxscene_resources(world: &mut World) {
    if !world.contains_resource::<AssetServerResource>() {
        world.insert_resource(AssetServerResource::default());
    }
    if !world.contains_resource::<SceneDescriptorAssets>() {
        world.insert_resource(SceneDescriptorAssets::default());
    }
    if !world.contains_resource::<PendingOxSceneSpawns>() {
        world.insert_resource(PendingOxSceneSpawns::default());
    }
    if !world.contains_resource::<SpawnedOxScenes>() {
        world.insert_resource(SpawnedOxScenes::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_scene::{SceneEntityDescriptor, SceneEntityKind};

    #[test]
    fn queued_oxscene_spawns_when_asset_is_available() {
        let mut world = World::new();
        ensure_oxscene_resources(&mut world);

        let handle = {
            let server = world.resource_mut::<AssetServerResource>();
            server.server.allocate_handle::<SceneDescriptor>()
        };

        let scene = SceneDescriptor {
            entities: vec![SceneEntityDescriptor {
                name: Some("Asset Cube".to_string()),
                kind: SceneEntityKind::Mesh {
                    primitive: Default::default(),
                    material: Default::default(),
                },
                ..Default::default()
            }],
        };

        world
            .resource_mut::<SceneDescriptorAssets>()
            .assets
            .insert(handle, scene);
        queue_oxscene_spawn(&mut world, handle);
        oxscene_spawn_system(&mut world);

        let roots =
            take_spawned_oxscene_roots(&mut world, handle).expect("scene should have spawned");
        assert_eq!(roots.len(), 1);
        assert!(world.contains(roots[0]));
    }
}
