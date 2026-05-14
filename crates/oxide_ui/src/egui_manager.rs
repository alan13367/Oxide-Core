//! egui manager - context and state management
//!
//! Provides egui context and winit integration for debug/editor UI.
//! Note: Applications should use the context directly for rendering
//! due to wgpu version compatibility.

use egui_winit::State;
use wgpu::{Device, TextureFormat};
use winit::window::Window;

/// Manager for egui state and rendering.
pub struct EguiManager {
    /// The egui context.
    pub context: egui::Context,
    /// winit integration state.
    pub winit_state: State,
}

impl EguiManager {
    /// Creates a new egui manager.
    pub fn new(
        _device: &Device,
        _output_format: TextureFormat,
        window: &Window,
        scale_factor: f32,
    ) -> Self {
        let context = egui::Context::default();
        let viewport_id = context.viewport_id();

        let winit_state = State::new(
            context.clone(),
            viewport_id,
            window,
            Some(scale_factor),
            None,
            None,
        );

        Self {
            context,
            winit_state,
        }
    }

    /// Returns true if egui wants pointer input.
    pub fn wants_pointer_input(&self) -> bool {
        self.context.wants_pointer_input()
    }

    /// Returns true if egui wants keyboard input.
    pub fn wants_keyboard_input(&self) -> bool {
        self.context.wants_keyboard_input()
    }

    /// Begins a new egui frame.
    pub fn begin_frame(&mut self, window: &Window) {
        let raw_input = self.winit_state.take_egui_input(window);
        self.context.begin_pass(raw_input);
    }

    /// Ends the current frame and returns the output.
    pub fn end_frame(&mut self) -> egui::FullOutput {
        self.context.end_pass()
    }
}
