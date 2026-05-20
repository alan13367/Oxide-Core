//! FPS-style camera controller

use glam::Vec3;
use oxide_ecs::Component;

use oxide_ecs::world::World;
use oxide_input::{KeyboardInput, MouseInput};
use oxide_math::prelude::Camera;

#[derive(Component, Clone, Copy, Debug)]
pub struct CameraComponent(pub Camera);

impl Default for CameraComponent {
    fn default() -> Self {
        Self(Camera::new())
    }
}

impl CameraComponent {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Render-view metadata for a camera.
///
/// `CameraComponent` stores projection/view data. `CameraRenderView` describes
/// how the renderer should choose and clear that camera view. Cameras without
/// this component behave like the default render view.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CameraRenderView {
    /// Lower values render first and win primary-camera selection.
    pub order: i32,
    /// Disabled views are ignored by renderers.
    pub is_active: bool,
    /// Optional per-view clear color. `None` uses the renderer default.
    pub clear_color: Option<[f64; 4]>,
}

impl Default for CameraRenderView {
    fn default() -> Self {
        Self {
            order: 0,
            is_active: true,
            clear_color: None,
        }
    }
}

impl CameraRenderView {
    /// Creates an active render view with default ordering.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an active render view with a specific ordering priority.
    pub fn with_order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    /// Creates a disabled render view that renderers ignore.
    pub fn disabled() -> Self {
        Self {
            is_active: false,
            ..Default::default()
        }
    }

    /// Sets a per-view clear color.
    pub fn with_clear_color(mut self, clear_color: [f64; 4]) -> Self {
        self.clear_color = Some(clear_color);
        self
    }
}

#[derive(Component)]
pub struct CameraController {
    pub speed: f32,
    pub sensitivity: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            speed: 5.0,
            sensitivity: 0.002,
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

impl CameraController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    pub fn with_sensitivity(mut self, sensitivity: f32) -> Self {
        self.sensitivity = sensitivity;
        self
    }

    pub fn update_camera(&self, camera: &mut Camera) {
        let rotation =
            glam::Quat::from_rotation_y(self.yaw) * glam::Quat::from_rotation_x(self.pitch);
        let forward = rotation * Vec3::NEG_Z;

        camera.target = camera.position + forward;
        camera.up = rotation * Vec3::Y;
    }
}

pub fn camera_controller_system(world: &mut World) {
    use winit::keyboard::KeyCode;

    let (w, s, a, d, space, shift, dx, dy) = {
        let keyboard = world.resource::<KeyboardInput>();
        let mouse = world.resource::<MouseInput>();

        let (dx, dy) = mouse.delta();

        (
            keyboard.pressed(KeyCode::KeyW),
            keyboard.pressed(KeyCode::KeyS),
            keyboard.pressed(KeyCode::KeyA),
            keyboard.pressed(KeyCode::KeyD),
            keyboard.pressed(KeyCode::Space),
            keyboard.pressed(KeyCode::ShiftLeft),
            dx,
            dy,
        )
    };

    let mut query = world.query::<(&mut CameraController, &mut CameraComponent)>();
    for (controller, camera_comp) in query.iter_mut(world) {
        let camera = &mut camera_comp.0;

        controller.yaw -= dx * controller.sensitivity;
        controller.pitch -= dy * controller.sensitivity;
        controller.pitch = controller.pitch.clamp(
            -std::f32::consts::FRAC_PI_2 + 0.01,
            std::f32::consts::FRAC_PI_2 - 0.01,
        );

        let rotation = glam::Quat::from_rotation_y(controller.yaw)
            * glam::Quat::from_rotation_x(controller.pitch);
        let forward = rotation * Vec3::NEG_Z;
        let right = rotation * Vec3::X;

        let mut velocity = Vec3::ZERO;
        if w {
            velocity += forward;
        }
        if s {
            velocity -= forward;
        }
        if a {
            velocity -= right;
        }
        if d {
            velocity += right;
        }
        if space {
            velocity += Vec3::Y;
        }
        if shift {
            velocity -= Vec3::Y;
        }

        if velocity != Vec3::ZERO {
            camera.position += velocity.normalize() * controller.speed * 0.016;
        }

        controller.update_camera(camera);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_render_view_builders_set_metadata() {
        let view = CameraRenderView::new()
            .with_order(-3)
            .with_clear_color([0.1, 0.2, 0.3, 1.0]);

        assert_eq!(view.order, -3);
        assert!(view.is_active);
        assert_eq!(view.clear_color, Some([0.1, 0.2, 0.3, 1.0]));
        assert!(!CameraRenderView::disabled().is_active);
    }
}
