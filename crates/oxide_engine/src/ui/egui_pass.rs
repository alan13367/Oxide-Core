//! egui render pass
//!
//! Provides helper functions for egui event handling.
//! Note: Applications should use EguiManager's context directly for rendering
//! due to wgpu version compatibility.

use super::EguiManager;

/// Handles a winit event and returns true if it was consumed by egui.
pub fn handle_egui_event(
    manager: &mut EguiManager,
    window: &winit::window::Window,
    event: &winit::event::WindowEvent,
) -> bool {
    let response = manager.winit_state.on_window_event(window, event);

    // Check if egui wants to consume input
    if manager.wants_pointer_input() || manager.wants_keyboard_input() {
        response.consumed
    } else {
        false
    }
}

/// Trait for implementing custom egui render passes.
/// Implement this trait to render egui UI in your application.
pub trait EguiRender {
    /// Renders the egui UI. Called between begin_frame and end_frame.
    fn show(&mut self, ctx: &egui::Context);
}