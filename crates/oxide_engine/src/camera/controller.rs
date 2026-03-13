//! FPS-style camera controller

use bevy_ecs::prelude::{Component, Query, Res};
use glam::Vec3;

use crate::input::{KeyboardInput, MouseInput};
use crate::ecs::WindowResource;
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
        let rotation = glam::Quat::from_rotation_y(self.yaw) * glam::Quat::from_rotation_x(self.pitch);
        let forward = rotation * Vec3::NEG_Z;

        camera.target = camera.position + forward;
        camera.up = rotation * Vec3::Y;
    }
}

pub fn camera_controller_system(
    keyboard: Res<KeyboardInput>,
    mouse: Res<MouseInput>,
    _window: Res<WindowResource>,
    mut query: Query<(&mut CameraController, &mut CameraComponent)>,
) {
    use winit::keyboard::KeyCode;

    for (mut controller, mut camera_comp) in query.iter_mut() {
        let camera = &mut camera_comp.0;
        
        let (dx, dy) = mouse.delta();
        controller.yaw -= dx * controller.sensitivity;
        controller.pitch -= dy * controller.sensitivity;
        controller.pitch = controller.pitch.clamp(-std::f32::consts::FRAC_PI_2 + 0.01, std::f32::consts::FRAC_PI_2 - 0.01);

        let rotation = glam::Quat::from_rotation_y(controller.yaw) * glam::Quat::from_rotation_x(controller.pitch);
        let forward = rotation * Vec3::NEG_Z;
        let right = rotation * Vec3::X;

        let mut velocity = Vec3::ZERO;
        if keyboard.pressed(KeyCode::KeyW) {
            velocity += forward;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            velocity -= forward;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            velocity -= right;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            velocity += right;
        }
        if keyboard.pressed(KeyCode::Space) {
            velocity += Vec3::Y;
        }
        if keyboard.pressed(KeyCode::ShiftLeft) {
            velocity -= Vec3::Y;
        }

        if velocity != Vec3::ZERO {
            camera.position += velocity.normalize() * controller.speed * 0.016;
        }

        controller.update_camera(camera);
    }
}