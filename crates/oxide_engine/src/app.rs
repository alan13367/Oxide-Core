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

#[cfg(feature = "gltf-import")]
use crate::asset::GltfSceneAssets;
use crate::asset::{AssetServerResource, MaterialAssets};
use crate::ecs::{CommandQueue, IntoSystem, RendererResource, System, Time, WindowResource, World};
use crate::event::{window_event_to_engine, EngineEvent};
use crate::input::{KeyboardInput, MouseInput};
use crate::render::RenderFrame;
#[cfg(feature = "gltf-import")]
use crate::scene::{gltf_scene_spawn_system, PendingGltfSceneSpawns, SpawnedGltfScenes};
use crate::scene::{
    oxscene_spawn_system, prepare_scene_renderer, queue_scene_renderer, resize_scene_renderer,
    transform_propagate_system, PendingOxSceneSpawns, SceneDescriptorAssets, SpawnedOxScenes,
};
use crate::ui::{
    authoring_ui_visible, begin_engine_egui_frame, handle_egui_event, handle_engine_egui_event,
    prepare_game_text_renderer, queue_engine_egui, queue_game_text_renderer, toggle_authoring_ui,
    EguiManager,
};
use crate::window::Window;
use oxide_renderer::Renderer;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PreUpdate;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Update;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PostUpdate;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Render;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppStage {
    PreUpdate,
    Update,
    PostUpdate,
    Extract,
    Prepare,
}

pub type StartupSystemFn = fn(&mut World, &Window);

#[derive(Default)]
struct RunnerSystems {
    startup: Vec<StartupSystemFn>,
    pre_update: Vec<System>,
    update: Vec<System>,
    post_update: Vec<System>,
    extract: Vec<System>,
    prepare: Vec<System>,
}

impl RunnerSystems {
    fn run(stage_systems: &mut [System], world: &mut World) {
        let mut commands = CommandQueue::new();
        for system in stage_systems {
            system.run(world, &mut commands);
        }
        commands.apply(world);
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
        app.add_system_mut(AppStage::PostUpdate, transform_propagate_system);
    }
}

pub struct RenderPlugin;

impl<T: App> Plugin<T> for RenderPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_window_resource);
        app.add_startup_system_mut(initialize_asset_resources);
        app.add_system_mut(AppStage::PreUpdate, oxscene_spawn_system);
        #[cfg(feature = "gltf-import")]
        app.add_system_mut(AppStage::PreUpdate, gltf_scene_spawn_system);
    }
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
    if !world.contains_resource::<MaterialAssets>() {
        world.insert_resource(MaterialAssets::default());
    }
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
        if !world.contains_resource::<PendingGltfSceneSpawns>() {
            world.insert_resource(PendingGltfSceneSpawns::default());
        }
        if !world.contains_resource::<SpawnedGltfScenes>() {
            world.insert_resource(SpawnedGltfScenes::default());
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
        app.add_plugin_mut(RenderPlugin);
    }
}

pub struct AppBuilder<T: App> {
    systems: RunnerSystems,
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
            _marker: PhantomData,
        }
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
        let system = system.into_system();
        match stage {
            AppStage::PreUpdate => self.systems.pre_update.push(system),
            AppStage::Update => self.systems.update.push(system),
            AppStage::PostUpdate => self.systems.post_update.push(system),
            AppStage::Extract => self.systems.extract.push(system),
            AppStage::Prepare => self.systems.prepare.push(system),
        }
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
            for startup in &self.systems.startup {
                startup(app.world_mut(), window);
            }
            self.startup_ran = true;
        }
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
                    {
                        let keyboard = app.world_mut().resource_mut::<KeyboardInput>();
                        keyboard.update();
                    }

                    RunnerSystems::run(&mut self.systems.pre_update, app.world_mut());
                    app.update();
                    RunnerSystems::run(&mut self.systems.update, app.world_mut());
                    RunnerSystems::run(&mut self.systems.post_update, app.world_mut());
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
                        queue_scene_renderer(app.world_mut(), &mut frame);
                        queue_game_text_renderer(app.world_mut(), &mut frame);
                        app.queue(&mut frame);
                        if let Some(window) = self.window.as_ref() {
                            queue_engine_egui(app.world_mut(), window, &mut frame);
                        }
                        frame.present(&queue);
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
