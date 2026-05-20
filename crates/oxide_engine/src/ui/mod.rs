//! Compatibility facade and engine plugin wiring for UI APIs.

use crate::app::{App, AppBuilder, AppStage, Plugin};
use crate::ecs::{RendererResource, WindowResource, World};
use crate::render::RenderFrame;
use crate::window::Window;
use oxide_ecs::Resource;

pub use oxide_ui::{
    handle_egui_event, load_game_font, register_game_font_bytes, DevOverlay, DevOverlaySnapshot,
    EguiManager, EguiRender, EguiWgpuPass, GameFont, GameFontError, GameFontId, GameFonts,
    GameTextRenderer, GameTextStyle, GameUi, GameUiAnchor, GameUiBar, GameUiButton, GameUiCounter,
    GameUiRect, GameUiReticle, GameUiText, GameUiWidget, RuntimeUi, TextHorizontalAlign,
    TextVerticalAlign, UiElement, BUILTIN_GAME_FONT,
};

pub struct EguiPlugin;

impl<T: App> Plugin<T> for EguiPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_authoring_ui);
        app.add_startup_system_mut(initialize_egui_pass);
    }
}

#[derive(Clone, Copy, Debug, Default, Resource)]
pub struct AuthoringUi {
    pub visible: bool,
}

impl AuthoringUi {
    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }
}

pub fn initialize_authoring_ui(world: &mut World, _window: &Window) {
    if !world.contains_resource::<AuthoringUi>() {
        world.insert_resource(AuthoringUi::default());
    }
}

pub fn authoring_ui_visible(world: &World) -> bool {
    world
        .get_resource::<AuthoringUi>()
        .map(|authoring_ui| authoring_ui.visible)
        .unwrap_or(false)
}

pub fn toggle_authoring_ui(world: &mut World) {
    if !world.contains_resource::<AuthoringUi>() {
        world.insert_resource(AuthoringUi::default());
    }
    world.resource_mut::<AuthoringUi>().toggle();
}

pub fn initialize_egui_pass(world: &mut World, window: &Window) {
    install_egui_pass(world, window);
}

pub fn install_egui_pass(world: &mut World, window: &Window) {
    if world.get_non_send_resource::<EguiWgpuPass>().is_some()
        || !world.contains_resource::<RendererResource>()
    {
        return;
    }

    let (device, format) = {
        let renderer = &world.resource::<RendererResource>().renderer;
        (renderer.device.clone(), renderer.format())
    };
    world.insert_non_send_resource(EguiWgpuPass::new(
        &device,
        format,
        window.winit_window().as_ref(),
        window.scale_factor() as f32,
    ));
}

pub fn handle_engine_egui_event(
    world: &mut World,
    window: &Window,
    event: &winit::event::WindowEvent,
) -> Option<(bool, bool)> {
    let egui_pass = world.get_non_send_resource_mut::<EguiWgpuPass>()?;
    let consumed = egui_pass.handle_event(window.winit_window().as_ref(), event);
    let blocks_game_input = egui_pass.wants_pointer_input() || egui_pass.wants_keyboard_input();
    Some((consumed, blocks_game_input))
}

pub fn begin_engine_egui_frame(world: &mut World, window: &Window) -> Option<egui::Context> {
    let egui_pass = world.get_non_send_resource_mut::<EguiWgpuPass>()?;
    egui_pass.begin_frame(window.winit_window().as_ref());
    Some(egui_pass.context().clone())
}

pub fn queue_engine_egui(world: &mut World, window: &Window, frame: &mut RenderFrame) {
    let (device, queue) = {
        let renderer = &world.resource::<RendererResource>().renderer;
        (renderer.device.clone(), renderer.queue.clone())
    };

    let Some(egui_pass) = world.get_non_send_resource_mut::<EguiWgpuPass>() else {
        return;
    };
    egui_pass.queue(
        window.winit_window().as_ref(),
        &device,
        &queue,
        &frame.view,
        &mut frame.encoder,
    );
}

pub struct RuntimeUiPlugin;

impl<T: App> Plugin<T> for RuntimeUiPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_runtime_ui);
    }
}

pub fn initialize_runtime_ui(world: &mut World, _window: &Window) {
    oxide_ui::initialize_runtime_ui(world);
}

pub struct DevOverlayPlugin;

impl<T: App> Plugin<T> for DevOverlayPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_dev_overlay);
    }
}

pub fn initialize_dev_overlay(world: &mut World, _window: &Window) {
    oxide_ui::initialize_dev_overlay(world);
}

pub struct GameTextRendererPlugin;

impl<T: App> Plugin<T> for GameTextRendererPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_game_fonts);
        app.add_startup_system_mut(initialize_game_text_renderer);
    }
}

pub fn initialize_game_fonts(world: &mut World, _window: &Window) {
    oxide_ui::initialize_game_fonts(world);
}

pub fn initialize_game_text_renderer(world: &mut World, _window: &Window) {
    install_game_text_renderer(world);
}

pub fn install_game_text_renderer(world: &mut World) {
    if world.get_non_send_resource::<GameTextRenderer>().is_some()
        || !world.contains_resource::<RendererResource>()
    {
        return;
    }

    let (device, queue, format) = {
        let renderer = &world.resource::<RendererResource>().renderer;
        (
            renderer.device.clone(),
            renderer.queue.clone(),
            renderer.format(),
        )
    };
    world.insert_non_send_resource(GameTextRenderer::new(&device, &queue, format));
}

pub fn prepare_game_text_renderer(world: &mut World) {
    let Some(mut text_renderer) = world.remove_non_send_resource::<GameTextRenderer>() else {
        return;
    };

    if world.contains_resource::<RendererResource>() {
        let (device, queue, viewport_size) = {
            let renderer = &world.resource::<RendererResource>().renderer;
            let viewport_size = if world.contains_resource::<WindowResource>() {
                let window = world.resource::<WindowResource>();
                (window.width as f32, window.height as f32)
            } else {
                (renderer.width() as f32, renderer.height() as f32)
            };
            (
                renderer.device.clone(),
                renderer.queue.clone(),
                viewport_size,
            )
        };
        text_renderer.prepare(&device, &queue, world, viewport_size);
    }

    world.insert_non_send_resource(text_renderer);
}

pub fn queue_game_text_renderer(world: &mut World, frame: &mut RenderFrame) {
    let Some(text_renderer) = world.get_non_send_resource::<GameTextRenderer>() else {
        return;
    };
    text_renderer.queue(&frame.view, &mut frame.encoder);
}

pub struct GameUiPlugin;

impl<T: App> Plugin<T> for GameUiPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_game_fonts);
        app.add_startup_system_mut(initialize_game_ui);
        app.add_startup_system_mut(initialize_game_text_renderer);
        app.add_system_mut(AppStage::Update, game_ui_sync_system);
    }
}

pub fn initialize_game_ui(world: &mut World, _window: &Window) {
    oxide_ui::initialize_game_ui(world);
}

pub fn game_ui_sync_system(world: &mut World) {
    let aspect_ratio = if world.contains_resource::<WindowResource>() {
        world.resource::<WindowResource>().aspect_ratio()
    } else {
        16.0 / 9.0
    };
    oxide_ui::game_ui_sync_system(world, aspect_ratio);
}
