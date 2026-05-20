//! Compatibility facade for asset APIs plus engine-level typed asset resources.

use std::path::PathBuf;
#[cfg(feature = "gltf-import")]
use std::sync::Arc;

use oxide_asset::AssetServerError as CoreAssetServerError;
use oxide_asset::{
    AssetChange as CoreAssetChange, AssetChangeCursor as CoreAssetChangeCursor,
    AssetServer as CoreAssetServer, Assets as CoreAssets, Handle as CoreHandle,
};
use oxide_ecs::prelude::{EventWriter, Local, Res};
use oxide_ecs::world::World;
use oxide_ecs::Resource;
use oxide_renderer::descriptor::{
    load_material_descriptor, MaterialDescriptorError, ShaderDescriptor,
};
#[cfg(feature = "gltf-import")]
use oxide_renderer::gltf::{load_gltf, GltfScene};
use oxide_renderer::material::MaterialPipeline;
use oxide_renderer::mesh::Mesh3D;
use oxide_renderer::texture::TextureImage;
#[cfg(feature = "gltf-import")]
use wgpu::{Device, Queue};

#[cfg(feature = "gltf-import")]
use crate::ecs::RendererResource;
#[cfg(feature = "gltf-import")]
use crate::scene::reload_changed_gltf_scenes;
use crate::scene::{reload_changed_oxscenes, SceneDescriptor, SceneMaterialLibrary};
use crate::watcher::AssetWatcher;

pub use oxide_asset::*;
pub use oxide_renderer::descriptor::{AlphaMode, MaterialDescriptor};
pub use oxide_scene::{
    MaterialDescriptorAssets, MaterialFilter, MeshCache, MeshFilter, TextureImageAssets,
};

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

/// Typed resource adapter for publishing asset-store changes into ECS events.
///
/// Implement this for Oxide-owned asset resources that wrap [`Assets<T>`]. It
/// lets generic systems bridge retained asset change logs into
/// `Events<AssetChange<T>>` without draining the underlying log.
pub trait AssetStore<T>: oxide_ecs::prelude::Resource {
    /// Returns the typed asset storage backing this resource.
    fn assets(&self) -> &CoreAssets<T>;
}

impl AssetStore<MaterialPipeline> for MaterialAssets {
    fn assets(&self) -> &CoreAssets<MaterialPipeline> {
        &self.assets
    }
}

impl AssetStore<Mesh3D> for MeshCache {
    fn assets(&self) -> &CoreAssets<Mesh3D> {
        self.assets()
    }
}

impl AssetStore<MaterialDescriptor> for MaterialDescriptorAssets {
    fn assets(&self) -> &CoreAssets<MaterialDescriptor> {
        &self.assets
    }
}

impl AssetStore<TextureImage> for TextureImageAssets {
    fn assets(&self) -> &CoreAssets<TextureImage> {
        &self.assets
    }
}

/// Publishes unread asset changes from an [`AssetStore`] into ECS events.
///
/// Register this with concrete type parameters, for example:
/// `publish_asset_change_events::<MaterialDescriptor, MaterialDescriptorAssets>`.
/// Each registered system owns an independent local cursor, so multiple systems
/// can publish or mirror the same asset store without consuming the store log.
/// The matching `Events<AssetChange<T>>` resource must exist before the system
/// runs.
pub fn publish_asset_change_events<T, S>(
    assets: Res<S>,
    mut cursor: Local<CoreAssetChangeCursor<T>>,
    mut events: EventWriter<CoreAssetChange<T>>,
) where
    T: 'static,
    S: AssetStore<T> + 'static,
{
    events.extend(cursor.read(assets.assets()));
}

pub type MeshHandle = CoreHandle<Mesh3D>;
pub type MaterialHandle = CoreHandle<MaterialPipeline>;
pub type MaterialDescriptorHandle = oxide_scene::MaterialDescriptorHandle;
pub type TextureImageHandle = oxide_scene::TextureImageHandle;

