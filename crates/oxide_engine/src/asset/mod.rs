//! Compatibility facade for asset APIs plus engine-level typed asset resources.

use std::path::PathBuf;
#[cfg(feature = "gltf-import")]
use std::sync::Arc;

#[cfg(feature = "gltf-import")]
use oxide_asset::AssetServerError as CoreAssetServerError;
use oxide_asset::{AssetServer as CoreAssetServer, Assets as CoreAssets, Handle as CoreHandle};
use oxide_ecs::world::World;
use oxide_ecs::{Component, Resource};
use oxide_renderer::descriptor::{
    load_material_descriptor, MaterialDescriptor, MaterialDescriptorError, ShaderDescriptor,
};
#[cfg(feature = "gltf-import")]
use oxide_renderer::gltf::{load_gltf, GltfScene};
use oxide_renderer::material::MaterialPipeline;
use oxide_renderer::mesh::Mesh3D;
#[cfg(feature = "gltf-import")]
use wgpu::{Device, Queue};

use crate::scene::SceneMaterialLibrary;

pub use oxide_asset::*;

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

/// ECS resource storing handle-indexed glTF scenes.
#[cfg(feature = "gltf-import")]
#[derive(Resource, Default)]
pub struct GltfSceneAssets {
    pub assets: CoreAssets<GltfScene>,
}

/// Resource that caches GPU meshes by handle.
pub struct MeshCache {
    meshes: CoreAssets<Mesh3D>,
}

impl MeshCache {
    pub fn new() -> Self {
        Self {
            meshes: CoreAssets::new(),
        }
    }

    pub fn insert(&mut self, handle: CoreHandle<Mesh3D>, mesh: Mesh3D) {
        self.meshes.insert(handle, mesh);
    }

    pub fn get(&self, handle: CoreHandle<Mesh3D>) -> Option<&Mesh3D> {
        self.meshes.get(&handle)
    }

    pub fn remove(&mut self, handle: CoreHandle<Mesh3D>) -> Option<Mesh3D> {
        self.meshes.remove(&handle)
    }

    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }
}

impl Default for MeshCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Component that references a mesh for rendering.
#[derive(Component, Clone, Debug)]
pub struct MeshFilter {
    pub mesh: MeshHandle,
}

impl MeshFilter {
    pub fn new(mesh: MeshHandle) -> Self {
        Self { mesh }
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
    if let Some(path) = &descriptor.albedo_texture {
        dependencies.push(resolve_descriptor_dependency(base.as_ref(), path));
    }
    if let Some(path) = &descriptor.normal_texture {
        dependencies.push(resolve_descriptor_dependency(base.as_ref(), path));
    }
    if let Some(path) = &descriptor.roughness_texture {
        dependencies.push(resolve_descriptor_dependency(base.as_ref(), path));
    }

    dependencies.sort();
    dependencies.dedup();
    dependencies
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
    let path = path.into();
    server.load_async(move || {
        load_gltf(&device, &queue, &path)
            .map_err(|err| CoreAssetServerError::Message(err.to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
                .map(|library| library.contains("Bronze"))
                .unwrap_or(false)
            {
                let _ = fs::remove_dir_all(root);
                return;
            }
            std::thread::yield_now();
        }

        panic!("scene material library was not updated from material descriptor asset");
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
                        "shader": {{ "source": "file", "path": "{shader}" }},
                        "fallback_shader": "lit"
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
