//! Mouse input handling

use oxide_ecs::Resource;
use winit::dpi::PhysicalPosition;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

impl From<winit::event::MouseButton> for MouseButton {
    fn from(button: winit::event::MouseButton) -> Self {
        match button {
            winit::event::MouseButton::Left => MouseButton::Left,
            winit::event::MouseButton::Right => MouseButton::Right,
            winit::event::MouseButton::Middle => MouseButton::Middle,
            winit::event::MouseButton::Other(v) => MouseButton::Other(v),
            _ => MouseButton::Other(0),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MouseDelta {
    pub x: f64,
    pub y: f64,
}

#[derive(Resource, Default)]
pub struct MouseInput {
    pub position: Option<PhysicalPosition<f64>>,
    pub delta: MouseDelta,
    pub left_pressed: bool,
    pub right_pressed: bool,
    pub middle_pressed: bool,
    cursor_grabbed: bool,
}

impl MouseInput {
    pub fn update(&mut self) {
        self.delta = MouseDelta::default();
    }

    pub fn process_move(&mut self, position: PhysicalPosition<f64>) {
        if let Some(old) = self.position {
            self.delta.x += position.x - old.x;
            self.delta.y += position.y - old.y;
        }
        self.position = Some(position);
    }

    pub fn process_button(&mut self, button: MouseButton, pressed: bool) {
        match button {
            MouseButton::Left => self.left_pressed = pressed,
            MouseButton::Right => self.right_pressed = pressed,
            MouseButton::Middle => self.middle_pressed = pressed,
            _ => {}
        }
    }

    pub fn set_position(&mut self, position: PhysicalPosition<f64>) {
        self.position = Some(position);
        self.delta = MouseDelta::default();
    }

    pub fn delta(&self) -> (f32, f32) {
        (self.delta.x as f32, self.delta.y as f32)
    }

    pub fn set_cursor_grabbed(&mut self, grabbed: bool) {
        self.cursor_grabbed = grabbed;
    }

    pub fn cursor_grabbed(&self) -> bool {
        self.cursor_grabbed
    }
}
