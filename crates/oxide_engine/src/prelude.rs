//! Oxide Core engine prelude

pub use crate::app::{
    app, create_renderer, run_app, App, AppBuilder, AppRunner, AppStage, DefaultPlugins,
    InputPlugin, Plugin, PluginGroup, PostUpdate, PreUpdate, Render, RenderPlugin, TransformPlugin,
    Update,
};
pub use crate::asset::{
    load_gltf_async, register_material_asset, AssetServerResource, GltfSceneAssets, Handle,
    HandleAllocator, MaterialAssets, MeshCache, MeshFilter,
};
pub use crate::camera::{
    camera_controller_system, CameraBuffer, CameraComponent, CameraController, CameraUniform,
};
pub use crate::ecs::{
    in_state, CommandQueue, Commands, Component, Entity, IntoSystem, IntoSystemExt, Query, Res,
    ResMut, Resource, State, System, SystemParam, World,
};
pub use crate::ecs::{RendererResource, Time, WindowResource};
pub use crate::event::{window_event_to_engine, EngineEvent};
pub use crate::input::{ButtonState, KeyboardInput, MouseButton, MouseDelta, MouseInput};
pub use crate::light::{
    AmbientLight, DirectionalLight, LightBuffer, LightUniform, PointLight, MAX_DIRECTIONAL_LIGHTS,
    MAX_POINT_LIGHTS,
};
pub use crate::render::RenderFrame;
pub use crate::scene::{
    attach_child, detach_child, gltf_scene_spawn_system, mark_subtree_dirty,
    queue_gltf_scene_spawn, request_gltf_scene_spawn, spawn_gltf_scene_hierarchy,
    take_spawned_scene_roots, transform_propagate_system, Children, GlobalTransform, GltfMeshRef,
    MeshRenderer, Parent, PendingGltfSceneSpawns, SpawnedGltfScenes, TransformComponent,
};
pub use crate::ui::{handle_egui_event, EguiManager, EguiRender};
pub use crate::watcher::AssetWatcher;
pub use crate::window::Window;
pub use oxide_ecs::schedule::ScheduleLabel;
pub use oxide_math::prelude::*;
pub use oxide_renderer::prelude::*;
pub use winit::keyboard::KeyCode;
