//! Application trait and engine entry points.

use std::marker::PhantomData;
use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowId,
};

use crate::animation::{
    AnimationPlugin, SkeletonSkin, SkeletonSkinAssets, TransformAnimationClip,
    TransformAnimationClipAssets,
};
#[cfg(feature = "gltf-import")]
use crate::asset::GltfSceneAssets;
use crate::asset::{
    material_descriptor_asset_system, publish_asset_change_events, register_native_asset_loaders,
    AssetChange, AssetServerResource, MaterialAssets, MaterialDescriptorAssets, MeshCache,
    TextureImageAssets,
};
use crate::diagnostics::FrameDiagnosticsPlugin;
use crate::ecs::{
    AppExit, Events, FixedTime, IntoSystem, RendererResource, Schedule, Time, WindowResource, World,
};
use crate::event::{window_event_to_engine, EngineEvent};
use crate::input::{KeyboardInput, MouseInput};
use crate::render::{
    RenderFrame, RenderPassAnchor, RenderPassFn, RenderPassSchedule, RenderPassStep,
};
#[cfg(feature = "gltf-import")]
use crate::scene::{
    gltf_scene_spawn_system, GltfSceneAnimationHandles, GltfSceneImageHandles,
    GltfSceneMaterialHandles, GltfSceneMeshHandles, GltfSceneSkinHandles, PendingGltfSceneSpawns,
    SpawnedGltfScenes,
};
use crate::scene::{
    oxscene_spawn_system, prepare_scene_renderer, queue_scene_renderer, resize_scene_renderer,
    transform_propagate_system, visibility_propagate_system, PendingOxSceneSpawns,
    SceneDescriptorAssets, SpawnedOxScenes,
};
use crate::ui::{
    authoring_ui_visible, begin_engine_egui_frame, handle_egui_event, handle_engine_egui_event,
    prepare_game_text_renderer, queue_engine_egui, queue_game_text_renderer, toggle_authoring_ui,
    EguiManager,
};
use crate::window::Window;
#[cfg(feature = "gltf-import")]
use oxide_renderer::gltf::GltfScene;
use oxide_renderer::{
    descriptor::MaterialDescriptor, material::MaterialPipeline, mesh::Mesh3D,
    texture::TextureImage, Renderer,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Startup;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PreUpdate;

/// Fixed-step gameplay stage driven by [`FixedTime`](crate::ecs::FixedTime).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FixedUpdate;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Update;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PostUpdate;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Render;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppStage {
    /// Runs once after app/window initialization and startup plugin hooks.
    Startup,
    /// Runs once per rendered frame before fixed and variable gameplay work.
    PreUpdate,
    /// Runs zero or more times per rendered frame using the `FixedTime` accumulator.
    FixedUpdate,
    /// Runs once per rendered frame for variable-rate gameplay work.
    Update,
    /// Runs once per rendered frame after update systems.
    PostUpdate,
    /// Runs before render preparation to extract app data for rendering.
    Extract,
    /// Runs before queueing render work.
    Prepare,
}

/// Stable label for the built-in transform propagation system.
pub const TRANSFORM_PROPAGATE_SYSTEM: &str = "oxide.transform.propagate";
/// Stable label for the built-in visibility propagation system.
pub const VISIBILITY_PROPAGATE_SYSTEM: &str = "oxide.visibility.propagate";
/// Stable label for the built-in material descriptor asset polling system.
pub const MATERIAL_DESCRIPTOR_ASSET_SYSTEM: &str = "oxide.asset.material_descriptors";
/// Stable label for publishing material pipeline asset change events.
pub const MATERIAL_ASSET_EVENTS_SYSTEM: &str = "oxide.asset.events.materials";
/// Stable label for publishing mesh asset change events.
pub const MESH_ASSET_EVENTS_SYSTEM: &str = "oxide.asset.events.meshes";
/// Stable label for publishing material descriptor asset change events.
pub const MATERIAL_DESCRIPTOR_ASSET_EVENTS_SYSTEM: &str = "oxide.asset.events.material_descriptors";
/// Stable label for publishing texture image asset change events.
pub const TEXTURE_IMAGE_ASSET_EVENTS_SYSTEM: &str = "oxide.asset.events.texture_images";
/// Stable label for publishing transform animation clip asset change events.
pub const TRANSFORM_ANIMATION_CLIP_ASSET_EVENTS_SYSTEM: &str =
    "oxide.asset.events.transform_animation_clips";
/// Stable label for publishing skeleton skin asset change events.
pub const SKELETON_SKIN_ASSET_EVENTS_SYSTEM: &str = "oxide.asset.events.skeleton_skins";
/// Stable label for the built-in native `.oxscene` spawn system.
pub const OXSCENE_SPAWN_SYSTEM: &str = "oxide.scene.oxscene_spawn";
/// Stable label for the built-in glTF hierarchy spawn system.
pub const GLTF_SCENE_SPAWN_SYSTEM: &str = "oxide.scene.gltf_spawn";
/// Stable label for publishing glTF scene asset change events.
#[cfg(feature = "gltf-import")]
pub const GLTF_SCENE_ASSET_EVENTS_SYSTEM: &str = "oxide.asset.events.gltf_scenes";

pub type StartupSystemFn = fn(&mut World, &Window);

struct RunnerSystems {
    startup: Vec<StartupSystemFn>,
    startup_schedule: Schedule,
    pre_update: Schedule,
    fixed_update: Schedule,
    update: Schedule,
    post_update: Schedule,
    extract: Schedule,
    prepare: Schedule,
    render_passes: RenderPassSchedule,
}

impl Default for RunnerSystems {
    fn default() -> Self {
        Self {
            startup: Vec::new(),
            startup_schedule: Schedule::new(),
            pre_update: Schedule::new(),
            fixed_update: Schedule::new(),
            update: Schedule::new(),
            post_update: Schedule::new(),
            extract: Schedule::new(),
            prepare: Schedule::new(),
            render_passes: RenderPassSchedule::new(),
        }
    }
}

impl RunnerSystems {
    fn run(stage_systems: &mut Schedule, world: &mut World) {
        stage_systems.run(world);
    }
}

