//! Compatibility facade for asset APIs plus engine-level typed asset resources.

use std::path::PathBuf;
#[cfg(feature = "gltf-import")]
use std::sync::Arc;

#[cfg(feature = "gltf-import")]
use oxide_asset::AssetServerError as CoreAssetServerError;
use oxide_asset::{AssetServer as CoreAssetServer, Assets as CoreAssets, Handle as CoreHandle};
use oxide_ecs::world::World;
use oxide_ecs::Resource;
use oxide_renderer::descriptor::{
    load_material_descriptor, MaterialDescriptor, MaterialDescriptorError, ShaderDescriptor,
};
#[cfg(feature = "gltf-import")]
use oxide_renderer::gltf::{load_gltf, GltfScene};
use oxide_renderer::material::MaterialPipeline;
use oxide_renderer::mesh::Mesh3D;
#[cfg(feature = "gltf-import")]
use wgpu::{Device, Queue};

use crate::scene::{reload_changed_oxscenes, SceneDescriptor, SceneMaterialLibrary};
use crate::watcher::AssetWatcher;

pub use oxide_asset::*;
pub use oxide_scene::{MaterialFilter, MeshCache, MeshFilter, TextureImageAssets};

/// ECS resource wrapper for the engine asset server.
#[derive(Resource, Default)]
pub struct AssetServerResource {
    pub server: CoreAssetServer,
}

/// ECS resource storing handle-indexed material pipelines.
#[derive(Resource, Default)]
pub struct MaterialAssets {
    pub assets: CoreAssets<MaterialPipeline>,
}

/// ECS resource storing CPU-side material descriptors loaded from `.oxmat`,
/// JSON, RON, or TOML files.
#[derive(Resource, Default)]
pub struct MaterialDescriptorAssets {
    pub assets: CoreAssets<MaterialDescriptor>,
}

pub type MeshHandle = CoreHandle<Mesh3D>;
pub type MaterialHandle = CoreHandle<MaterialPipeline>;
pub type MaterialDescriptorHandle = CoreHandle<MaterialDescriptor>;
pub type TextureImageHandle = oxide_scene::TextureImageHandle;

/// ECS resource storing handle-indexed glTF scenes.
#[cfg(feature = "gltf-import")]
#[derive(Resource, Default)]
pub struct GltfSceneAssets {
    pub assets: CoreAssets<GltfScene>,
}

/// Result of routing changed source paths through Oxide's native reload systems.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeAssetReloadSummary {
    /// Changed paths reported by a watcher or caller.
    pub changed_paths: Vec<PathBuf>,
    /// Native scene descriptors that started reloading.
    pub oxscenes: Vec<CoreHandle<SceneDescriptor>>,
    /// Material descriptors that started reloading.
    pub material_descriptors: Vec<MaterialDescriptorHandle>,
}

impl NativeAssetReloadSummary {
    /// Returns true when no changed paths produced native asset reloads.
    pub fn is_empty(&self) -> bool {
        self.oxscenes.is_empty() && self.material_descriptors.is_empty()
    }
}

/// Registers a material pipeline under a stable handle.
pub fn register_material_asset(
    server: &mut CoreAssetServer,
    assets: &mut CoreAssets<MaterialPipeline>,
    material: MaterialPipeline,
) -> CoreHandle<MaterialPipeline> {
    let handle = server.allocate_handle::<MaterialPipeline>();
    assets.insert(handle, material);
    handle
}

/// Requests an async material descriptor load and returns a stable typed handle.
pub fn request_material_descriptor_load(
    server: &mut CoreAssetServer,
    path: impl Into<PathBuf>,
) -> MaterialDescriptorHandle {
    server.load_path_async(path.into(), |path| {
        load_material_descriptor(&path).map_err(material_descriptor_asset_error)
    })
}

/// Starts an in-place reload for a previously loaded material descriptor path.
pub fn reload_material_descriptor_path(
    server: &mut CoreAssetServer,
    path: impl Into<PathBuf>,
) -> Option<MaterialDescriptorHandle> {
    server.reload_path_async(path.into(), |path| {
        load_material_descriptor(&path).map_err(material_descriptor_asset_error)
    })
}

