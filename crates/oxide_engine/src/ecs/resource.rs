//! Engine resources

use std::time::{Duration, Instant};

use bevy_ecs::prelude::Resource;
use oxide_renderer::Renderer;

#[derive(Resource)]
pub struct Time {
    pub delta: Duration,
    pub elapsed: Duration,
    last_frame: Instant,
}

impl Default for Time {
    fn default() -> Self {
        Self {
            delta: Duration::ZERO,
            elapsed: Duration::ZERO,
            last_frame: Instant::now(),
        }
    }
}

impl Time {
    pub fn update(&mut self) {
        let now = Instant::now();
        self.delta = now - self.last_frame;
        self.elapsed += self.delta;
        self.last_frame = now;
    }

    pub fn delta_secs(&self) -> f32 {
        self.delta.as_secs_f32()
    }

    pub fn elapsed_secs(&self) -> f32 {
        self.elapsed.as_secs_f32()
    }
}

#[derive(Resource)]
pub struct RendererResource {
    pub renderer: Renderer,
}

impl RendererResource {
    pub fn new(renderer: Renderer) -> Self {
        Self { renderer }
    }
}

#[derive(Resource)]
pub struct WindowResource {
    pub width: u32,
    pub height: u32,
}

impl WindowResource {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn aspect_ratio(&self) -> f32 {
        if self.height > 0 {
            self.width as f32 / self.height as f32
        } else {
            1.0
        }
    }

    pub fn update(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }
}