pub trait App: 'static {
    fn configure(world: &mut World);
    fn init(window: &Window, renderer: Renderer) -> Self;
    fn world(&self) -> &World;
    fn world_mut(&mut self) -> &mut World;

    fn update(&mut self);
    fn extract(&mut self) {}
    fn prepare(&mut self) {}
    fn queue(&mut self, _frame: &mut RenderFrame) {}

    fn on_event(&mut self, event: EngineEvent);

    /// Returns the egui manager if the app integrates editor/debug UI.
    fn egui_manager_mut(&mut self) -> Option<&mut EguiManager> {
        None
    }
}

pub trait Plugin<T: App> {
    /// Stable plugin name used for diagnostics and duplicate registration checks.
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Returns true when only one instance of this plugin should be registered.
    ///
    /// Most engine plugins are unique. Override this for plugin types that are
    /// intentionally installed multiple times with different configuration.
    fn is_unique(&self) -> bool {
        true
    }

    fn build(&self, app: &mut AppBuilder<T>);
}

pub trait PluginGroup<T: App> {
    fn build(self, app: &mut AppBuilder<T>);
}

pub struct InputPlugin;

impl<T: App> Plugin<T> for InputPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_input_resources);
    }
}

fn initialize_input_resources(world: &mut World, _window: &Window) {
    if !world.contains_resource::<Time>() {
        world.init_resource::<Time>();
    }
    if !world.contains_resource::<FixedTime>() {
        world.init_resource::<FixedTime>();
    }
    if !world.contains_resource::<KeyboardInput>() {
        world.init_resource::<KeyboardInput>();
    }
    if !world.contains_resource::<MouseInput>() {
        world.init_resource::<MouseInput>();
    }
}

pub struct TransformPlugin;

impl<T: App> Plugin<T> for TransformPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_labeled_system_before_mut(
            AppStage::PostUpdate,
            VISIBILITY_PROPAGATE_SYSTEM,
            TRANSFORM_PROPAGATE_SYSTEM,
            visibility_propagate_system,
        );
        app.add_labeled_system_mut(
            AppStage::PostUpdate,
            TRANSFORM_PROPAGATE_SYSTEM,
            transform_propagate_system,
        );
    }
}

pub struct RenderPlugin;

impl<T: App> Plugin<T> for RenderPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_window_resource);
        app.add_startup_system_mut(initialize_asset_resources);
        app.add_labeled_system_mut(
            AppStage::PreUpdate,
            MATERIAL_DESCRIPTOR_ASSET_SYSTEM,
            material_descriptor_asset_system,
        );
        app.add_labeled_system_mut(
            AppStage::PreUpdate,
            OXSCENE_SPAWN_SYSTEM,
            oxscene_spawn_system,
        );
        #[cfg(feature = "gltf-import")]
        app.add_labeled_system_mut(
            AppStage::PreUpdate,
            GLTF_SCENE_SPAWN_SYSTEM,
            gltf_scene_spawn_system,
        );
        install_builtin_asset_event_systems(app);
    }
}

fn install_builtin_asset_event_systems<T: App>(app: &mut AppBuilder<T>) {
    #[cfg(feature = "gltf-import")]
    const ASSET_EVENT_ANCHOR: &str = GLTF_SCENE_SPAWN_SYSTEM;
    #[cfg(not(feature = "gltf-import"))]
    const ASSET_EVENT_ANCHOR: &str = OXSCENE_SPAWN_SYSTEM;

    app.add_labeled_system_after_mut(
        AppStage::PreUpdate,
        MATERIAL_ASSET_EVENTS_SYSTEM,
        ASSET_EVENT_ANCHOR,
        publish_asset_change_events::<MaterialPipeline, MaterialAssets>,
    );
    app.add_labeled_system_after_mut(
        AppStage::PreUpdate,
        MESH_ASSET_EVENTS_SYSTEM,
        ASSET_EVENT_ANCHOR,
        publish_asset_change_events::<Mesh3D, MeshCache>,
    );
    app.add_labeled_system_after_mut(
        AppStage::PreUpdate,
        MATERIAL_DESCRIPTOR_ASSET_EVENTS_SYSTEM,
        ASSET_EVENT_ANCHOR,
        publish_asset_change_events::<MaterialDescriptor, MaterialDescriptorAssets>,
    );
    app.add_labeled_system_after_mut(
        AppStage::PreUpdate,
        TEXTURE_IMAGE_ASSET_EVENTS_SYSTEM,
        ASSET_EVENT_ANCHOR,
        publish_asset_change_events::<TextureImage, TextureImageAssets>,
    );
    app.add_labeled_system_after_mut(
        AppStage::PreUpdate,
        TRANSFORM_ANIMATION_CLIP_ASSET_EVENTS_SYSTEM,
        ASSET_EVENT_ANCHOR,
        publish_asset_change_events::<TransformAnimationClip, TransformAnimationClipAssets>,
    );
    app.add_labeled_system_after_mut(
        AppStage::PreUpdate,
        SKELETON_SKIN_ASSET_EVENTS_SYSTEM,
        ASSET_EVENT_ANCHOR,
        publish_asset_change_events::<SkeletonSkin, SkeletonSkinAssets>,
    );
    #[cfg(feature = "gltf-import")]
    app.add_labeled_system_after_mut(
        AppStage::PreUpdate,
        GLTF_SCENE_ASSET_EVENTS_SYSTEM,
        ASSET_EVENT_ANCHOR,
        publish_asset_change_events::<GltfScene, GltfSceneAssets>,
    );
}

fn initialize_window_resource(world: &mut World, window: &Window) {
    if !world.contains_resource::<WindowResource>() {
        let size = window.size();
        world.insert_resource(WindowResource::new(size.width, size.height));
    }
}

