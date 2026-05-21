//! Developer overlay helpers for inspecting a running game.

use oxide_ecs::world::World;
use oxide_ecs::Resource;

#[derive(Resource, Clone, Debug)]
pub struct DevOverlay {
    pub visible: bool,
}

impl Default for DevOverlay {
    fn default() -> Self {
        Self { visible: true }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DevOverlaySnapshot {
    pub entity_count: usize,
    pub resource_count: usize,
    pub frame_ms: f32,
    pub fps: f32,
    pub scene_camera_views: u32,
    pub scene_renderable_candidates: u32,
    pub scene_culled_renderables: u32,
    pub scene_draw_calls: u32,
}

impl DevOverlaySnapshot {
    pub fn from_world(world: &World) -> Self {
        Self::from_world_with_frame_secs(world, 0.0)
    }

    pub fn from_world_with_frame_secs(world: &World, frame_secs: f32) -> Self {
        let frame_ms = frame_secs * 1000.0;
        let fps = if frame_ms > f32::EPSILON {
            1000.0 / frame_ms
        } else {
            0.0
        };

        Self {
            entity_count: world.entity_count(),
            resource_count: world.resource_count(),
            frame_ms,
            fps,
            scene_camera_views: 0,
            scene_renderable_candidates: 0,
            scene_culled_renderables: 0,
            scene_draw_calls: 0,
        }
    }
}

impl DevOverlay {
    pub fn show_egui(&self, ctx: &egui::Context, snapshot: DevOverlaySnapshot) {
        if !self.visible {
            return;
        }

        egui::Window::new("Oxide Debug").show(ctx, |ui| {
            ui.label(format!("Entities: {}", snapshot.entity_count));
            ui.label(format!("Resources: {}", snapshot.resource_count));
            ui.label(format!("Frame: {:.2} ms", snapshot.frame_ms));
            ui.label(format!("FPS: {:.1}", snapshot.fps));
            ui.separator();
            ui.label(format!("Scene Cameras: {}", snapshot.scene_camera_views));
            ui.label(format!(
                "Scene Renderables: {}",
                snapshot.scene_renderable_candidates
            ));
            ui.label(format!(
                "Scene Culled: {}",
                snapshot.scene_culled_renderables
            ));
            ui.label(format!("Scene Draw Calls: {}", snapshot.scene_draw_calls));
        });
    }
}

pub fn initialize_dev_overlay(world: &mut World) {
    if !world.contains_resource::<DevOverlay>() {
        world.insert_resource(DevOverlay::default());
    }
}
