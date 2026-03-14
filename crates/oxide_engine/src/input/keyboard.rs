//! Keyboard input handling

use std::collections::HashSet;

use bevy_ecs::prelude::Resource;
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
    pub fn update(&mut self) {
        let to_press: Vec<_> = self.just_pressed.drain().collect();
        for key in to_press {
            self.keys.insert(key);
        }
        self.just_released.clear();
    }

    pub fn process_event(&mut self, key: PhysicalKey, pressed: bool) {
        if let PhysicalKey::Code(code) = key {
            if pressed && !self.keys.contains(&code) {
                self.just_pressed.insert(code);
            } else if !pressed && self.keys.contains(&code) {
                self.keys.remove(&code);
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
