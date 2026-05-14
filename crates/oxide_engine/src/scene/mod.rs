//! Compatibility facade and engine plugin wiring for scene/editor APIs.

#[cfg(feature = "gltf-import")]
mod gltf_hierarchy;

use crate::app::{App, AppBuilder, Plugin, PluginGroup};
use crate::ecs::{RendererResource, WindowResource, World};
use crate::render::RenderFrame;
use crate::ui::{DevOverlayPlugin, GameUiPlugin, RuntimeUiPlugin};
use crate::window::Window;

pub use oxide_editor::{
    show_scene_authoring_egui, show_scene_editor_egui, with_scene_editor, SceneEditor,
    SceneEntitySummary,
};
pub use oxide_scene::*;

#[cfg(feature = "gltf-import")]
pub use gltf_hierarchy::*;

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
    }
}

pub fn initialize_scene_editor(world: &mut World, _window: &Window) {
    oxide_editor::initialize_scene_editor(world);
}

/// Convenience plugin group for code-first game authoring.
pub struct SceneAuthoringPlugins;

impl<T: App> PluginGroup<T> for SceneAuthoringPlugins {
    fn build(self, app: &mut AppBuilder<T>) {
        app.add_plugin_mut(SceneRendererPlugin);
        app.add_plugin_mut(SceneEditorPlugin);
        app.add_plugin_mut(GameUiPlugin);
        app.add_plugin_mut(RuntimeUiPlugin);
        app.add_plugin_mut(DevOverlayPlugin);
    }
}