/// ECS resource storing handle-indexed glTF scenes.
#[cfg(feature = "gltf-import")]
#[derive(Resource, Default)]
pub struct GltfSceneAssets {
    pub assets: CoreAssets<GltfScene>,
}

#[cfg(feature = "gltf-import")]
impl AssetStore<GltfScene> for GltfSceneAssets {
    fn assets(&self) -> &CoreAssets<GltfScene> {
        &self.assets
    }
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

/// Result of routing changed source paths through renderer-facing asset reload systems.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderAssetReloadSummary {
    /// Native Oxide scene and material descriptor reloads.
    pub native: NativeAssetReloadSummary,
    /// glTF scenes that started reloading.
    #[cfg(feature = "gltf-import")]
    pub gltf_scenes: Vec<CoreHandle<GltfScene>>,
}

impl RenderAssetReloadSummary {
    /// Returns true when no changed paths produced renderer-facing asset reloads.
    pub fn is_empty(&self) -> bool {
        self.native.is_empty() && {
            #[cfg(feature = "gltf-import")]
            {
                self.gltf_scenes.is_empty()
            }
            #[cfg(not(feature = "gltf-import"))]
            {
                true
            }
        }
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

/// Registers Oxide's built-in native asset loaders on an [`AssetServer`].
///
/// `DefaultPlugins` installs these automatically. Calling this manually is
/// useful for tools or tests that construct an [`AssetServer`] directly but
/// still want the generic `load_registered_path` workflow for native `.oxscene`
/// and `.oxmat` assets.
pub fn register_native_asset_loaders(server: &mut CoreAssetServer) {
    server.register_loader::<MaterialDescriptor, _>(["oxmat", "json", "ron", "toml"], |path| {
        load_material_descriptor(path).map_err(material_descriptor_asset_error)
    });
    server.register_loader::<SceneDescriptor, _>(["oxscene", "json"], |path| {
        oxide_scene::load_scene_descriptor(path)
            .map_err(|err| CoreAssetServerError::Message(err.to_string()))
    });
}

/// Requests an async material descriptor load and returns a stable typed handle.
pub fn request_material_descriptor_load(
    server: &mut CoreAssetServer,
    path: impl Into<PathBuf>,
) -> MaterialDescriptorHandle {
    register_native_asset_loaders(server);
    let path = path.into();
    match server.load_registered_path::<MaterialDescriptor>(path.clone()) {
        Ok(handle) => handle,
        Err(CoreAssetServerError::NoLoader { .. }) => server.load_path_async(path, |path| {
            load_material_descriptor(&path).map_err(material_descriptor_asset_error)
        }),
        Err(err) => server.load_async(move || Err(err)),
    }
}

/// Starts an in-place reload for a previously loaded material descriptor path.
pub fn reload_material_descriptor_path(
    server: &mut CoreAssetServer,
    path: impl Into<PathBuf>,
) -> Option<MaterialDescriptorHandle> {
    register_native_asset_loaders(server);
    let path = path.into();
    match server.reload_registered_path::<MaterialDescriptor>(path.clone()) {
        Ok(handle) => handle,
        Err(CoreAssetServerError::NoLoader { .. }) => server.reload_path_async(path, |path| {
            load_material_descriptor(&path).map_err(material_descriptor_asset_error)
        }),
        Err(err) => {
            tracing::warn!(
                "Failed to reload material descriptor '{}': {err}",
                path.display()
            );
            None
        }
    }
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

/// Reloads renderer-facing assets affected by changed source paths.
///
/// This includes native `.oxscene`/`.oxmat` assets plus glTF scenes when the
/// `gltf-import` feature and [`RendererResource`] are available.
pub fn reload_changed_render_assets<I, P>(
    world: &mut World,
    changed_paths: I,
) -> RenderAssetReloadSummary
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    let changed_paths: Vec<PathBuf> = changed_paths.into_iter().map(Into::into).collect();
    let native = reload_changed_native_assets(world, changed_paths.iter().cloned());

    #[cfg(feature = "gltf-import")]
    let gltf_scenes = if world.contains_resource::<RendererResource>() {
        let (device, queue) = {
            let renderer = &world.resource::<RendererResource>().renderer;
            (renderer.device.clone(), renderer.queue.clone())
        };
        reload_changed_gltf_scenes(world, device, queue, changed_paths.iter().cloned())
    } else {
        Vec::new()
    };

    RenderAssetReloadSummary {
        native,
        #[cfg(feature = "gltf-import")]
        gltf_scenes,
    }
}

/// Polls the installed [`AssetWatcher`] and reloads affected renderer-facing assets.
pub fn poll_render_asset_reloads(world: &mut World) -> RenderAssetReloadSummary {
    let Some(watcher) = world.get_non_send_resource_mut::<AssetWatcher>() else {
        return RenderAssetReloadSummary::default();
    };
    let changed_paths = watcher.poll_changed_files().to_vec();
    reload_changed_render_assets(world, changed_paths)
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
                    let source_path = server.server.asset_path(&handle).map(PathBuf::from);
                    let dependencies = server
                        .server
                        .asset_path(&handle)
                        .map(|path| material_descriptor_dependencies(path, &descriptor))
                        .unwrap_or_default();
                    let _ = server.server.set_asset_dependencies(&handle, dependencies);
                    completed.push(Ok((handle, source_path, descriptor)));
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
    let mut texture_requests = Vec::new();
    let assets = world.resource_mut::<MaterialDescriptorAssets>();
    for result in completed {
        match result {
            Ok((handle, source_path, descriptor)) => {
                let scene_material = descriptor.clone();
                if let Some(path) = source_path {
                    texture_requests
                        .extend(material_descriptor_texture_sources(&path, &descriptor));
                }
                assets.assets.insert(handle, descriptor);
                scene_materials.push(scene_material);
            }
            Err(err) => tracing::warn!("Failed to load material descriptor: {err}"),
        }
    }
    if !texture_requests.is_empty() {
        publish_material_texture_assets(world, texture_requests);
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

pub(crate) fn publish_material_texture_assets(world: &mut World, textures: Vec<(String, PathBuf)>) {
    if !world.contains_resource::<TextureImageAssets>() {
        world.insert_resource(TextureImageAssets::default());
    }

    let mut loaded = Vec::new();
    for (label, path) in textures {
        match TextureImage::from_file(&path) {
            Ok(image) => loaded.push((label, path, image)),
            Err(err) => tracing::warn!(
                "Failed to load material texture '{}': {err}",
                path.display()
            ),
        }
    }

    if loaded.is_empty() {
        return;
    }

    let mut texture_assets = world
        .remove_resource::<TextureImageAssets>()
        .unwrap_or_default();
    {
        let server = world.resource_mut::<AssetServerResource>();
        for (label, path, image) in loaded {
            let handle = server
                .server
                .register_loaded_path::<TextureImage>(path.clone());
            texture_assets.insert_labeled(label, handle, image);
            texture_assets.associate_label(path.display().to_string(), handle);
        }
    }
    world.insert_resource(texture_assets);
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

fn material_descriptor_texture_sources(
    descriptor_path: &std::path::Path,
    descriptor: &MaterialDescriptor,
) -> Vec<(String, PathBuf)> {
    let base = descriptor_path.parent().map(PathBuf::from);
    let mut textures = Vec::new();
    for texture in [
        descriptor.albedo_texture.as_ref(),
        descriptor.normal_texture.as_ref(),
        descriptor.roughness_texture.as_ref(),
    ]
    .into_iter()
    .flatten()
    .filter(|path| !is_virtual_texture_ref(path))
    {
        let source = (
            texture.clone(),
            resolve_descriptor_dependency(base.as_ref(), texture),
        );
        if !textures.contains(&source) {
            textures.push(source);
        }
    }
    textures
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
    use oxide_ecs::prelude::{Events, Schedule};
    use std::fs;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[derive(Resource, Default)]
    struct TestAssetStore {
        assets: CoreAssets<String>,
    }

    impl AssetStore<String> for TestAssetStore {
        fn assets(&self) -> &CoreAssets<String> {
            &self.assets
        }
    }

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
            metallic_factor: 0.0,
            roughness_factor: 0.5,
            emissive_color: [0.0, 0.0, 0.0],
            alpha_mode: AlphaMode::Opaque,
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
            metallic_factor: 0.0,
            roughness_factor: 0.5,
            emissive_color: [0.0, 0.0, 0.0],
            alpha_mode: AlphaMode::Opaque,
            albedo_texture: Some("#image_0".to_string()),
            normal_texture: None,
            roughness_texture: None,
        };

        assert!(material_descriptor_dependencies("assets/model.gltf", &descriptor).is_empty());
    }

    #[test]
    fn publish_asset_change_events_forwards_unread_asset_changes() {
        let mut allocator = oxide_asset::HandleAllocator::new();
        let handle = allocator.allocate::<String>();
        let mut world = World::new();
        world.insert_resource(TestAssetStore::default());
        world.init_resource::<Events<AssetChange<String>>>();

        world
            .resource_mut::<TestAssetStore>()
            .assets
            .insert(handle, "first".to_string());

        let mut schedule = Schedule::new();
        schedule.add_system(publish_asset_change_events::<String, TestAssetStore>);
        schedule.run(&mut world);

        let events = world.resource::<Events<AssetChange<String>>>();
        assert_eq!(events.len(), 1);
        let first = events.iter().next().unwrap();
        assert_eq!(first.handle, handle);
        assert_eq!(first.revision, 1);
        assert_eq!(first.kind, AssetChangeKind::Added);

        schedule.run(&mut world);
        assert_eq!(world.resource::<Events<AssetChange<String>>>().len(), 1);

        world
            .resource_mut::<TestAssetStore>()
            .assets
            .insert(handle, "second".to_string());
        schedule.run(&mut world);

        let events: Vec<_> = world
            .resource::<Events<AssetChange<String>>>()
            .iter()
            .copied()
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].handle, handle);
        assert_eq!(events[1].revision, 2);
        assert_eq!(events[1].kind, AssetChangeKind::Modified);
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
    fn native_asset_loaders_register_material_descriptor_extensions() {
        let root = temp_dir("oxide_registered_material_loader");
        let material_path = root.join("stone.oxmat");
        write_material(&material_path, "Stone", "stone.wgsl");

        let mut server = CoreAssetServer::new();
        register_native_asset_loaders(&mut server);

        assert_eq!(
            server.registered_loader_extensions::<MaterialDescriptor>(),
            vec!["json", "oxmat", "ron", "toml"]
        );

        let handle = server
            .load_registered_path::<MaterialDescriptor>(&material_path)
            .unwrap();
        let mut assets = CoreAssets::<MaterialDescriptor>::new();
        poll_until_material_named(&mut server, &mut assets, handle, "Stone");

        assert_eq!(assets.revision(&handle), Some(1));
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
    fn material_descriptor_asset_system_publishes_material_texture_images() {
        let root = temp_dir("oxide_scene_material_texture_asset");
        let material_path = root.join("textured.oxmat");
        let albedo_path = root.join("albedo.png");
        let normal_path = root.join("normal.png");
        let roughness_path = root.join("roughness.png");
        write_png_1x1(&albedo_path);
        write_png_1x1(&normal_path);
        write_png_1x1(&roughness_path);
        write_material_with_texture_slots(
            &material_path,
            "Textured",
            "albedo.png",
            "normal.png",
            "roughness.png",
        );

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
            if let Some(image) = world
                .get_resource::<TextureImageAssets>()
                .and_then(|assets| assets.get_labeled("albedo.png"))
            {
                assert_eq!((image.width, image.height), (1, 1));
                assert_eq!(image.rgba.as_slice(), &[255, 0, 0, 255]);
                let texture_assets = world.resource::<TextureImageAssets>();
                assert!(texture_assets.get_labeled("normal.png").is_some());
                assert!(texture_assets.get_labeled("roughness.png").is_some());
                let server = &world.resource::<AssetServerResource>().server;
                assert!(server
                    .handle_for_path::<TextureImage>(&albedo_path)
                    .is_some());
                assert!(server
                    .handle_for_path::<TextureImage>(&normal_path)
                    .is_some());
                assert!(server
                    .handle_for_path::<TextureImage>(&roughness_path)
                    .is_some());
                let _ = fs::remove_dir_all(root);
                return;
            }
            std::thread::yield_now();
        }

        panic!("material texture slots were not published into TextureImageAssets");
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
    fn render_asset_reload_summary_includes_native_fanout_without_renderer() {
        let root = temp_dir("oxide_render_asset_reload");
        let material_path = root.join("stone.oxmat");
        let shader_path = root.join("stone.wgsl");
        fs::write(&shader_path, "// shader").unwrap();
        write_material(&material_path, "Stone", "stone.wgsl");

        let mut world = World::new();
        world.insert_resource(AssetServerResource::default());
        world.insert_resource(MaterialDescriptorAssets::default());
        let material_handle = {
            let server = world.resource_mut::<AssetServerResource>();
            request_material_descriptor_load(&mut server.server, &material_path)
        };
        poll_material_descriptor_asset_system_until_named(&mut world, material_handle, "Stone");

        write_material(&material_path, "Reloaded Stone", "stone.wgsl");
        let summary = reload_changed_render_assets(&mut world, [shader_path.clone()]);

        assert_eq!(summary.native.changed_paths, vec![shader_path]);
        assert_eq!(summary.native.material_descriptors, vec![material_handle]);
        assert!(summary.native.oxscenes.is_empty());
        assert!(!summary.is_empty());
        #[cfg(feature = "gltf-import")]
        assert!(summary.gltf_scenes.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn poll_native_asset_reloads_is_empty_without_watcher() {
        let mut world = World::new();
        assert!(poll_native_asset_reloads(&mut world).is_empty());
    }

    #[test]
    fn poll_render_asset_reloads_is_empty_without_watcher() {
        let mut world = World::new();
        assert!(poll_render_asset_reloads(&mut world).is_empty());
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

    fn poll_material_descriptor_asset_system_until_named(
        world: &mut World,
        handle: MaterialDescriptorHandle,
        expected_name: &str,
    ) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            material_descriptor_asset_system(world);
            if world
                .resource::<MaterialDescriptorAssets>()
                .assets
                .get(&handle)
                .map(|material| material.name.as_str())
                == Some(expected_name)
            {
                return;
            }
            std::thread::yield_now();
        }

        panic!("material descriptor was not loaded with expected name '{expected_name}'");
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

    fn write_material_with_texture_slots(
        path: &std::path::Path,
        name: &str,
        albedo: &str,
        normal: &str,
        roughness: &str,
    ) {
        fs::write(
            path,
            format!(
                r#"{{
                    "format": "oxide.oxmat",
                    "version": 1,
                    "material": {{
                        "name": "{name}",
                        "material_type": "lit",
                        "base_color": [1.0, 1.0, 1.0, 1.0],
                        "shader": {{ "source": "builtin", "shader": "lit" }},
                        "fallback_shader": "lit",
                        "albedo_texture": "{albedo}",
                        "normal_texture": "{normal}",
                        "roughness_texture": "{roughness}"
                    }}
                }}"#
            ),
        )
        .unwrap();
    }

    fn write_png_1x1(path: &std::path::Path) {
        fs::write(
            path,
            [
                0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
                0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
                0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78,
                0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99,
                0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
            ],
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
