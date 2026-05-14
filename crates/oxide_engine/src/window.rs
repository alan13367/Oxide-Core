//! Window abstraction

use std::sync::Arc;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event_loop::ActiveEventLoop,
    window::{CursorGrabMode, Window as WinitWindow, WindowId},
};

#[derive(Clone)]
pub struct Window {
    inner: Arc<WinitWindow>,
}

impl Window {
    pub fn new(event_loop: &ActiveEventLoop, title: &str, width: u32, height: u32) -> Self {
        let window = event_loop
            .create_window(
                WinitWindow::default_attributes()
                    .with_title(title)
                    .with_inner_size(PhysicalSize::new(width, height)),
            )
            .expect("Failed to create window");

        Self {
            inner: Arc::new(window),
        }
    }

    pub fn id(&self) -> WindowId {
        self.inner.id()
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.inner.inner_size()
    }

    pub fn scale_factor(&self) -> f64 {
        self.inner.scale_factor()
    }

    pub fn set_title(&self, title: &str) {
        self.inner.set_title(title);
    }

    pub fn set_cursor_visible(&self, visible: bool) {
        self.inner.set_cursor_visible(visible);
    }

    pub fn set_cursor_grabbed(&self, grabbed: bool) -> Result<(), winit::error::ExternalError> {
        if grabbed {
            self.inner
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| self.inner.set_cursor_grab(CursorGrabMode::Confined))
        } else {
            self.inner.set_cursor_grab(CursorGrabMode::None)
        }
    }

    pub fn center_cursor(&self) -> Result<(), winit::error::ExternalError> {
        let size = self.size();
        self.set_cursor_position(PhysicalPosition::new(
            size.width as f64 * 0.5,
            size.height as f64 * 0.5,
        ))
    }

    pub fn set_cursor_position(
        &self,
        position: PhysicalPosition<f64>,
    ) -> Result<(), winit::error::ExternalError> {
        self.inner.set_cursor_position(position)
    }

    pub fn request_redraw(&self) {
        self.inner.request_redraw();
    }

    pub fn winit_window(&self) -> &Arc<WinitWindow> {
        &self.inner
    }
}

impl std::ops::Deref for Window {
    type Target = Arc<WinitWindow>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
