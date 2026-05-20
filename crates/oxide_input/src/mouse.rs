use std::collections::HashSet;

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
    pressed_buttons: HashSet<MouseButton>,
    just_pressed_buttons: HashSet<MouseButton>,
    just_released_buttons: HashSet<MouseButton>,
    cursor_grabbed: bool,
}

impl MouseInput {
    /// Clears per-frame transition state and accumulated motion.
    pub fn update(&mut self) {
        self.delta = MouseDelta::default();
        self.just_pressed_buttons.clear();
        self.just_released_buttons.clear();
    }

    pub fn process_move(&mut self, position: PhysicalPosition<f64>) {
        if let Some(old) = self.position {
            self.delta.x += position.x - old.x;
            self.delta.y += position.y - old.y;
        }
        self.position = Some(position);
    }

    pub fn process_delta(&mut self, x: f64, y: f64) {
        self.delta.x += x;
        self.delta.y += y;
    }

    pub fn process_button(&mut self, button: MouseButton, pressed: bool) {
        if pressed {
            if self.pressed_buttons.insert(button) {
                self.just_pressed_buttons.insert(button);
            }
        } else if self.pressed_buttons.remove(&button) {
            self.just_released_buttons.insert(button);
        }

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

    /// Returns `true` while a mouse button is held.
    pub fn pressed(&self, button: MouseButton) -> bool {
        self.pressed_buttons.contains(&button)
    }

    /// Returns `true` for the first frame a mouse button is held.
    pub fn just_pressed(&self, button: MouseButton) -> bool {
        self.just_pressed_buttons.contains(&button)
    }

    /// Returns `true` for the first frame after a mouse button is released.
    pub fn just_released(&self, button: MouseButton) -> bool {
        self.just_released_buttons.contains(&button)
    }

    /// Returns the button-style state for a mouse button.
    pub fn button_state(&self, button: MouseButton) -> crate::ButtonState {
        if self.just_pressed(button) {
            crate::ButtonState::JustPressed
        } else if self.just_released(button) {
            crate::ButtonState::JustReleased
        } else if self.pressed(button) {
            crate::ButtonState::Pressed
        } else {
            crate::ButtonState::Released
        }
    }

    pub fn set_cursor_grabbed(&mut self, grabbed: bool) {
        self.cursor_grabbed = grabbed;
    }

    pub fn cursor_grabbed(&self) -> bool {
        self.cursor_grabbed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_motion_accumulates_without_cursor_position() {
        let mut mouse = MouseInput::default();

        mouse.process_delta(12.0, -4.0);
        mouse.process_delta(3.5, 2.0);

        assert_eq!(mouse.delta(), (15.5, -2.0));
    }

    #[test]
    fn mouse_button_transitions_are_visible_until_update_clears_them() {
        let mut mouse = MouseInput::default();

        mouse.process_button(MouseButton::Left, true);
        assert!(mouse.left_pressed);
        assert!(mouse.pressed(MouseButton::Left));
        assert!(mouse.just_pressed(MouseButton::Left));
        assert_eq!(
            mouse.button_state(MouseButton::Left),
            crate::ButtonState::JustPressed
        );

        mouse.update();
        assert!(mouse.pressed(MouseButton::Left));
        assert!(!mouse.just_pressed(MouseButton::Left));

        mouse.process_button(MouseButton::Left, false);
        assert!(!mouse.left_pressed);
        assert!(mouse.just_released(MouseButton::Left));

        mouse.update();
        assert_eq!(
            mouse.button_state(MouseButton::Left),
            crate::ButtonState::Released
        );
    }
}
