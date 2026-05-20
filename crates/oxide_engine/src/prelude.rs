//! Oxide Core engine prelude

pub use crate::animation::{
    transform_tween_system, AnimationPlugin, TransformTween, TweenEasing, TweenRepeat,
    TRANSFORM_TWEEN_SYSTEM,
};
pub use crate::app::{
    app, create_renderer, run_app, App, AppBuilder, AppRunner, AppStage, DefaultPlugins,
    FixedUpdate, InputPlugin, Plugin, PluginGroup, PluginRegistration, PostUpdate, PreUpdate,
    Render, RenderPlugin, Startup, TransformPlugin, Update, GLTF_SCENE_SPAWN_SYSTEM,
    MATERIAL_DESCRIPTOR_ASSET_SYSTEM, OXSCENE_SPAWN_SYSTEM, TRANSFORM_PROPAGATE_SYSTEM,
    VISIBILITY_PROPAGATE_SYSTEM,
};
#[cfg(feature = "gltf-import")]
pub use crate::asset::{load_gltf_async, reload_gltf_async, GltfSceneAssets};
pub use crate::asset::{
    material_descriptor_asset_system, material_descriptor_dependencies,
    poll_material_descriptor_assets, poll_native_asset_reloads, poll_render_asset_reloads,
    register_material_asset, reload_changed_material_descriptors, reload_changed_native_assets,
    reload_changed_render_assets, reload_material_descriptor_path,
    request_material_descriptor_load, AssetChange, AssetChangeCursor, AssetChangeKind,
    AssetLoadStatus, AssetPath, AssetServer, AssetServerError, AssetServerResource, Assets, Handle,
    HandleAllocator, MaterialAssets, MaterialDescriptorAssets, MaterialDescriptorHandle,
    MaterialFilter, MaterialHandle, MeshCache, MeshFilter, MeshHandle, NativeAssetReloadSummary,
    RenderAssetReloadSummary, TextureImageAssets, TextureImageHandle,
};
pub use crate::audio::{
    initialize_audio, Audio, AudioClip, AudioClipError, AudioError, AudioPlugin, AudioTone,
    AudioWaveform, PlaySoundSettings, SoundInstanceId, SpatialSoundSettings,
};
pub use crate::camera::{
    camera_controller_system, CameraBuffer, CameraComponent, CameraController, CameraRenderView,
    CameraUniform, CameraViewport,
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
    SystemParam, TypeKind, TypeMetadata, TypeRegistry, With, Without, World,
};
pub use crate::ecs::{AppExit, FixedTime, RendererResource, Time, WindowResource};
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
    RenderFrame, RenderPassFn, RenderPassInfo, RenderPassKind, RenderPassOrderDiagnostic,
    RenderPassSchedule, RENDER_PASS_APP_QUEUE, RENDER_PASS_EGUI, RENDER_PASS_GAME_TEXT,
    RENDER_PASS_SCENE,
};
#[cfg(feature = "image-import")]
pub use crate::scene::SpriteImageLoadError;
pub use crate::scene::{
    attach_child, despawn_scene_instance, detach_child, entities_in_scene_instance,
    entities_under_scene_path, entities_under_scene_path_in_instance, entities_with_tag,
    entities_with_tag_in_instance, entity_by_scene_path, entity_by_scene_path_in_instance,
    entity_has_tag, first_entity_with_tag, first_entity_with_tag_in_instance,
    initialize_scene_editor, initialize_scene_renderer, install_scene_renderer,
    load_scene_descriptor, mark_subtree_dirty, pick_render_mesh, prepare_scene_renderer,
    queue_oxscene_spawn, queue_scene_renderer, register_sprite, reload_changed_oxscenes,
    reload_oxscene_path, request_oxscene_spawn, resize_scene_renderer, save_scene_descriptor,
    scene_descriptor_dependencies, scene_descriptor_from_roots, scene_descriptor_from_world,
    scene_editor_viewport_system, scene_entity_path, scene_instance_id, scene_prefab_from_roots,
    show_scene_authoring_egui, show_scene_editor_egui, spawn_scene_descriptor,
    spawn_scene_descriptor_instance, spawn_scene_prefab, spawn_scene_prefab_instance,
    spawn_world_descriptor, take_spawned_oxscene_roots, transform_propagate_system,
    try_spawn_scene_descriptor, try_spawn_scene_descriptor_instance, try_spawn_scene_prefab,
    try_spawn_scene_prefab_instance, viewport_pick_ray, visibility_propagate_system,
    with_scene_editor, Children, GizmoAxis, GlobalTransform, HierarchyCommandsExt,
    InheritedVisibility, MeshPrimitive, MeshRenderer, Name, OxSceneDocument, Parent,
    PendingOxSceneSpawns, RenderLayers, RenderMaterial, RenderMesh, SceneAuthoringPlugins,
    SceneBuiltinShader, SceneDescriptor, SceneDescriptorAssets, SceneEditor, SceneEditorPlugin,
    SceneEditorTool, SceneEntityDescriptor, SceneEntityKind, SceneEntityPath, SceneEntitySummary,
    SceneExportError, SceneGizmoLine, SceneGizmoLines, SceneInstanceId, SceneMaterialDescriptor,
    SceneMaterialLibrary, SceneMeshPrimitive, ScenePickHit, ScenePickRay, ScenePrefabDescriptor,
    ScenePrefabOverride, SceneRenderer, SceneRendererPlugin, SceneRendererStats, SceneSpawnResult,
    SceneSpriteDepthMode, SceneSpriteDescriptor, SceneSpriteFacing, SceneTransform,
    SceneValidationDiagnostic, SceneValidationError, SceneWorldDescriptor, SceneWorldSpawnResult,
    SpawnedOxScenes, SpawnedSceneInstance, SpriteAssets, SpriteBillboard, SpriteDepthMode,
    SpriteFacing, SpriteId, SpriteImage, SpriteImageError, Tags, Terrain, TerrainDescriptor,
    TerrainWaveDescriptor, TransformComponent, Visibility, WorldObjectDescriptor, OXSCENE_FORMAT,
    OXSCENE_VERSION,
};
#[cfg(feature = "gltf-import")]
pub use crate::scene::{
    gltf_scene_spawn_system, queue_gltf_scene_spawn, reload_changed_gltf_scenes,
    reload_gltf_scene_path, request_gltf_scene_spawn, spawn_gltf_scene_hierarchy,
    spawn_gltf_scene_hierarchy_with_assets, spawn_gltf_scene_hierarchy_with_meshes,
    take_spawned_scene_roots, GltfMaterialRef, GltfMeshRef, GltfSceneImageHandles,
    GltfSceneInstance, GltfSceneMaterialHandles, GltfSceneMeshHandles, PendingGltfSceneSpawns,
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
