//! Oxide Core - A 3D game engine targeting macOS/Metal
//!
//! Oxide Core is built from scratch using wgpu for rendering,
//! bevy_ecs for entity-component-system architecture, and glam for math.

pub mod app;
pub mod camera;
pub mod ecs;
pub mod event;
pub mod input;
pub mod prelude;
pub mod scene;
pub mod time;
pub mod window;

pub use oxide_renderer as renderer;
pub use oxide_math as math;

pub use bevy_ecs;