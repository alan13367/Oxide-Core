//! Native Oxide scene asset loading and spawn utilities.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

/// Starts an in-place reload for a known native scene path.
///
/// The existing scene handle is preserved. The replacement descriptor is
/// published by [`oxscene_spawn_system`] into [`SceneDescriptorAssets`] once the
/// async load completes. Reloading does not automatically spawn duplicate scene
/// roots; queue the returned handle explicitly if the app wants to instantiate
/// the reloaded descriptor again.
pub fn reload_oxscene_path(
    world: &mut World,
    path: impl Into<PathBuf>,
) -> Option<Handle<SceneDescriptor>> {
    ensure_oxscene_resources(world);
    let server = world.resource_mut::<AssetServerResource>();
    server.server.reload_path_async(path.into(), |path| {
        load_scene_descriptor(&path).map_err(|err| AssetServerError::Message(err.to_string()))
    })
}

/// Reloads loaded native scene assets affected by changed source paths.
///
/// A path can match either the scene's own source path or a dependency path
/// registered on the handle through `AssetServer`.
pub fn reload_changed_oxscenes<I, P>(
    world: &mut World,
    changed_paths: I,
) -> Vec<Handle<SceneDescriptor>>
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    ensure_oxscene_resources(world);
    let mut reload_paths = Vec::new();
    {
        let server = world.resource::<AssetServerResource>();
        for changed_path in changed_paths {
            for handle in server
                .server
                .handles_for_changed_path::<SceneDescriptor>(changed_path.into())
            {
                if let Some(path) = server.server.asset_path(&handle) {
                    let path = path.to_path_buf();
                    if !reload_paths.contains(&path) {
                        reload_paths.push(path);
                    }
                }
            }
        }
    }

    let mut reloaded = Vec::new();
    for path in reload_paths {
        if let Some(handle) = reload_oxscene_path(world, path) {
            if !reloaded.contains(&handle) {
                reloaded.push(handle);
            }
        }
    }
    reloaded
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
        let mut scenes = Vec::new();
        for result in completed {
            match result {
                Ok((handle, scene)) => {
                    record_scene_dependencies(world, &handle, &scene);
                    scenes.push((handle, scene));
                }
                Err(err) => tracing::warn!("Failed to load Oxide scene: {err}"),
            }
        }
        let scene_assets = world.resource_mut::<SceneDescriptorAssets>();
        for (handle, scene) in scenes {
            scene_assets.assets.insert(handle, scene);
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

fn record_scene_dependencies(
    world: &mut World,
    handle: &Handle<SceneDescriptor>,
    scene: &SceneDescriptor,
) {
    let dependencies = {
        let server = world.resource::<AssetServerResource>();
        server
            .server
            .asset_path(handle)
            .map(|path| scene_descriptor_dependencies(path, scene))
            .unwrap_or_default()
    };
    let server = world.resource_mut::<AssetServerResource>();
    let _ = server.server.set_asset_dependencies(handle, dependencies);
}

/// Returns dependency paths that should invalidate a native scene descriptor.
pub fn scene_descriptor_dependencies(
    descriptor_path: impl AsRef<Path>,
    scene: &SceneDescriptor,
) -> Vec<PathBuf> {
    let base = descriptor_path.as_ref().parent().map(PathBuf::from);
    let mut dependencies: Vec<_> = scene
        .dependencies
        .iter()
        .filter_map(|dependency| {
            let dependency = dependency.trim();
            (!dependency.is_empty()).then(|| resolve_scene_dependency(base.as_ref(), dependency))
        })
        .collect();
    dependencies.sort();
    dependencies.dedup();
    dependencies
}

fn resolve_scene_dependency(base: Option<&PathBuf>, dependency: &str) -> PathBuf {
    let path = PathBuf::from(dependency);
    if path.is_absolute() {
        path
    } else if let Some(base) = base {
        base.join(path)
    } else {
        path
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
    use std::fs;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
            dependencies: Vec::new(),
            materials: Vec::new(),
            prefabs: Vec::new(),
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

    #[test]
    fn reload_oxscene_path_updates_asset_without_respawning_roots() {
        let path = temp_path("reload_scene", "oxscene");
        write_scene(&path, "Original Cube");

        let mut world = World::new();
        let handle = request_oxscene_spawn(&mut world, &path);
        run_until_scene_asset_named(&mut world, handle, "Original Cube");
        let roots =
            take_spawned_oxscene_roots(&mut world, handle).expect("initial scene should spawn");
        assert_eq!(roots.len(), 1);

        write_scene(&path, "Reloaded Cube");
        let reloaded = reload_oxscene_path(&mut world, &path).expect("known path should reload");
        assert_eq!(reloaded, handle);

        run_until_scene_asset_named(&mut world, handle, "Reloaded Cube");
        assert!(take_spawned_oxscene_roots(&mut world, handle).is_none());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn reload_changed_oxscenes_matches_dependency_paths() {
        let scene_path = temp_path("dependency_scene", "oxscene");
        let dependency_path = temp_path("scene_material", "oxmat");
        write_scene(&scene_path, "Dependency Cube");
        fs::write(&dependency_path, "{}").unwrap();

        let mut world = World::new();
        let handle = request_oxscene_spawn(&mut world, &scene_path);
        run_until_scene_asset_named(&mut world, handle, "Dependency Cube");
        let _ = take_spawned_oxscene_roots(&mut world, handle);

        {
            let server = world.resource_mut::<AssetServerResource>();
            assert!(server
                .server
                .add_asset_dependency(&handle, dependency_path.clone()));
        }

        write_scene(&scene_path, "Dependency Reloaded Cube");
        let reloaded = reload_changed_oxscenes(&mut world, [dependency_path.clone()]);
        assert_eq!(reloaded, vec![handle]);

        run_until_scene_asset_named(&mut world, handle, "Dependency Reloaded Cube");
        assert!(take_spawned_oxscene_roots(&mut world, handle).is_none());

        let _ = fs::remove_file(scene_path);
        let _ = fs::remove_file(dependency_path);
    }

    #[test]
    fn loaded_oxscene_records_declared_dependency_paths() {
        let scene_path = temp_path("declared_dependency_scene", "oxscene");
        let dependency_path = scene_path
            .parent()
            .unwrap()
            .join("declared_scene_material.oxmat");
        write_scene_with_dependencies(
            &scene_path,
            "Declared Dependency Cube",
            ["declared_scene_material.oxmat"],
        );
        fs::write(&dependency_path, "{}").unwrap();

        let mut world = World::new();
        let handle = request_oxscene_spawn(&mut world, &scene_path);
        run_until_scene_asset_named(&mut world, handle, "Declared Dependency Cube");

        let server = world.resource::<AssetServerResource>();
        let expected_dependency = server
            .server
            .asset_path(&handle)
            .unwrap()
            .parent()
            .unwrap()
            .join("declared_scene_material.oxmat");
        let dependencies = server.server.asset_dependencies(&handle).unwrap();
        assert_eq!(dependencies, &[expected_dependency]);

        let _ = fs::remove_file(scene_path);
        let _ = fs::remove_file(dependency_path);
    }

    #[test]
    fn reload_changed_oxscenes_matches_declared_dependency_paths() {
        let scene_path = temp_path("declared_reload_scene", "oxscene");
        let dependency_path = scene_path.parent().unwrap().join("declared_reload.oxmat");
        write_scene_with_dependencies(
            &scene_path,
            "Declared Original Cube",
            ["declared_reload.oxmat"],
        );
        fs::write(&dependency_path, "{}").unwrap();

        let mut world = World::new();
        let handle = request_oxscene_spawn(&mut world, &scene_path);
        run_until_scene_asset_named(&mut world, handle, "Declared Original Cube");
        let _ = take_spawned_oxscene_roots(&mut world, handle);

        write_scene_with_dependencies(
            &scene_path,
            "Declared Reloaded Cube",
            ["declared_reload.oxmat"],
        );
        let reloaded = reload_changed_oxscenes(&mut world, [dependency_path.clone()]);
        assert_eq!(reloaded, vec![handle]);

        run_until_scene_asset_named(&mut world, handle, "Declared Reloaded Cube");
        assert!(take_spawned_oxscene_roots(&mut world, handle).is_none());

        let _ = fs::remove_file(scene_path);
        let _ = fs::remove_file(dependency_path);
    }

    #[test]
    fn scene_descriptor_dependencies_resolve_relative_to_scene() {
        let scene = SceneDescriptor {
            dependencies: vec![
                "materials/stone.oxmat".to_string(),
                "materials/stone.oxmat".to_string(),
                "sprites/hud.png".to_string(),
            ],
            ..Default::default()
        };

        let dependencies = scene_descriptor_dependencies("assets/scenes/level.oxscene", &scene);
        assert_eq!(
            dependencies,
            vec![
                PathBuf::from("assets/scenes/materials/stone.oxmat"),
                PathBuf::from("assets/scenes/sprites/hud.png"),
            ]
        );
    }

    fn run_until_scene_asset_named(
        world: &mut World,
        handle: Handle<SceneDescriptor>,
        expected_name: &str,
    ) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            oxscene_spawn_system(world);
            let is_loaded = world
                .resource::<SceneDescriptorAssets>()
                .assets
                .get(&handle)
                .and_then(|scene| scene.entities.first())
                .and_then(|entity| entity.name.as_deref())
                == Some(expected_name);
            if is_loaded {
                return;
            }
            std::thread::yield_now();
        }

        panic!("scene asset was not loaded with expected name '{expected_name}'");
    }

    fn write_scene(path: &std::path::Path, name: &str) {
        write_scene_with_dependencies(path, name, std::iter::empty::<&str>());
    }

    fn write_scene_with_dependencies<I, S>(path: &std::path::Path, name: &str, dependencies: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let dependencies = dependencies
            .into_iter()
            .map(|dependency| format!(r#""{}""#, dependency.as_ref()))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            path,
            format!(
                r#"{{
                    "format": "oxide.oxscene",
                    "version": 1,
                    "scene": {{
                        "dependencies": [{dependencies}],
                        "entities": [
                            {{
                                "name": "{name}",
                                "type": "mesh",
                                "primitive": "cube"
                            }}
                        ]
                    }}
                }}"#
            ),
        )
        .unwrap();
    }

    fn temp_path(name: &str, extension: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{name}_{stamp}.{extension}"))
    }
}