/// Reloads material descriptors affected by changed source or dependency paths.
pub fn reload_changed_material_descriptors<I, P>(
    server: &mut CoreAssetServer,
    changed_paths: I,
) -> Vec<MaterialDescriptorHandle>
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    let mut reload_paths = Vec::new();
    for changed_path in changed_paths {
        for handle in server.handles_for_changed_path::<MaterialDescriptor>(changed_path.into()) {
            if let Some(path) = server.asset_path(&handle) {
                let path = path.to_path_buf();
                if !reload_paths.contains(&path) {
                    reload_paths.push(path);
                }
            }
        }
    }

    let mut reloaded = Vec::new();
    for path in reload_paths {
        if let Some(handle) = reload_material_descriptor_path(server, path) {
            if !reloaded.contains(&handle) {
                reloaded.push(handle);
            }
        }
    }
    reloaded
}

/// Reloads native Oxide assets affected by changed source paths.
///
/// This fans out one changed-path list to the built-in `.oxscene` and `.oxmat`
/// reload systems. Reload completion is still published by the normal
/// `oxscene_spawn_system` and `material_descriptor_asset_system` systems.
pub fn reload_changed_native_assets<I, P>(
    world: &mut World,
    changed_paths: I,
) -> NativeAssetReloadSummary
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    let changed_paths: Vec<PathBuf> = changed_paths.into_iter().map(Into::into).collect();
    let oxscenes = reload_changed_oxscenes(world, changed_paths.iter().cloned());
    let material_descriptors = if world.contains_resource::<AssetServerResource>() {
        let server = world.resource_mut::<AssetServerResource>();
        reload_changed_material_descriptors(&mut server.server, changed_paths.iter().cloned())
    } else {
        Vec::new()
    };

    NativeAssetReloadSummary {
        changed_paths,
        oxscenes,
        material_descriptors,
    }
}

/// Polls the installed [`AssetWatcher`] and reloads affected native Oxide assets.
///
/// Returns an empty summary when no watcher is installed or no relevant assets
/// are affected.
pub fn poll_native_asset_reloads(world: &mut World) -> NativeAssetReloadSummary {
    let Some(watcher) = world.get_non_send_resource_mut::<AssetWatcher>() else {
        return NativeAssetReloadSummary::default();
    };
    let changed_paths = watcher.poll_changed_files().to_vec();
    reload_changed_native_assets(world, changed_paths)
}

/// Publishes completed material descriptor loads and records source dependencies.
pub fn poll_material_descriptor_assets(
    server: &mut CoreAssetServer,
    assets: &mut CoreAssets<MaterialDescriptor>,
) -> Vec<Result<MaterialDescriptorHandle, oxide_asset::AssetServerError>> {
    let mut completed_handles = Vec::new();
    for result in server.poll_ready::<MaterialDescriptor>() {
        match result {
            Ok((handle, descriptor)) => {
                let dependencies = server
                    .asset_path(&handle)
                    .map(|path| material_descriptor_dependencies(path, &descriptor))
                    .unwrap_or_default();
                let _ = server.set_asset_dependencies(&handle, dependencies);
                assets.insert(handle, descriptor);
                completed_handles.push(Ok(handle));
            }
            Err(err) => completed_handles.push(Err(err)),
        }
    }
    completed_handles
}

/// ECS system that publishes ready material descriptor assets.
pub fn material_descriptor_asset_system(world: &mut World) {
    if !world.contains_resource::<AssetServerResource>()
        || !world.contains_resource::<MaterialDescriptorAssets>()
    {
        return;
    }

    let completed = {
        let server = world.resource_mut::<AssetServerResource>();
        let mut completed = Vec::new();
        for result in server.server.poll_ready::<MaterialDescriptor>() {
            match result {
                Ok((handle, descriptor)) => {
                    let dependencies = server
                        .server
                        .asset_path(&handle)
                        .map(|path| material_descriptor_dependencies(path, &descriptor))
                        .unwrap_or_default();
                    let _ = server.server.set_asset_dependencies(&handle, dependencies);
                    completed.push(Ok((handle, descriptor)));
                }
                Err(err) => completed.push(Err(err)),
            }
        }
        completed
    };

    if completed.is_empty() {
        return;
    }

    let mut scene_materials = Vec::new();
    let assets = world.resource_mut::<MaterialDescriptorAssets>();
    for result in completed {
        match result {
            Ok((handle, descriptor)) => {
                let scene_material = descriptor.clone();
                assets.assets.insert(handle, descriptor);
                scene_materials.push(scene_material);
            }
            Err(err) => tracing::warn!("Failed to load material descriptor: {err}"),
        }
    }
    if scene_materials.is_empty() {
        return;
    }
    if !world.contains_resource::<SceneMaterialLibrary>() {
        world.insert_resource(SceneMaterialLibrary::default());
    }
    let library = world.resource_mut::<SceneMaterialLibrary>();
    for descriptor in scene_materials {
        library.register_descriptor(&descriptor);
    }
}

