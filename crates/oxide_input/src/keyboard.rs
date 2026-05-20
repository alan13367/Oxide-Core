use std::collections::HashSet;

use oxide_ecs::Resource;
use winit::keyboard::{KeyCode, PhysicalKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonState {
    Released,
    JustPressed,
    Pressed,
    JustReleased,
}

#[derive(Resource, Default)]
pub struct KeyboardInput {
    keys: HashSet<KeyCode>,
    just_pressed: HashSet<KeyCode>,
    just_released: HashSet<KeyCode>,
}

impl KeyboardInput {
    /// Clears per-frame transition state.
    pub fn update(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }

    /// Processes a keyboard event using winit's physical key mapping.
    /// Physical keys keep positional bindings stable across keyboard layouts.
    pub fn process_event(&mut self, key: PhysicalKey, pressed: bool) {
        if let PhysicalKey::Code(code) = key {
            if pressed && !self.keys.contains(&code) {
                self.keys.insert(code);
                self.just_pressed.insert(code);
            } else if !pressed && self.keys.remove(&code) {
                self.just_released.insert(code);
            }
        }
    }

    pub fn pressed(&self, key: KeyCode) -> bool {
        self.keys.contains(&key)
    }

    pub fn just_pressed(&self, key: KeyCode) -> bool {
        self.just_pressed.contains(&key)
    }

    pub fn just_released(&self, key: KeyCode) -> bool {
        self.just_released.contains(&key)
    }

    pub fn button_state(&self, key: KeyCode) -> ButtonState {
        if self.just_pressed.contains(&key) {
            ButtonState::JustPressed
        } else if self.just_released.contains(&key) {
            ButtonState::JustReleased
        } else if self.keys.contains(&key) {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_transitions_are_visible_until_update_clears_them() {
        let mut keyboard = KeyboardInput::default();

        keyboard.process_event(PhysicalKey::Code(KeyCode::Space), true);
        assert!(keyboard.pressed(KeyCode::Space));
        assert!(keyboard.just_pressed(KeyCode::Space));
        assert_eq!(
            keyboard.button_state(KeyCode::Space),
            ButtonState::JustPressed
        );

        keyboard.update();
        assert!(keyboard.pressed(KeyCode::Space));
        assert!(!keyboard.just_pressed(KeyCode::Space));
        assert_eq!(keyboard.button_state(KeyCode::Space), ButtonState::Pressed);

        keyboard.process_event(PhysicalKey::Code(KeyCode::Space), false);
        assert!(!keyboard.pressed(KeyCode::Space));
        assert!(keyboard.just_released(KeyCode::Space));

        keyboard.update();
        assert_eq!(keyboard.button_state(KeyCode::Space), ButtonState::Released);
    }
}