fn initialize_asset_resources(world: &mut World, _window: &Window) {
    if !world.contains_resource::<AssetServerResource>() {
        world.insert_resource(AssetServerResource::default());
    }
    {
        let server = world.resource_mut::<AssetServerResource>();
        register_native_asset_loaders(&mut server.server);
    }
    if !world.contains_resource::<MaterialAssets>() {
        world.insert_resource(MaterialAssets::default());
    }
    if !world.contains_resource::<MaterialDescriptorAssets>() {
        world.insert_resource(MaterialDescriptorAssets::default());
    }
    if !world.contains_resource::<MeshCache>() {
        world.insert_resource(MeshCache::default());
    }
    if !world.contains_resource::<TextureImageAssets>() {
        world.insert_resource(TextureImageAssets::default());
    }
    if !world.contains_resource::<TransformAnimationClipAssets>() {
        world.insert_resource(TransformAnimationClipAssets::default());
    }
    if !world.contains_resource::<SkeletonSkinAssets>() {
        world.insert_resource(SkeletonSkinAssets::default());
    }
    world.init_resource::<Events<AssetChange<MaterialPipeline>>>();
    world.init_resource::<Events<AssetChange<Mesh3D>>>();
    world.init_resource::<Events<AssetChange<MaterialDescriptor>>>();
    world.init_resource::<Events<AssetChange<TextureImage>>>();
    world.init_resource::<Events<AssetChange<TransformAnimationClip>>>();
    world.init_resource::<Events<AssetChange<SkeletonSkin>>>();
    if !world.contains_resource::<SceneDescriptorAssets>() {
        world.insert_resource(SceneDescriptorAssets::default());
    }
    if !world.contains_resource::<PendingOxSceneSpawns>() {
        world.insert_resource(PendingOxSceneSpawns::default());
    }
    if !world.contains_resource::<SpawnedOxScenes>() {
        world.insert_resource(SpawnedOxScenes::default());
    }
    #[cfg(feature = "gltf-import")]
    {
        if !world.contains_resource::<GltfSceneAssets>() {
            world.insert_resource(GltfSceneAssets::default());
        }
        world.init_resource::<Events<AssetChange<GltfScene>>>();
        if !world.contains_resource::<PendingGltfSceneSpawns>() {
            world.insert_resource(PendingGltfSceneSpawns::default());
        }
        if !world.contains_resource::<SpawnedGltfScenes>() {
            world.insert_resource(SpawnedGltfScenes::default());
        }
        if !world.contains_resource::<GltfSceneMeshHandles>() {
            world.insert_resource(GltfSceneMeshHandles::default());
        }
        if !world.contains_resource::<GltfSceneMaterialHandles>() {
            world.insert_resource(GltfSceneMaterialHandles::default());
        }
        if !world.contains_resource::<GltfSceneImageHandles>() {
            world.insert_resource(GltfSceneImageHandles::default());
        }
        if !world.contains_resource::<GltfSceneAnimationHandles>() {
            world.insert_resource(GltfSceneAnimationHandles::default());
        }
        if !world.contains_resource::<GltfSceneSkinHandles>() {
            world.insert_resource(GltfSceneSkinHandles::default());
        }
    }
}

fn resize_engine_render_resources(world: &mut World, width: u32, height: u32) {
    if width == 0 || height == 0 {
        return;
    }

    if world.contains_resource::<WindowResource>() {
        world.resource_mut::<WindowResource>().update(width, height);
    }

    if world.contains_resource::<RendererResource>() {
        world
            .resource_mut::<RendererResource>()
            .renderer
            .resize(width, height);
    }

    resize_scene_renderer(world, width, height);
}

fn sync_cursor_capture(
    world: &mut World,
    window: &Window,
    focused: bool,
    force: bool,
    synced_cursor_grabbed: &mut Option<bool>,
) {
    let cursor_grabbed = if world.contains_resource::<MouseInput>() {
        world.resource::<MouseInput>().cursor_grabbed()
    } else {
        false
    };

    if !focused {
        if let Err(err) = window.set_cursor_grabbed(false) {
            tracing::warn!("Failed to release cursor: {err}");
        }
        window.set_cursor_visible(true);
        *synced_cursor_grabbed = None;
        return;
    }

    if !force && *synced_cursor_grabbed == Some(cursor_grabbed) {
        return;
    }

    if cursor_grabbed {
        window.set_cursor_visible(false);
        if let Err(err) = window.set_cursor_grabbed(true) {
            tracing::warn!("Failed to grab cursor: {err}");
        }
        let size = window.size();
        let center = PhysicalPosition::new(size.width as f64 * 0.5, size.height as f64 * 0.5);
        if let Err(err) = window.set_cursor_position(center) {
            tracing::warn!("Failed to center cursor: {err}");
        }
        if world.contains_resource::<MouseInput>() {
            world.resource_mut::<MouseInput>().set_position(center);
        }
    } else {
        if let Err(err) = window.set_cursor_grabbed(false) {
            tracing::warn!("Failed to release cursor: {err}");
        }
        window.set_cursor_visible(true);
    }

    *synced_cursor_grabbed = Some(cursor_grabbed);
}

pub struct DefaultPlugins;

impl<T: App> PluginGroup<T> for DefaultPlugins {
    fn build(self, app: &mut AppBuilder<T>) {
        app.add_plugin_mut(InputPlugin);
        app.add_plugin_mut(TransformPlugin);
        app.add_plugin_mut(AnimationPlugin);
        app.add_plugin_mut(RenderPlugin);
        app.add_plugin_mut(FrameDiagnosticsPlugin);
    }
}

/// Metadata for a plugin registered with an [`AppBuilder`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PluginRegistration {
    /// Stable name returned by [`Plugin::name`].
    pub name: &'static str,
    /// Whether this plugin suppresses later registrations with the same name.
    pub unique: bool,
}

pub struct AppBuilder<T: App> {
    systems: RunnerSystems,
    plugins: Vec<PluginRegistration>,
    _marker: PhantomData<T>,
}

