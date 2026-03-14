//! Application trait and engine entry points.

use std::marker::PhantomData;
use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
};

use crate::ecs::{Time, World};
use crate::event::{window_event_to_engine, EngineEvent};
use crate::input::{KeyboardInput, MouseInput};
use crate::render::RenderFrame;
use crate::ui::{handle_egui_event, EguiManager};
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

pub type SystemFn = fn(&mut World);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppStage {
    PreUpdate,
    Update,
    PostUpdate,
    Extract,
    Prepare,
}

#[derive(Default, Clone)]
struct RunnerSystems {
    pre_update: Vec<SystemFn>,
    update: Vec<SystemFn>,
    post_update: Vec<SystemFn>,
    extract: Vec<SystemFn>,
    prepare: Vec<SystemFn>,
}

impl RunnerSystems {
    fn run(stage_systems: &[SystemFn], world: &mut World) {
        for system in stage_systems {
            system(world);
        }
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

    pub fn add_system(mut self, stage: AppStage, system: SystemFn) -> Self {
        match stage {
            AppStage::PreUpdate => self.systems.pre_update.push(system),
            AppStage::Update => self.systems.update.push(system),
            AppStage::PostUpdate => self.systems.post_update.push(system),
            AppStage::Extract => self.systems.extract.push(system),
            AppStage::Prepare => self.systems.prepare.push(system),
        }
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
        }
    }

    pub fn run(mut self) {
        let event_loop = EventLoop::new().expect("Failed to create event loop");
        event_loop
            .run_app(&mut self)
            .expect("Failed to run event loop");
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
            if let Some(egui_manager) = app.egui_manager_mut() {
                ui_consumed = handle_egui_event(egui_manager, window.winit_window(), &event);
                ui_blocks_game_input =
                    egui_manager.wants_pointer_input() || egui_manager.wants_keyboard_input();
            }
        }

        if ui_consumed {
            return;
        }

        if let Some(app) = self.app.as_mut() {
            if let Some(engine_event) = window_event_to_engine(&event) {
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

                    RunnerSystems::run(&self.systems.pre_update, app.world_mut());
                    app.update();
                    RunnerSystems::run(&self.systems.update, app.world_mut());
                    RunnerSystems::run(&self.systems.post_update, app.world_mut());

                    app.extract();
                    RunnerSystems::run(&self.systems.extract, app.world_mut());

                    app.prepare();
                    RunnerSystems::run(&self.systems.prepare, app.world_mut());

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
                        app.queue(&mut frame);
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
                    mouse.process_move(position);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
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
