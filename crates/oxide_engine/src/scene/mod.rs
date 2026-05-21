//! Compatibility facade and engine plugin wiring for scene/editor APIs.

#[cfg(feature = "gltf-import")]
mod gltf_hierarchy;
mod oxscene;

use crate::app::{App, AppBuilder, AppStage, Plugin, PluginGroup};
use crate::diagnostics::{
    Diagnostics, FPS, FRAME_TIME_MS, SCENE_CAMERA_VIEWS, SCENE_CUBE_INSTANCES,
    SCENE_CULLED_RENDERABLES, SCENE_DRAW_CALLS, SCENE_MESH_HANDLE_INSTANCES,
    SCENE_RENDERABLE_CANDIDATES, SCENE_SPHERE_INSTANCES, SCENE_SPRITE_INSTANCES,
    SCENE_TERRAIN_INSTANCES,
};
use crate::ecs::{RendererResource, WindowResource, World};
use crate::render::RenderFrame;
use crate::ui::{
    authoring_ui_visible, DevOverlay, DevOverlayPlugin, DevOverlaySnapshot, EguiPlugin,
    GameUiPlugin, RuntimeUi, RuntimeUiPlugin,
};
use crate::window::Window;

pub use oxide_editor::{
    apply_gizmo_drag, pick_render_mesh, show_scene_editor_egui, with_scene_editor, GizmoAxis,
    SceneEditor, SceneEditorTool, SceneEntitySummary,
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

    let (device, queue, format, width, height) = {
        let renderer = &world.resource::<RendererResource>().renderer;
        (
            renderer.device.clone(),
            renderer.queue.clone(),
            renderer.format(),
            renderer.width(),
            renderer.height(),
        )
    };

    world.insert_non_send_resource(SceneRenderer::new(&device, &queue, format, width, height));
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
        if let Some(diagnostics) = world.get_resource_mut::<Diagnostics>() {
            record_scene_renderer_stats(diagnostics, scene_renderer.stats());
        }
    }

    world.insert_non_send_resource(scene_renderer);
}

pub fn record_scene_renderer_stats(diagnostics: &mut Diagnostics, stats: SceneRendererStats) {
    diagnostics.record(SCENE_CAMERA_VIEWS, stats.camera_views as f64);
    diagnostics.record(
        SCENE_RENDERABLE_CANDIDATES,
        stats.renderable_candidates as f64,
    );
    diagnostics.record(SCENE_CULLED_RENDERABLES, stats.culled_renderables as f64);
    diagnostics.record(SCENE_CUBE_INSTANCES, stats.cube_instances as f64);
    diagnostics.record(SCENE_SPHERE_INSTANCES, stats.sphere_instances as f64);
    diagnostics.record(
        SCENE_MESH_HANDLE_INSTANCES,
        stats.mesh_handle_instances as f64,
    );
    diagnostics.record(SCENE_TERRAIN_INSTANCES, stats.terrain_instances as f64);
    diagnostics.record(SCENE_SPRITE_INSTANCES, stats.sprite_instances as f64);
    diagnostics.record(SCENE_DRAW_CALLS, stats.draw_calls as f64);
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
            if let Some(camera_views) = diagnostics.latest(SCENE_CAMERA_VIEWS) {
                snapshot.scene_camera_views = camera_views as u32;
            }
            if let Some(candidates) = diagnostics.latest(SCENE_RENDERABLE_CANDIDATES) {
                snapshot.scene_renderable_candidates = candidates as u32;
            }
            if let Some(culled) = diagnostics.latest(SCENE_CULLED_RENDERABLES) {
                snapshot.scene_culled_renderables = culled as u32;
            }
            if let Some(draw_calls) = diagnostics.latest(SCENE_DRAW_CALLS) {
                snapshot.scene_draw_calls = draw_calls as u32;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_scene_renderer_stats_publishes_diagnostic_labels() {
        let mut diagnostics = Diagnostics::default();
        record_scene_renderer_stats(
            &mut diagnostics,
            SceneRendererStats {
                camera_views: 2,
                renderable_candidates: 10,
                culled_renderables: 3,
                cube_instances: 4,
                sphere_instances: 5,
                mesh_handle_instances: 6,
                terrain_instances: 1,
                sprite_instances: 7,
                draw_calls: 8,
            },
        );

        assert_eq!(diagnostics.latest(SCENE_CAMERA_VIEWS), Some(2.0));
        assert_eq!(diagnostics.latest(SCENE_RENDERABLE_CANDIDATES), Some(10.0));
        assert_eq!(diagnostics.latest(SCENE_CULLED_RENDERABLES), Some(3.0));
        assert_eq!(diagnostics.latest(SCENE_CUBE_INSTANCES), Some(4.0));
        assert_eq!(diagnostics.latest(SCENE_SPHERE_INSTANCES), Some(5.0));
        assert_eq!(diagnostics.latest(SCENE_MESH_HANDLE_INSTANCES), Some(6.0));
        assert_eq!(diagnostics.latest(SCENE_TERRAIN_INSTANCES), Some(1.0));
        assert_eq!(diagnostics.latest(SCENE_SPRITE_INSTANCES), Some(7.0));
        assert_eq!(diagnostics.latest(SCENE_DRAW_CALLS), Some(8.0));
    }
}