/// Returns shader and texture paths that should invalidate a material descriptor.
pub fn material_descriptor_dependencies(
    descriptor_path: impl Into<PathBuf>,
    descriptor: &MaterialDescriptor,
) -> Vec<PathBuf> {
    let descriptor_path = descriptor_path.into();
    let base = descriptor_path.parent().map(PathBuf::from);
    let mut dependencies = Vec::new();

    if let ShaderDescriptor::File { path } = &descriptor.shader {
        dependencies.push(resolve_descriptor_dependency(base.as_ref(), path));
    }
    if let Some(path) = descriptor
        .albedo_texture
        .as_ref()
        .filter(|path| !is_virtual_texture_ref(path))
    {
        dependencies.push(resolve_descriptor_dependency(base.as_ref(), path));
    }
    if let Some(path) = descriptor
        .normal_texture
        .as_ref()
        .filter(|path| !is_virtual_texture_ref(path))
    {
        dependencies.push(resolve_descriptor_dependency(base.as_ref(), path));
    }
    if let Some(path) = descriptor
        .roughness_texture
        .as_ref()
        .filter(|path| !is_virtual_texture_ref(path))
    {
        dependencies.push(resolve_descriptor_dependency(base.as_ref(), path));
    }

    dependencies.sort();
    dependencies.dedup();
    dependencies
}

fn is_virtual_texture_ref(path: &str) -> bool {
    path.trim_start().starts_with('#')
}

fn resolve_descriptor_dependency(base: Option<&PathBuf>, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else if let Some(base) = base {
        base.join(path)
    } else {
        path
    }
}

fn material_descriptor_asset_error(
    error: MaterialDescriptorError,
) -> oxide_asset::AssetServerError {
    oxide_asset::AssetServerError::Message(error.to_string())
}

/// Starts async glTF loading and returns a scene handle.
///
/// The engine's `gltf_scene_spawn_system` consumes readiness via `AssetServer::poll_ready`.
#[cfg(feature = "gltf-import")]
pub fn load_gltf_async(
    server: &mut CoreAssetServer,
    device: Arc<Device>,
    queue: Arc<Queue>,
    path: impl Into<PathBuf>,
) -> CoreHandle<GltfScene> {
    server.load_path_async(path.into(), move |path| {
        load_gltf(&device, &queue, &path)
            .map_err(|err| CoreAssetServerError::Message(err.to_string()))
    })
}

