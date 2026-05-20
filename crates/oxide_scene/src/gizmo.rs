//! Scene-space editor gizmo line overlay data.

use glam::{Mat4, Vec3};
use oxide_ecs::Resource;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneGizmoLine {
    pub start: Vec3,
    pub end: Vec3,
    pub color: Vec3,
}

#[derive(Resource, Default)]
pub struct SceneGizmoLines {
    lines: Vec<SceneGizmoLine>,
}

impl SceneGizmoLines {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn draw_line(&mut self, start: Vec3, end: Vec3, color: Vec3) {
        self.lines.push(SceneGizmoLine { start, end, color });
    }

    pub fn draw_axes(&mut self, transform: Mat4, size: f32) {
        let origin = transform.transform_point3(Vec3::ZERO);
        self.draw_line(
            origin,
            origin + transform.transform_vector3(Vec3::X * size),
            Vec3::new(1.0, 0.15, 0.12),
        );
        self.draw_line(
            origin,
            origin + transform.transform_vector3(Vec3::Y * size),
            Vec3::new(0.2, 0.9, 0.2),
        );
        self.draw_line(
            origin,
            origin + transform.transform_vector3(Vec3::Z * size),
            Vec3::new(0.2, 0.45, 1.0),
        );
    }

    pub fn lines(&self) -> &[SceneGizmoLine] {
        &self.lines
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}
