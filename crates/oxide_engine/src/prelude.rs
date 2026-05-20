//! Oxide Core engine prelude

pub use crate::app::{
    app, create_renderer, run_app, App, AppBuilder, AppRunner, AppStage, DefaultPlugins,
    FixedUpdate, InputPlugin, Plugin, PluginGroup, PluginRegistration, PostUpdate, PreUpdate,
    Render, RenderPlugin, Startup, TransformPlugin, Update, GLTF_SCENE_SPAWN_SYSTEM,
    MATERIAL_DESCRIPTOR_ASSET_SYSTEM, OXSCENE_SPAWN_SYSTEM, TRANSFORM_PROPAGATE_SYSTEM,
};
#[cfg(feature = "gltf-import")]
pub use crate::asset::{load_gltf_async, GltfSceneAssets};
pub use crate::asset::{
    material_descriptor_asset_system, material_descriptor_dependencies,
    poll_material_descriptor_assets, register_material_asset, reload_changed_material_descriptors,
    reload_material_descriptor_path, request_material_descriptor_load, AssetChange,
    AssetChangeCursor, AssetChangeKind, AssetLoadStatus, AssetServerResource, Assets, Handle,
    HandleAllocator, MaterialAssets, MaterialDescriptorAssets, MaterialDescriptorHandle,
    MaterialHandle, MeshCache, MeshFilter, MeshHandle,
};
pub use crate::audio::{
    initialize_audio, Audio, AudioClip, AudioClipError, AudioError, AudioPlugin, AudioTone,
    AudioWaveform, PlaySoundSettings, SoundInstanceId,
};
pub use crate::camera::{
    camera_controller_system, CameraBuffer, CameraComponent, CameraController, CameraUniform,
};
pub use crate::diagnostics::{
    frame_diagnostics_system, initialize_diagnostics, Diagnostic, Diagnostics,
    FrameDiagnosticsPlugin, DELTA_SECONDS, FPS, FRAME_TIME_MS,
};
pub use crate::ecs::{
    in_state, state_entered, state_exited, Added, Changed, CommandQueue, Commands, Component,
    ComponentChanges, Entity, EventCursor, EventDrain, EventReader, EventWriter, Events,
    IntoSystem, IntoSystemExt, Local, Query, RemovedComponent, RemovedComponents, Res, ResMut,
    Resource, ResourceCursor, Schedule, ScheduleOrderDiagnostic, State, StateTransition, System,
    SystemParam, With, Without, World,
};
pub use crate::ecs::{FixedTime, RendererResource, Time, WindowResource};
pub use crate::event::{window_event_to_engine, EngineEvent};
pub use crate::input::{
    sync_action_input_system, sync_axis_input_system, ActionBindings, ActionInput, AxisBindings,
    AxisInput, AxisTrigger, ButtonState, InputTrigger, KeyboardInput, MouseButton, MouseDelta,
    MouseInput,
};
pub use crate::light::{
    AmbientLight, DirectionalLight, LightBuffer, LightUniform, PointLight, MAX_DIRECTIONAL_LIGHTS,
    MAX_POINT_LIGHTS,
};
pub use crate::render::{
    RenderFrame, RenderPassFn, RenderPassOrderDiagnostic, RenderPassSchedule,
    RENDER_PASS_APP_QUEUE, RENDER_PASS_EGUI, RENDER_PASS_GAME_TEXT, RENDER_PASS_SCENE,
};
#[cfg(feature = "image-import")]
pub use crate::scene::SpriteImageLoadError;
pub use crate::scene::{
    attach_child, detach_child, initialize_scene_editor, initialize_scene_renderer,
    install_scene_renderer, load_scene_descriptor, mark_subtree_dirty, pick_render_mesh,
    prepare_scene_renderer, queue_oxscene_spawn, queue_scene_renderer, register_sprite,
    reload_changed_oxscenes, reload_oxscene_path, request_oxscene_spawn, resize_scene_renderer,
    save_scene_descriptor, scene_editor_viewport_system, show_scene_authoring_egui,
    show_scene_editor_egui, spawn_scene_descriptor, spawn_scene_prefab, spawn_world_descriptor,
    take_spawned_oxscene_roots, transform_propagate_system, try_spawn_scene_descriptor,
    try_spawn_scene_prefab, viewport_pick_ray, with_scene_editor, Children, GizmoAxis,
    GlobalTransform, HierarchyCommandsExt, MeshPrimitive, MeshRenderer, Name, OxSceneDocument,
    Parent, PendingOxSceneSpawns, RenderMaterial, RenderMesh, SceneAuthoringPlugins,
    SceneBuiltinShader, SceneDescriptor, SceneDescriptorAssets, SceneEditor, SceneEditorPlugin,
    SceneEditorTool, SceneEntityDescriptor, SceneEntityKind, SceneEntitySummary, SceneGizmoLine,
    SceneGizmoLines, SceneMaterialDescriptor, SceneMeshPrimitive, ScenePickHit, ScenePickRay,
    ScenePrefabDescriptor, SceneRenderer, SceneRendererPlugin, SceneRendererStats,
    SceneSpawnResult, SceneSpriteDepthMode, SceneSpriteDescriptor, SceneSpriteFacing,
    SceneTransform, SceneValidationDiagnostic, SceneValidationError, SceneWorldDescriptor,
    SceneWorldSpawnResult, SpawnedOxScenes, SpriteAssets, SpriteBillboard, SpriteDepthMode,
    SpriteFacing, SpriteId, SpriteImage, SpriteImageError, Terrain, TerrainDescriptor,
    TerrainWaveDescriptor, TransformComponent, WorldObjectDescriptor, OXSCENE_FORMAT,
    OXSCENE_VERSION,
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
