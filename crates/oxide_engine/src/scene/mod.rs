//! Compatibility facade and engine plugin wiring for scene/editor APIs.

#[cfg(feature = "gltf-import")]
mod gltf_hierarchy;
mod oxscene;

use crate::app::{App, AppBuilder, AppStage, Plugin, PluginGroup};
use crate::diagnostics::{Diagnostics, FPS, FRAME_TIME_MS};
use crate::ecs::{RendererResource, WindowResource, World};
use crate::render::RenderFrame;
use crate::ui::{
    authoring_ui_visible, DevOverlay, DevOverlayPlugin, DevOverlaySnapshot, EguiPlugin,
    GameUiPlugin, RuntimeUi, RuntimeUiPlugin,
};
use crate::window::Window;

pub use oxide_editor::{
    apply_gizmo_drag, pick_render_mesh, show_scene_editor_egui, viewport_pick_ray,
    with_scene_editor, GizmoAxis, SceneEditor, SceneEditorTool, SceneEntitySummary, ScenePickHit,
    ScenePickRay,
};
pub use oxide_scene::*;

#[cfg(feature = "gltf-import")]
pub use gltf_hierarchy::*;
pub use oxscene::*;

pub struct SceneRendererPlugin;

impl<T: App> Plugin<T> for SceneRendererPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_scene_renderer);
    }
}

pub fn initialize_scene_renderer(world: &mut World, _window: &Window) {
    install_scene_renderer(world);
}

pub fn install_scene_renderer(world: &mut World) {
    if world.get_non_send_resource::<SceneRenderer>().is_some()
        || !world.contains_resource::<RendererResource>()
    {
        return;
    }

    let (device, format, width, height) = {
        let renderer = &world.resource::<RendererResource>().renderer;
        (
            renderer.device.clone(),
            renderer.format(),
            renderer.width(),
            renderer.height(),
        )
    };

    world.insert_non_send_resource(SceneRenderer::new(&device, format, width, height));
}

pub fn prepare_scene_renderer(world: &mut World) {
    let Some(mut scene_renderer) = world.remove_non_send_resource::<SceneRenderer>() else {
        return;
    };

    if world.contains_resource::<RendererResource>() {
        let (device, queue, aspect_ratio) = {
            let renderer = &world.resource::<RendererResource>().renderer;
            let aspect_ratio = if world.contains_resource::<WindowResource>() {
                world.resource::<WindowResource>().aspect_ratio()
            } else {
                1.0
            };
            (
                renderer.device.clone(),
                renderer.queue.clone(),
                aspect_ratio,
            )
        };
        scene_renderer.prepare(&device, &queue, world, aspect_ratio);
    }

    world.insert_non_send_resource(scene_renderer);
}

pub fn queue_scene_renderer(world: &mut World, frame: &mut RenderFrame) {
    let Some(mut scene_renderer) = world.remove_non_send_resource::<SceneRenderer>() else {
        return;
    };

    scene_renderer.queue(&frame.view, &mut frame.encoder);
    world.insert_non_send_resource(scene_renderer);
}

pub fn resize_scene_renderer(world: &mut World, width: u32, height: u32) {
    if width == 0 || height == 0 {
        return;
    }

    let Some(mut scene_renderer) = world.remove_non_send_resource::<SceneRenderer>() else {
        return;
    };

    if world.contains_resource::<RendererResource>() {
        let device = world.resource::<RendererResource>().renderer.device.clone();
        scene_renderer.resize(&device, width, height);
    }

    world.insert_non_send_resource(scene_renderer);
}

pub struct SceneEditorPlugin;

impl<T: App> Plugin<T> for SceneEditorPlugin {
    fn build(&self, app: &mut AppBuilder<T>) {
        app.add_startup_system_mut(initialize_scene_editor);
        app.add_system_mut(AppStage::Update, scene_editor_viewport_system);
    }
}

pub fn initialize_scene_editor(world: &mut World, _window: &Window) {
    oxide_editor::initialize_scene_editor(world);
}

pub fn scene_editor_viewport_system(world: &mut World) {
    if !authoring_ui_visible(world) {
        return;
    }

    let viewport_size = if world.contains_resource::<WindowResource>() {
        let window = world.resource::<WindowResource>();
        [window.width as f32, window.height as f32]
    } else if world.contains_resource::<RendererResource>() {
        let renderer = &world.resource::<RendererResource>().renderer;
        [renderer.width() as f32, renderer.height() as f32]
    } else {
        [1280.0, 720.0]
    };

    oxide_editor::scene_editor_viewport_system(world, viewport_size);
}

pub fn show_scene_authoring_egui(world: &mut World, ctx: &egui::Context) {
    show_scene_editor_egui(world, ctx);

    if let Some(mut runtime_ui) = world.remove_resource::<RuntimeUi>() {
        runtime_ui.show_egui(ctx);
        world.insert_resource(runtime_ui);
    }

    if world.contains_resource::<DevOverlay>() {
        let overlay = world.resource::<DevOverlay>().clone();
        let mut snapshot = DevOverlaySnapshot::from_world(world);
        if let Some(diagnostics) = world.get_resource::<Diagnostics>() {
            if let Some(frame_ms) = diagnostics.latest(FRAME_TIME_MS) {
                snapshot.frame_ms = frame_ms as f32;
            }
            if let Some(fps) = diagnostics.latest(FPS) {
                snapshot.fps = fps as f32;
            }
        }
        overlay.show_egui(ctx, snapshot);
    }
}

/// Convenience plugin group for code-first game authoring.
pub struct SceneAuthoringPlugins;

impl<T: App> PluginGroup<T> for SceneAuthoringPlugins {
    fn build(self, app: &mut AppBuilder<T>) {
        app.add_plugin_mut(EguiPlugin);
        app.add_plugin_mut(SceneRendererPlugin);
        app.add_plugin_mut(SceneEditorPlugin);
        app.add_plugin_mut(GameUiPlugin);
        app.add_plugin_mut(RuntimeUiPlugin);
        app.add_plugin_mut(DevOverlayPlugin);
    }
}