/// Starts async glTF reloading into an existing typed scene handle.
#[cfg(feature = "gltf-import")]
pub fn reload_gltf_async(
    server: &mut CoreAssetServer,
    device: Arc<Device>,
    queue: Arc<Queue>,
    path: impl Into<PathBuf>,
) -> Option<CoreHandle<GltfScene>> {
    server.reload_path_async(path.into(), move |path| {
        load_gltf(&device, &queue, &path)
            .map_err(|err| CoreAssetServerError::Message(err.to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{
        oxscene_spawn_system, request_oxscene_spawn, take_spawned_oxscene_roots,
        SceneDescriptorAssets,
    };
    use oxide_asset::AssetChangeKind;
    use std::fs;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn material_descriptor_dependencies_resolve_relative_to_descriptor() {
        let descriptor = MaterialDescriptor {
            name: "Stone".to_string(),
            material_type: oxide_renderer::descriptor::MaterialType::Lit,
            shader: ShaderDescriptor::File {
                path: "shaders/stone.wgsl".to_string(),
            },
            fallback_shader: Some("lit".to_string()),
            base_color: [1.0, 1.0, 1.0, 1.0],
            albedo_texture: Some("textures/stone.png".to_string()),
            normal_texture: Some("textures/stone_n.png".to_string()),
            roughness_texture: Some("textures/stone_r.png".to_string()),
        };

        let dependencies =
            material_descriptor_dependencies("assets/materials/stone.oxmat", &descriptor);

        assert_eq!(
            dependencies,
            vec![
                PathBuf::from("assets/materials/shaders/stone.wgsl"),
                PathBuf::from("assets/materials/textures/stone.png"),
                PathBuf::from("assets/materials/textures/stone_n.png"),
                PathBuf::from("assets/materials/textures/stone_r.png"),
            ]
        );
    }

    #[test]
    fn material_descriptor_dependencies_ignore_virtual_texture_refs() {
        let descriptor = MaterialDescriptor {
            name: "Imported".to_string(),
            material_type: oxide_renderer::descriptor::MaterialType::Lit,
            shader: ShaderDescriptor::Builtin {
                shader: "lit".to_string(),
            },
            fallback_shader: Some("lit".to_string()),
            base_color: [1.0, 1.0, 1.0, 1.0],
            albedo_texture: Some("#image_0".to_string()),
            normal_texture: None,
            roughness_texture: None,
        };

        assert!(material_descriptor_dependencies("assets/model.gltf", &descriptor).is_empty());
    }

    #[test]
    fn material_descriptor_assets_load_and_reload_from_dependency_changes() {
        let root = temp_dir("oxide_material_asset");
        let material_path = root.join("stone.oxmat");
        let shader_path = root.join("stone.wgsl");
        fs::write(&shader_path, "// shader").unwrap();
        write_material(&material_path, "Stone", "stone.wgsl");

        let mut server = CoreAssetServer::new();
        let mut assets = CoreAssets::<MaterialDescriptor>::new();
        let handle = request_material_descriptor_load(&mut server, &material_path);
        poll_until_material_named(&mut server, &mut assets, handle, "Stone");
        assert_eq!(assets.revision(&handle), Some(1));
        assert_eq!(assets.changes().len(), 1);
        assert_eq!(assets.changes()[0].kind, AssetChangeKind::Added);
        assets.clear_changes();

        let canonical_shader_path = std::fs::canonicalize(&shader_path).unwrap_or(shader_path);
        assert_eq!(
            server.asset_dependencies(&handle).unwrap(),
            std::slice::from_ref(&canonical_shader_path)
        );

        write_material(&material_path, "Reloaded Stone", "stone.wgsl");
        let reloaded =
            reload_changed_material_descriptors(&mut server, [canonical_shader_path.clone()]);
        assert_eq!(reloaded, vec![handle]);

        poll_until_material_named(&mut server, &mut assets, handle, "Reloaded Stone");
        assert_eq!(assets.revision(&handle), Some(2));
        assert_eq!(assets.changes().len(), 1);
        assert_eq!(assets.changes()[0].kind, AssetChangeKind::Modified);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn material_descriptor_asset_system_updates_scene_material_library() {
        let root = temp_dir("oxide_scene_material_asset");
        let material_path = root.join("bronze.oxmat");
        let shader_path = root.join("bronze.wgsl");
        fs::write(&shader_path, "// shader").unwrap();
        write_material(&material_path, "Bronze", "bronze.wgsl");

        let mut world = World::new();
        world.insert_resource(AssetServerResource::default());
        world.insert_resource(MaterialDescriptorAssets::default());
        {
            let server = world.resource_mut::<AssetServerResource>();
            request_material_descriptor_load(&mut server.server, &material_path);
        }

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            material_descriptor_asset_system(&mut world);
            if world
                .get_resource::<SceneMaterialLibrary>()
                .and_then(|library| library.get("Bronze"))
                .map(|material| material.base_color_with_library(None) == [0.45, 0.3, 0.18, 1.0])
                .unwrap_or(false)
            {
                let _ = fs::remove_dir_all(root);
                return;
            }
            std::thread::yield_now();
        }

        panic!("scene material library was not updated from material descriptor asset");
    }

    #[test]
    fn native_asset_reload_summary_fans_out_changed_paths() {
        let root = temp_dir("oxide_native_asset_reload");
        let material_path = root.join("stone.oxmat");
        let shader_path = root.join("stone.wgsl");
        let scene_path = root.join("level.oxscene");
        let scene_dependency_path = root.join("level.sidecar");
        fs::write(&shader_path, "// shader").unwrap();
        fs::write(&scene_dependency_path, "{}").unwrap();
        write_material(&material_path, "Stone", "stone.wgsl");
        write_scene_with_dependencies(&scene_path, "Original Level", ["level.sidecar"]);

        let mut world = World::new();
        world.insert_resource(AssetServerResource::default());
        world.insert_resource(MaterialDescriptorAssets::default());
        let material_handle = {
            let server = world.resource_mut::<AssetServerResource>();
            request_material_descriptor_load(&mut server.server, &material_path)
        };
        let scene_handle = request_oxscene_spawn(&mut world, &scene_path);

        run_until_native_assets_named(
            &mut world,
            material_handle,
            "Stone",
            scene_handle,
            "Original Level",
        );
        let _ = take_spawned_oxscene_roots(&mut world, scene_handle);

        write_material(&material_path, "Reloaded Stone", "stone.wgsl");
        write_scene_with_dependencies(&scene_path, "Reloaded Level", ["level.sidecar"]);
        let summary = reload_changed_native_assets(
            &mut world,
            [shader_path.clone(), scene_dependency_path.clone()],
        );

        assert_eq!(
            summary.changed_paths,
            vec![shader_path, scene_dependency_path]
        );
        assert_eq!(summary.material_descriptors, vec![material_handle]);
        assert_eq!(summary.oxscenes, vec![scene_handle]);
        assert!(!summary.is_empty());

        run_until_native_assets_named(
            &mut world,
            material_handle,
            "Reloaded Stone",
            scene_handle,
            "Reloaded Level",
        );
        assert!(take_spawned_oxscene_roots(&mut world, scene_handle).is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn poll_native_asset_reloads_is_empty_without_watcher() {
        let mut world = World::new();
        assert!(poll_native_asset_reloads(&mut world).is_empty());
    }

    fn poll_until_material_named(
        server: &mut CoreAssetServer,
        assets: &mut CoreAssets<MaterialDescriptor>,
        handle: MaterialDescriptorHandle,
        expected_name: &str,
    ) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let _ = poll_material_descriptor_assets(server, assets);
            if assets.get(&handle).map(|material| material.name.as_str()) == Some(expected_name) {
                return;
            }
            std::thread::yield_now();
        }

        panic!("material descriptor was not loaded with expected name '{expected_name}'");
    }

    fn run_until_native_assets_named(
        world: &mut World,
        material_handle: MaterialDescriptorHandle,
        expected_material_name: &str,
        scene_handle: CoreHandle<SceneDescriptor>,
        expected_scene_name: &str,
    ) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            material_descriptor_asset_system(world);
            oxscene_spawn_system(world);
            let material_loaded = world
                .resource::<MaterialDescriptorAssets>()
                .assets
                .get(&material_handle)
                .map(|material| material.name.as_str())
                == Some(expected_material_name);
            let scene_loaded = world
                .resource::<SceneDescriptorAssets>()
                .assets
                .get(&scene_handle)
                .and_then(|scene| scene.entities.first())
                .and_then(|entity| entity.name.as_deref())
                == Some(expected_scene_name);
            if material_loaded && scene_loaded {
                return;
            }
            std::thread::yield_now();
        }

        panic!(
            "native assets did not load material '{expected_material_name}' and scene '{expected_scene_name}'"
        );
    }

    fn write_material(path: &std::path::Path, name: &str, shader: &str) {
        fs::write(
            path,
            format!(
                r#"{{
                    "format": "oxide.oxmat",
                    "version": 1,
                    "material": {{
                        "name": "{name}",
                        "material_type": "lit",
                        "base_color": [0.45, 0.3, 0.18, 1.0],
                        "shader": {{ "source": "file", "path": "{shader}" }},
                        "fallback_shader": "lit"
                    }}
                }}"#
            ),
        )
        .unwrap();
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

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{name}_{stamp}"));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
