//! Compatibility facade for asset APIs plus engine-level typed asset resources.

use std::path::PathBuf;
use std::sync::Arc;

use oxide_asset::{
    AssetServer as CoreAssetServer, AssetServerError as CoreAssetServerError, Assets as CoreAssets,
    Handle as CoreHandle,
};
use oxide_ecs::Resource;
use oxide_renderer::gltf::{load_gltf, GltfScene};
use oxide_renderer::material::MaterialPipeline;
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

/// ECS resource storing handle-indexed glTF scenes.
#[derive(Resource, Default)]
pub struct GltfSceneAssets {
    pub assets: CoreAssets<GltfScene>,
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
