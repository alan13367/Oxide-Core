//! Oxide Core engine prelude

pub use crate::app::{
    app, create_renderer, run_app, App, AppBuilder, AppRunner, AppStage, PostUpdate, PreUpdate,
    Render, Update,
};
pub use crate::asset::{Handle, HandleAllocator, MeshCache, MeshFilter};
pub use crate::camera::{
    camera_controller_system, CameraBuffer, CameraComponent, CameraController, CameraUniform,
};
pub use crate::ecs::{
    Commands, Component, Entity, Query, Res, ResMut, Resource, SystemParam, World,
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
    attach_child, detach_child, mark_subtree_dirty, spawn_gltf_scene_hierarchy,
    transform_propagate_system, Children, GlobalTransform, GltfMeshRef, MeshRenderer, Parent,
    TransformComponent,
};
pub use crate::ui::{handle_egui_event, EguiManager, EguiRender};
pub use crate::watcher::AssetWatcher;
pub use crate::window::Window;
pub use oxide_ecs::schedule::ScheduleLabel;
pub use oxide_math::prelude::*;
pub use oxide_renderer::prelude::*;
pub use winit::keyboard::KeyCode;
