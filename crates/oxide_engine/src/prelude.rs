//! Oxide Core engine prelude

pub use crate::app::{
    app, create_renderer, run_app, App, AppBuilder, AppRunner, AppStage, DefaultPlugins,
    InputPlugin, Plugin, PluginGroup, PostUpdate, PreUpdate, Render, RenderPlugin, TransformPlugin,
    Update,
};
#[cfg(feature = "gltf-import")]
pub use crate::asset::{load_gltf_async, GltfSceneAssets};
pub use crate::asset::{
    register_material_asset, AssetLoadStatus, AssetServerResource, Handle, HandleAllocator,
    MaterialAssets, MaterialHandle, MeshCache, MeshFilter, MeshHandle,
};
pub use crate::audio::{
    initialize_audio, Audio, AudioClip, AudioClipError, AudioError, AudioPlugin, AudioTone,
    AudioWaveform, PlaySoundSettings, SoundInstanceId,
};
pub use crate::camera::{
    camera_controller_system, CameraBuffer, CameraComponent, CameraController, CameraUniform,
};
pub use crate::ecs::{
    in_state, CommandQueue, Commands, Component, Entity, Events, IntoSystem, IntoSystemExt, Query,
    Res, ResMut, Resource, State, System, SystemParam, World,
};
pub use crate::ecs::{RendererResource, Time, WindowResource};
pub use crate::event::{window_event_to_engine, EngineEvent};
pub use crate::input::{ButtonState, KeyboardInput, MouseButton, MouseDelta, MouseInput};
pub use crate::light::{
    AmbientLight, DirectionalLight, LightBuffer, LightUniform, PointLight, MAX_DIRECTIONAL_LIGHTS,
    MAX_POINT_LIGHTS,
};
pub use crate::render::RenderFrame;
#[cfg(feature = "image-import")]
pub use crate::scene::SpriteImageLoadError;
pub use crate::scene::{
    attach_child, detach_child, initialize_scene_editor, initialize_scene_renderer,
    install_scene_renderer, load_scene_descriptor, mark_subtree_dirty, pick_render_mesh,
    prepare_scene_renderer, queue_oxscene_spawn, queue_scene_renderer, register_sprite,
    request_oxscene_spawn, resize_scene_renderer, save_scene_descriptor,
    scene_editor_viewport_system, show_scene_authoring_egui, show_scene_editor_egui,
    spawn_scene_descriptor, spawn_world_descriptor, take_spawned_oxscene_roots,
    transform_propagate_system, viewport_pick_ray, with_scene_editor, Children, GizmoAxis,
    GlobalTransform, MeshPrimitive, MeshRenderer, Name, OxSceneDocument, Parent,
    PendingOxSceneSpawns, RenderMaterial, RenderMesh, SceneAuthoringPlugins, SceneBuiltinShader,
    SceneDescriptor, SceneDescriptorAssets, SceneEditor, SceneEditorPlugin, SceneEditorTool,
    SceneEntityDescriptor, SceneEntityKind, SceneEntitySummary, SceneGizmoLine, SceneGizmoLines,
    SceneMaterialDescriptor, SceneMeshPrimitive, ScenePickHit, ScenePickRay, SceneRenderer,
    SceneRendererPlugin, SceneRendererStats, SceneSpawnResult, SceneTransform,
    SceneWorldDescriptor, SceneWorldSpawnResult, SpawnedOxScenes, SpriteAssets, SpriteBillboard,
    SpriteDepthMode, SpriteFacing, SpriteId, SpriteImage, SpriteImageError, Terrain,
    TerrainDescriptor, TerrainWaveDescriptor, TransformComponent, WorldObjectDescriptor,
    OXSCENE_FORMAT, OXSCENE_VERSION,
};
#[cfg(feature = "gltf-import")]
pub use crate::scene::{
    gltf_scene_spawn_system, queue_gltf_scene_spawn, request_gltf_scene_spawn,
    spawn_gltf_scene_hierarchy, take_spawned_scene_roots, GltfMeshRef, PendingGltfSceneSpawns,
    SpawnedGltfScenes,
};
pub use crate::time::{Timer, TimerMode};
pub use crate::ui::{
    authoring_ui_visible, begin_engine_egui_frame, game_ui_sync_system, handle_egui_event,
    handle_engine_egui_event, initialize_authoring_ui, initialize_egui_pass, initialize_game_fonts,
    initialize_game_text_renderer, initialize_game_ui, install_egui_pass,
    install_game_text_renderer, load_game_font, prepare_game_text_renderer, queue_engine_egui,
    queue_game_text_renderer, register_game_font_bytes, toggle_authoring_ui, AuthoringUi,
    DevOverlay, DevOverlayPlugin, DevOverlaySnapshot, EguiManager, EguiPlugin, EguiRender,
    EguiWgpuPass, GameFont, GameFontError, GameFontId, GameFonts, GameTextRenderer,
    GameTextRendererPlugin, GameTextStyle, GameUi, GameUiAnchor, GameUiBar, GameUiButton,
    GameUiCounter, GameUiPlugin, GameUiRect, GameUiReticle, GameUiText, GameUiWidget, RuntimeUi,
    RuntimeUiPlugin, TextHorizontalAlign, TextVerticalAlign, UiElement, BUILTIN_GAME_FONT,
};
pub use crate::watcher::AssetWatcher;
pub use crate::window::Window;
pub use oxide_ecs::schedule::ScheduleLabel;
pub use oxide_math::prelude::*;
pub use oxide_renderer::prelude::*;
pub use winit::keyboard::KeyCode;
