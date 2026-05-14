//! Compatibility facade for asset APIs plus engine-level typed asset resources.

#[cfg(feature = "gltf-import")]
use std::path::PathBuf;
#[cfg(feature = "gltf-import")]
use std::sync::Arc;

#[cfg(feature = "gltf-import")]
use oxide_asset::AssetServerError as CoreAssetServerError;
use oxide_asset::{AssetServer as CoreAssetServer, Assets as CoreAssets, Handle as CoreHandle};
use oxide_ecs::{Component, Resource};
#[cfg(feature = "gltf-import")]
use oxide_renderer::gltf::{load_gltf, GltfScene};
use oxide_renderer::material::MaterialPipeline;
use oxide_renderer::mesh::Mesh3D;
#[cfg(feature = "gltf-import")]
use wgpu::{Device, Queue};

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

pub type MeshHandle = CoreHandle<Mesh3D>;
pub type MaterialHandle = CoreHandle<MaterialPipeline>;

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