impl<T: App> Default for AppBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: App> AppBuilder<T> {
    pub fn new() -> Self {
        Self {
            systems: RunnerSystems::default(),
            plugins: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Returns metadata for plugins already registered with this builder.
    pub fn plugins(&self) -> &[PluginRegistration] {
        &self.plugins
    }

    /// Returns true when a plugin with `name` has already been registered.
    pub fn has_plugin(&self, name: &str) -> bool {
        self.plugins.iter().any(|plugin| plugin.name == name)
    }

    pub fn add_system<S, Marker>(mut self, stage: AppStage, system: S) -> Self
    where
        S: IntoSystem<Marker>,
    {
        self.add_system_mut(stage, system);
        self
    }

    pub fn add_system_mut<S, Marker>(&mut self, stage: AppStage, system: S) -> &mut Self
    where
        S: IntoSystem<Marker>,
    {
        self.stage_schedule_mut(stage).add_system(system);
        self
    }

    /// Adds a system to a named set in the same stage.
    pub fn add_system_to_set<S, Marker>(
        mut self,
        stage: AppStage,
        set: impl Into<String>,
        system: S,
    ) -> Self
    where
        S: IntoSystem<Marker>,
    {
        self.add_system_to_set_mut(stage, set, system);
        self
    }

    /// Mutable form of [`Self::add_system_to_set`].
    pub fn add_system_to_set_mut<S, Marker>(
        &mut self,
        stage: AppStage,
        set: impl Into<String>,
        system: S,
    ) -> &mut Self
    where
        S: IntoSystem<Marker>,
    {
        self.stage_schedule_mut(stage)
            .add_system_to_set(set, system);
        self
    }

    /// Adds a labeled system that other systems in the same stage can order
    /// themselves before or after.
    pub fn add_labeled_system<S, Marker>(
        mut self,
        stage: AppStage,
        label: impl Into<String>,
        system: S,
    ) -> Self
    where
        S: IntoSystem<Marker>,
    {
        self.add_labeled_system_mut(stage, label, system);
        self
    }

    /// Adds a labeled system to a named set in the same stage.
    pub fn add_labeled_system_to_set<S, Marker>(
        mut self,
        stage: AppStage,
        label: impl Into<String>,
        set: impl Into<String>,
        system: S,
    ) -> Self
    where
        S: IntoSystem<Marker>,
    {
        self.add_labeled_system_to_set_mut(stage, label, set, system);
        self
    }

    /// Mutable form of [`Self::add_labeled_system_to_set`].
    pub fn add_labeled_system_to_set_mut<S, Marker>(
        &mut self,
        stage: AppStage,
        label: impl Into<String>,
        set: impl Into<String>,
        system: S,
    ) -> &mut Self
    where
        S: IntoSystem<Marker>,
    {
        self.stage_schedule_mut(stage)
            .add_labeled_system_to_set(label, set, system);
        self
    }

    /// Mutable form of [`Self::add_labeled_system`].
    pub fn add_labeled_system_mut<S, Marker>(
        &mut self,
        stage: AppStage,
        label: impl Into<String>,
        system: S,
    ) -> &mut Self
    where
        S: IntoSystem<Marker>,
    {
        self.stage_schedule_mut(stage)
            .add_labeled_system(label, system);
        self
    }

    /// Adds a labeled system that runs before `before_label` in the same stage.
    pub fn add_labeled_system_before<S, Marker>(
        mut self,
        stage: AppStage,
        label: impl Into<String>,
        before_label: impl Into<String>,
        system: S,
    ) -> Self
    where
        S: IntoSystem<Marker>,
    {
        self.add_labeled_system_before_mut(stage, label, before_label, system);
        self
    }

    /// Mutable form of [`Self::add_labeled_system_before`].
    pub fn add_labeled_system_before_mut<S, Marker>(
        &mut self,
        stage: AppStage,
        label: impl Into<String>,
        before_label: impl Into<String>,
        system: S,
    ) -> &mut Self
    where
        S: IntoSystem<Marker>,
    {
        self.stage_schedule_mut(stage)
            .add_labeled_system_before(label, before_label, system);
        self
    }

    /// Adds a system that runs before `before_label` in the same stage.
    pub fn add_system_before<S, Marker>(
        mut self,
        stage: AppStage,
        before_label: impl Into<String>,
        system: S,
    ) -> Self
    where
        S: IntoSystem<Marker>,
    {
        self.add_system_before_mut(stage, before_label, system);
        self
    }

    /// Mutable form of [`Self::add_system_before`].
    pub fn add_system_before_mut<S, Marker>(
        &mut self,
        stage: AppStage,
        before_label: impl Into<String>,
        system: S,
    ) -> &mut Self
    where
        S: IntoSystem<Marker>,
    {
        self.stage_schedule_mut(stage)
            .add_system_before(before_label, system);
        self
    }

    /// Adds a labeled system that runs after `after_label` in the same stage.
    pub fn add_labeled_system_after<S, Marker>(
        mut self,
        stage: AppStage,
        label: impl Into<String>,
        after_label: impl Into<String>,
        system: S,
    ) -> Self
    where
        S: IntoSystem<Marker>,
    {
        self.add_labeled_system_after_mut(stage, label, after_label, system);
        self
    }

    /// Mutable form of [`Self::add_labeled_system_after`].
    pub fn add_labeled_system_after_mut<S, Marker>(
        &mut self,
        stage: AppStage,
        label: impl Into<String>,
        after_label: impl Into<String>,
        system: S,
    ) -> &mut Self
    where
        S: IntoSystem<Marker>,
    {
        self.stage_schedule_mut(stage)
            .add_labeled_system_after(label, after_label, system);
        self
    }

    /// Adds a system that runs after `after_label` in the same stage.
    pub fn add_system_after<S, Marker>(
        mut self,
        stage: AppStage,
        after_label: impl Into<String>,
        system: S,
    ) -> Self
    where
        S: IntoSystem<Marker>,
    {
        self.add_system_after_mut(stage, after_label, system);
        self
    }

    /// Mutable form of [`Self::add_system_after`].
    pub fn add_system_after_mut<S, Marker>(
        &mut self,
        stage: AppStage,
        after_label: impl Into<String>,
        system: S,
    ) -> &mut Self
    where
        S: IntoSystem<Marker>,
    {
        self.stage_schedule_mut(stage)
            .add_system_after(after_label, system);
        self
    }

    /// Orders every system in `set` before `before_label` in the same stage.
    pub fn configure_set_before(
        mut self,
        stage: AppStage,
        set: impl Into<String>,
        before_label: impl Into<String>,
    ) -> Self {
        self.configure_set_before_mut(stage, set, before_label);
        self
    }

    /// Mutable form of [`Self::configure_set_before`].
    pub fn configure_set_before_mut(
        &mut self,
        stage: AppStage,
        set: impl Into<String>,
        before_label: impl Into<String>,
    ) -> &mut Self {
        self.stage_schedule_mut(stage)
            .configure_set_before(set, before_label);
        self
    }

    /// Orders every system in `set` after `after_label` in the same stage.
    pub fn configure_set_after(
        mut self,
        stage: AppStage,
        set: impl Into<String>,
        after_label: impl Into<String>,
    ) -> Self {
        self.configure_set_after_mut(stage, set, after_label);
        self
    }

    /// Mutable form of [`Self::configure_set_after`].
    pub fn configure_set_after_mut(
        &mut self,
        stage: AppStage,
        set: impl Into<String>,
        after_label: impl Into<String>,
    ) -> &mut Self {
        self.stage_schedule_mut(stage)
            .configure_set_after(set, after_label);
        self
    }

    /// Adds a render pass to the default custom-pass position.
    ///
    /// Custom render passes run after `AppStage::Prepare` and receive the
    /// active frame encoder/view. By default they execute after built-in scene
    /// and text passes and before [`App::queue`].
    pub fn add_render_pass(mut self, label: impl Into<String>, pass: RenderPassFn) -> Self {
        self.add_render_pass_mut(label, pass);
        self
    }

    /// Mutable form of [`Self::add_render_pass`].
    pub fn add_render_pass_mut(
        &mut self,
        label: impl Into<String>,
        pass: RenderPassFn,
    ) -> &mut Self {
        self.systems.render_passes.add_pass(label, pass);
        self
    }

    /// Returns the current render pass schedule.
    pub fn render_pass_schedule(&self) -> &RenderPassSchedule {
        &self.systems.render_passes
    }

    /// Returns the current render pass schedule for direct configuration.
    pub fn render_pass_schedule_mut(&mut self) -> &mut RenderPassSchedule {
        &mut self.systems.render_passes
    }

    /// Sets enabled state for every custom render pass matching `label`.
    ///
    /// Built-in anchors are retained and always enabled.
    pub fn set_render_pass_enabled(mut self, label: impl AsRef<str>, enabled: bool) -> Self {
        self.set_render_pass_enabled_mut(label, enabled);
        self
    }

    /// Mutable form of [`Self::set_render_pass_enabled`].
    pub fn set_render_pass_enabled_mut(
        &mut self,
        label: impl AsRef<str>,
        enabled: bool,
    ) -> &mut Self {
        self.systems.render_passes.set_pass_enabled(label, enabled);
        self
    }

    /// Enables every custom render pass matching `label`.
    pub fn enable_render_pass(mut self, label: impl AsRef<str>) -> Self {
        self.enable_render_pass_mut(label);
        self
    }

    /// Mutable form of [`Self::enable_render_pass`].
    pub fn enable_render_pass_mut(&mut self, label: impl AsRef<str>) -> &mut Self {
        self.systems.render_passes.enable_pass(label);
        self
    }

    /// Disables every custom render pass matching `label` without unregistering it.
    pub fn disable_render_pass(mut self, label: impl AsRef<str>) -> Self {
        self.disable_render_pass_mut(label);
        self
    }

    /// Mutable form of [`Self::disable_render_pass`].
    pub fn disable_render_pass_mut(&mut self, label: impl AsRef<str>) -> &mut Self {
        self.systems.render_passes.disable_pass(label);
        self
    }

    /// Sets enabled state for every custom render pass in `set`.
    ///
    /// This is useful for plugin-owned pass groups such as debug overlays,
    /// capture passes, or post-processing stacks.
    pub fn set_render_pass_set_enabled(mut self, set: impl AsRef<str>, enabled: bool) -> Self {
        self.set_render_pass_set_enabled_mut(set, enabled);
        self
    }

    /// Mutable form of [`Self::set_render_pass_set_enabled`].
    pub fn set_render_pass_set_enabled_mut(
        &mut self,
        set: impl AsRef<str>,
        enabled: bool,
    ) -> &mut Self {
        self.systems
            .render_passes
            .set_pass_set_enabled(set, enabled);
        self
    }

    /// Enables every custom render pass in `set`.
    pub fn enable_render_pass_set(mut self, set: impl AsRef<str>) -> Self {
        self.enable_render_pass_set_mut(set);
        self
    }

    /// Mutable form of [`Self::enable_render_pass_set`].
    pub fn enable_render_pass_set_mut(&mut self, set: impl AsRef<str>) -> &mut Self {
        self.systems.render_passes.enable_set(set);
        self
    }

    /// Disables every custom render pass in `set` without unregistering it.
    pub fn disable_render_pass_set(mut self, set: impl AsRef<str>) -> Self {
        self.disable_render_pass_set_mut(set);
        self
    }

    /// Mutable form of [`Self::disable_render_pass_set`].
    pub fn disable_render_pass_set_mut(&mut self, set: impl AsRef<str>) -> &mut Self {
        self.systems.render_passes.disable_set(set);
        self
    }

    /// Adds a render pass to a named render ordering set.
    pub fn add_render_pass_to_set(
        mut self,
        label: impl Into<String>,
        set: impl Into<String>,
        pass: RenderPassFn,
    ) -> Self {
        self.add_render_pass_to_set_mut(label, set, pass);
        self
    }

    /// Mutable form of [`Self::add_render_pass_to_set`].
    pub fn add_render_pass_to_set_mut(
        &mut self,
        label: impl Into<String>,
        set: impl Into<String>,
        pass: RenderPassFn,
    ) -> &mut Self {
        self.systems.render_passes.add_pass_to_set(label, set, pass);
        self
    }

    /// Adds a render pass that runs before `before_label`.
    pub fn add_render_pass_before(
        mut self,
        label: impl Into<String>,
        before_label: impl Into<String>,
        pass: RenderPassFn,
    ) -> Self {
        self.add_render_pass_before_mut(label, before_label, pass);
        self
    }

    /// Mutable form of [`Self::add_render_pass_before`].
    pub fn add_render_pass_before_mut(
        &mut self,
        label: impl Into<String>,
        before_label: impl Into<String>,
        pass: RenderPassFn,
    ) -> &mut Self {
        self.systems
            .render_passes
            .add_pass_before(label, before_label, pass);
        self
    }

    /// Adds a render pass that runs after `after_label`.
    pub fn add_render_pass_after(
        mut self,
        label: impl Into<String>,
        after_label: impl Into<String>,
        pass: RenderPassFn,
    ) -> Self {
        self.add_render_pass_after_mut(label, after_label, pass);
        self
    }

    /// Mutable form of [`Self::add_render_pass_after`].
    pub fn add_render_pass_after_mut(
        &mut self,
        label: impl Into<String>,
        after_label: impl Into<String>,
        pass: RenderPassFn,
    ) -> &mut Self {
        self.systems
            .render_passes
            .add_pass_after(label, after_label, pass);
        self
    }

    /// Orders every render pass in `set` before `before_label`.
    pub fn configure_render_pass_set_before(
        mut self,
        set: impl Into<String>,
        before_label: impl Into<String>,
    ) -> Self {
        self.configure_render_pass_set_before_mut(set, before_label);
        self
    }

    /// Mutable form of [`Self::configure_render_pass_set_before`].
    pub fn configure_render_pass_set_before_mut(
        &mut self,
        set: impl Into<String>,
        before_label: impl Into<String>,
    ) -> &mut Self {
        self.systems
            .render_passes
            .configure_set_before(set, before_label);
        self
    }

    /// Orders every render pass in `set` after `after_label`.
    pub fn configure_render_pass_set_after(
        mut self,
        set: impl Into<String>,
        after_label: impl Into<String>,
    ) -> Self {
        self.configure_render_pass_set_after_mut(set, after_label);
        self
    }

    /// Mutable form of [`Self::configure_render_pass_set_after`].
    pub fn configure_render_pass_set_after_mut(
        &mut self,
        set: impl Into<String>,
        after_label: impl Into<String>,
    ) -> &mut Self {
        self.systems
            .render_passes
            .configure_set_after(set, after_label);
        self
    }

    pub fn add_startup_system(mut self, system: StartupSystemFn) -> Self {
        self.add_startup_system_mut(system);
        self
    }

    pub fn add_startup_system_mut(&mut self, system: StartupSystemFn) -> &mut Self {
        self.systems.startup.push(system);
        self
    }

    fn stage_schedule_mut(&mut self, stage: AppStage) -> &mut Schedule {
        match stage {
            AppStage::Startup => &mut self.systems.startup_schedule,
            AppStage::PreUpdate => &mut self.systems.pre_update,
            AppStage::FixedUpdate => &mut self.systems.fixed_update,
            AppStage::Update => &mut self.systems.update,
            AppStage::PostUpdate => &mut self.systems.post_update,
            AppStage::Extract => &mut self.systems.extract,
            AppStage::Prepare => &mut self.systems.prepare,
        }
    }

    pub fn add_plugin<P>(mut self, plugin: P) -> Self
    where
        P: Plugin<T>,
    {
        self.add_plugin_mut(plugin);
        self
    }

    pub fn add_plugin_mut<P>(&mut self, plugin: P) -> &mut Self
    where
        P: Plugin<T>,
    {
        let registration = PluginRegistration {
            name: plugin.name(),
            unique: plugin.is_unique(),
        };
        if registration.unique && self.has_plugin(registration.name) {
            tracing::debug!("Skipping duplicate plugin {}", registration.name);
            return self;
        }

        self.plugins.push(registration);
        plugin.build(self);
        self
    }

    pub fn add_plugins<G>(mut self, plugins: G) -> Self
    where
        G: PluginGroup<T>,
    {
        plugins.build(&mut self);
        self
    }

    pub fn run(self) {
        let runner = AppRunner::<T>::with_systems(self.systems);
        runner.run();
    }
}

pub struct AppRunner<T: App> {
    app: Option<T>,
    window: Option<Window>,
    systems: RunnerSystems,
    startup_ran: bool,
    window_focused: bool,
    synced_cursor_grabbed: Option<bool>,
}

impl<T: App> Default for AppRunner<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: App> AppRunner<T> {
    pub fn new() -> Self {
        Self::with_systems(RunnerSystems::default())
    }

    fn with_systems(systems: RunnerSystems) -> Self {
        Self {
            app: None,
            window: None,
            systems,
            startup_ran: false,
            window_focused: true,
            synced_cursor_grabbed: None,
        }
    }

    pub fn run(mut self) {
        let event_loop = EventLoop::new().expect("Failed to create event loop");
        event_loop
            .run_app(&mut self)
            .expect("Failed to run event loop");
    }

    fn run_startup_systems(&mut self) {
        if self.startup_ran {
            return;
        }

        if let (Some(app), Some(window)) = (self.app.as_mut(), self.window.as_ref()) {
            if !app.world().contains_resource::<Window>() {
                app.world_mut().insert_resource(window.clone());
            }
            if !app.world().contains_resource::<AppExit>() {
                app.world_mut().insert_resource(AppExit::default());
            }
            for startup in &self.systems.startup {
                startup(app.world_mut(), window);
            }
            RunnerSystems::run(&mut self.systems.startup_schedule, app.world_mut());
            self.startup_ran = true;
        }
    }

    fn exit_requested(&self) -> bool {
        self.app
            .as_ref()
            .and_then(|app| app.world().get_resource::<AppExit>())
            .map(AppExit::is_requested)
            .unwrap_or(false)
    }
}

impl<T: App> ApplicationHandler for AppRunner<T> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window = Window::new(event_loop, "Oxide Core", 1280, 720);
            let renderer = pollster::block_on(create_renderer(&window));
            let app = T::init(&window, renderer);

            self.app = Some(app);
            self.window = Some(window);
            self.run_startup_systems();
            if self.exit_requested() {
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let mut ui_consumed = false;
        let mut ui_blocks_game_input = false;

        if let (Some(app), Some(window)) = (self.app.as_mut(), self.window.as_ref()) {
            if let Some((consumed, blocks_game_input)) =
                handle_engine_egui_event(app.world_mut(), window, &event)
            {
                ui_consumed = consumed;
                ui_blocks_game_input = blocks_game_input;
            } else if let Some(egui_manager) = app.egui_manager_mut() {
                ui_consumed = handle_egui_event(egui_manager, window.winit_window(), &event);
                ui_blocks_game_input =
                    egui_manager.wants_pointer_input() || egui_manager.wants_keyboard_input();
            }
        }

        if authoring_ui_toggle_requested(&event) {
            if let Some(app) = self.app.as_mut() {
                toggle_authoring_ui(app.world_mut());
            }
            return;
        }

        if ui_consumed {
            return;
        }

        if let (Some(app), Some(window)) = (self.app.as_mut(), self.window.as_ref()) {
            if let Some(engine_event) = window_event_to_engine(&event) {
                if let EngineEvent::Resized { width, height } = &engine_event {
                    resize_engine_render_resources(app.world_mut(), *width, *height);
                }
                if let EngineEvent::Focused(focused) = &engine_event {
                    self.window_focused = *focused;
                    sync_cursor_capture(
                        app.world_mut(),
                        window,
                        *focused,
                        true,
                        &mut self.synced_cursor_grabbed,
                    );
                }
                app.on_event(engine_event);
            }
        }
        if self.exit_requested() {
            event_loop.exit();
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                if let Some(app) = self.app.as_mut() {
                    {
                        let time = app.world_mut().resource_mut::<Time>();
                        time.update();
                    }

                    RunnerSystems::run(&mut self.systems.pre_update, app.world_mut());
                    let fixed_steps = if app.world().contains_resource::<FixedTime>() {
                        let delta = app.world().resource::<Time>().delta;
                        app.world_mut().resource_mut::<FixedTime>().advance(delta)
                    } else {
                        0
                    };
                    for _ in 0..fixed_steps {
                        RunnerSystems::run(&mut self.systems.fixed_update, app.world_mut());
                    }
                    app.update();
                    RunnerSystems::run(&mut self.systems.update, app.world_mut());
                    RunnerSystems::run(&mut self.systems.post_update, app.world_mut());
                    if app
                        .world()
                        .get_resource::<AppExit>()
                        .map(AppExit::is_requested)
                        .unwrap_or(false)
                    {
                        event_loop.exit();
                        return;
                    }
                    if let Some(window) = self.window.as_ref() {
                        sync_cursor_capture(
                            app.world_mut(),
                            window,
                            self.window_focused,
                            false,
                            &mut self.synced_cursor_grabbed,
                        );

                        if let Some(ctx) = begin_engine_egui_frame(app.world_mut(), window) {
                            if authoring_ui_visible(app.world()) {
                                crate::scene::show_scene_authoring_egui(app.world_mut(), &ctx);
                            }
                        }
                    }

                    app.extract();
                    RunnerSystems::run(&mut self.systems.extract, app.world_mut());

                    app.prepare();
                    RunnerSystems::run(&mut self.systems.prepare, app.world_mut());
                    prepare_scene_renderer(app.world_mut());
                    prepare_game_text_renderer(app.world_mut());

                    let frame_parts = {
                        let renderer = &app
                            .world()
                            .resource::<crate::ecs::RendererResource>()
                            .renderer;
                        match renderer.begin_frame() {
                            Ok(surface_texture) => Some((
                                surface_texture,
                                Arc::clone(&renderer.device),
                                Arc::clone(&renderer.queue),
                            )),
                            Err(err) => {
                                tracing::warn!("Skipping render frame: {err}");
                                None
                            }
                        }
                    };

                    if let Some((surface_texture, device, queue)) = frame_parts {
                        let mut frame = RenderFrame::new(&device, surface_texture);
                        for step in self.systems.render_passes.ordered_steps() {
                            match step {
                                RenderPassStep::Anchor(RenderPassAnchor::Scene) => {
                                    queue_scene_renderer(app.world_mut(), &mut frame);
                                }
                                RenderPassStep::Anchor(RenderPassAnchor::GameText) => {
                                    queue_game_text_renderer(app.world_mut(), &mut frame);
                                }
                                RenderPassStep::Anchor(RenderPassAnchor::AppQueue) => {
                                    app.queue(&mut frame);
                                }
                                RenderPassStep::Anchor(RenderPassAnchor::Egui) => {
                                    if let Some(window) = self.window.as_ref() {
                                        queue_engine_egui(app.world_mut(), window, &mut frame);
                                    }
                                }
                                RenderPassStep::Pass(index) => {
                                    self.systems.render_passes.run_pass_at(
                                        index,
                                        app.world_mut(),
                                        &mut frame,
                                    );
                                }
                            }
                        }
                        frame.present(&queue);
                    }

                    {
                        let keyboard = app.world_mut().resource_mut::<KeyboardInput>();
                        keyboard.update();
                    }
                    {
                        let mouse = app.world_mut().resource_mut::<MouseInput>();
                        mouse.update();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if ui_blocks_game_input {
                    return;
                }

                if let Some(app) = self.app.as_mut() {
                    let keyboard = app.world_mut().resource_mut::<KeyboardInput>();
                    let pressed = event.state == ElementState::Pressed;
                    keyboard.process_event(event.physical_key, pressed);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if ui_blocks_game_input {
                    return;
                }

                if let Some(app) = self.app.as_mut() {
                    let mouse = app.world_mut().resource_mut::<MouseInput>();
                    let pressed = state == ElementState::Pressed;
                    mouse.process_button(button.into(), pressed);
                }
            }
            WindowEvent::CursorEntered { .. } => {
                if ui_blocks_game_input {
                    return;
                }

                if let (Some(app), Some(window)) = (self.app.as_mut(), self.window.as_ref()) {
                    let cursor_grabbed = app.world().contains_resource::<MouseInput>()
                        && app.world().resource::<MouseInput>().cursor_grabbed();
                    if !cursor_grabbed {
                        return;
                    }

                    let size = window.size();
                    let center =
                        PhysicalPosition::new(size.width as f64 * 0.5, size.height as f64 * 0.5);

                    if let Err(err) = window.set_cursor_position(center) {
                        tracing::warn!("Failed to recenter cursor: {err}");
                    }

                    let mouse = app.world_mut().resource_mut::<MouseInput>();
                    mouse.set_position(center);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if ui_blocks_game_input {
                    return;
                }

                if let Some(app) = self.app.as_mut() {
                    let mouse = app.world_mut().resource_mut::<MouseInput>();
                    if !mouse.cursor_grabbed() {
                        mouse.process_move(position);
                    }
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if !self.window_focused {
            return;
        }

        let DeviceEvent::MouseMotion { delta } = event else {
            return;
        };

        let Some(app) = self.app.as_mut() else {
            return;
        };
        if !app.world().contains_resource::<MouseInput>() {
            return;
        }
        if !app.world().resource::<MouseInput>().cursor_grabbed() {
            return;
        }

        app.world_mut()
            .resource_mut::<MouseInput>()
            .process_delta(delta.0, delta.1);
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn authoring_ui_toggle_requested(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput { event, .. }
            if event.state == ElementState::Pressed
                && matches!(event.physical_key, PhysicalKey::Code(KeyCode::F1))
    )
}

pub fn app<T: App>() -> AppBuilder<T> {
    AppBuilder::new()
}

pub fn run_app<T: App>() {
    AppBuilder::<T>::new().run();
}

pub async fn create_renderer(window: &Window) -> Renderer {
    Renderer::new(window.winit_window().clone())
        .await
        .expect("Failed to create renderer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::ResMut;

    #[derive(oxide_ecs::Resource, Default)]
    struct StartupCounter(u32);

    struct TestApp {
        world: World,
    }

    impl App for TestApp {
        fn configure(_world: &mut World) {}

        fn init(_window: &Window, _renderer: Renderer) -> Self {
            Self {
                world: World::new(),
            }
        }

        fn world(&self) -> &World {
            &self.world
        }

        fn world_mut(&mut self) -> &mut World {
            &mut self.world
        }

        fn update(&mut self) {}

        fn on_event(&mut self, _event: EngineEvent) {}
    }

    fn increment_startup_counter(mut counter: ResMut<StartupCounter>) {
        counter.0 += 1;
    }

    fn noop_render_pass(_world: &mut World, _frame: &mut RenderFrame) {}

    #[test]
    fn app_stage_startup_accepts_normal_system_params() {
        let mut builder = AppBuilder::<TestApp>::new();
        builder.add_system_mut(AppStage::Startup, increment_startup_counter);

        let mut world = World::new();
        world.insert_resource(StartupCounter::default());

        RunnerSystems::run(&mut builder.systems.startup_schedule, &mut world);

        assert_eq!(world.resource::<StartupCounter>().0, 1);
    }

    #[test]
    fn startup_stage_supports_system_ordering() {
        fn first(mut counter: ResMut<StartupCounter>) {
            counter.0 = counter.0 * 10 + 1;
        }

        fn second(mut counter: ResMut<StartupCounter>) {
            counter.0 = counter.0 * 10 + 2;
        }

        let mut builder = AppBuilder::<TestApp>::new();
        builder.add_labeled_system_mut(AppStage::Startup, "second", second);
        builder.add_labeled_system_before_mut(AppStage::Startup, "first", "second", first);

        let mut world = World::new();
        world.insert_resource(StartupCounter::default());

        RunnerSystems::run(&mut builder.systems.startup_schedule, &mut world);

        assert_eq!(world.resource::<StartupCounter>().0, 12);
    }

    struct UniqueCounterPlugin;

    impl Plugin<TestApp> for UniqueCounterPlugin {
        fn name(&self) -> &'static str {
            "test.unique_counter"
        }

        fn build(&self, app: &mut AppBuilder<TestApp>) {
            app.add_system_mut(AppStage::Startup, increment_startup_counter);
        }
    }

    struct RepeatableCounterPlugin;

    impl Plugin<TestApp> for RepeatableCounterPlugin {
        fn name(&self) -> &'static str {
            "test.repeatable_counter"
        }

        fn is_unique(&self) -> bool {
            false
        }

        fn build(&self, app: &mut AppBuilder<TestApp>) {
            app.add_system_mut(AppStage::Startup, increment_startup_counter);
        }
    }

    #[test]
    fn unique_plugins_are_registered_once() {
        let mut builder = AppBuilder::<TestApp>::new();
        builder.add_plugin_mut(UniqueCounterPlugin);
        builder.add_plugin_mut(UniqueCounterPlugin);

        assert_eq!(
            builder.plugins(),
            &[PluginRegistration {
                name: "test.unique_counter",
                unique: true,
            }]
        );
        assert!(builder.has_plugin("test.unique_counter"));

        let mut world = World::new();
        world.insert_resource(StartupCounter::default());

        RunnerSystems::run(&mut builder.systems.startup_schedule, &mut world);

        assert_eq!(world.resource::<StartupCounter>().0, 1);
    }

    #[test]
    fn non_unique_plugins_can_be_registered_multiple_times() {
        let mut builder = AppBuilder::<TestApp>::new();
        builder.add_plugin_mut(RepeatableCounterPlugin);
        builder.add_plugin_mut(RepeatableCounterPlugin);

        assert_eq!(builder.plugins().len(), 2);
        assert!(builder
            .plugins()
            .iter()
            .all(|plugin| plugin.name == "test.repeatable_counter" && !plugin.unique));

        let mut world = World::new();
        world.insert_resource(StartupCounter::default());

        RunnerSystems::run(&mut builder.systems.startup_schedule, &mut world);

        assert_eq!(world.resource::<StartupCounter>().0, 2);
    }

    #[test]
    fn app_builder_can_toggle_render_pass_sets() {
        let mut builder = AppBuilder::<TestApp>::new();
        builder.add_render_pass_to_set_mut("debug.lines", "debug", noop_render_pass);
        builder.add_render_pass_to_set_mut("debug.bounds", "debug", noop_render_pass);

        builder.disable_render_pass_set_mut("debug");
        let disabled = builder
            .render_pass_schedule()
            .pass_infos()
            .into_iter()
            .filter(|info| info.sets.iter().any(|set| set == "debug"))
            .collect::<Vec<_>>();
        assert_eq!(disabled.len(), 2);
        assert!(disabled.iter().all(|info| !info.enabled));

        builder.enable_render_pass_set_mut("debug");
        assert!(builder
            .render_pass_schedule()
            .pass_infos()
            .into_iter()
            .filter(|info| info.sets.iter().any(|set| set == "debug"))
            .all(|info| info.enabled));
    }

    #[test]
    fn app_runner_detects_system_requested_exit() {
        let mut runner = AppRunner::<TestApp>::with_systems(RunnerSystems::default());
        runner.app = Some(TestApp {
            world: World::new(),
        });
        assert!(!runner.exit_requested());

        let app = runner.app.as_mut().unwrap();
        app.world_mut().insert_resource(AppExit::default());
        assert!(!runner.exit_requested());

        runner
            .app
            .as_mut()
            .unwrap()
            .world_mut()
            .resource_mut::<AppExit>()
            .request_with_code(2);
        assert!(runner.exit_requested());
    }
}
