//! egui manager - context and state management
//!
//! Provides egui context and winit integration for debug/editor UI.
//! Note: Applications should use the context directly for rendering
//! due to wgpu version compatibility.

use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use egui_winit::State;
use wgpu::{CommandEncoder, Device, Queue, TextureFormat, TextureView};
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
        self.context.egui_wants_pointer_input()
    }

    /// Returns true if egui wants keyboard input.
    pub fn wants_keyboard_input(&self) -> bool {
        self.context.egui_wants_keyboard_input()
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

/// Engine-owned egui integration that handles winit input and renders with wgpu.
pub struct EguiWgpuPass {
    manager: EguiManager,
    renderer: Renderer,
}

impl EguiWgpuPass {
    pub fn new(
        device: &Device,
        output_format: TextureFormat,
        window: &Window,
        scale_factor: f32,
    ) -> Self {
        Self {
            manager: EguiManager::new(device, output_format, window, scale_factor),
            renderer: Renderer::new(device, output_format, RendererOptions::default()),
        }
    }

    pub fn context(&self) -> &egui::Context {
        &self.manager.context
    }

    pub fn wants_pointer_input(&self) -> bool {
        self.manager.wants_pointer_input()
    }

    pub fn wants_keyboard_input(&self) -> bool {
        self.manager.wants_keyboard_input()
    }

    pub fn begin_frame(&mut self, window: &Window) {
        self.manager.begin_frame(window);
    }

    pub fn handle_event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        super::handle_egui_event(&mut self.manager, window, event)
    }

    pub fn queue(
        &mut self,
        window: &Window,
        device: &Device,
        queue: &Queue,
        view: &TextureView,
        encoder: &mut CommandEncoder,
    ) {
        let output = self.manager.end_frame();
        self.manager
            .winit_state
            .handle_platform_output(window, output.platform_output);

        for (id, image_delta) in &output.textures_delta.set {
            self.renderer
                .update_texture(device, queue, *id, image_delta);
        }

        let pixels_per_point = output.pixels_per_point;
        let paint_jobs = self
            .manager
            .context
            .tessellate(output.shapes, pixels_per_point);
        let size = window.inner_size();
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [size.width, size.height],
            pixels_per_point,
        };

        let callback_commands =
            self.renderer
                .update_buffers(device, queue, encoder, &paint_jobs, &screen_descriptor);
        if !callback_commands.is_empty() {
            queue.submit(callback_commands);
        }

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Oxide egui Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut render_pass = render_pass.forget_lifetime();
            self.renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }

        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}
