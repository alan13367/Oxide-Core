//! Oxide Core - A 3D game engine targeting macOS/Metal
//!
//! Oxide Core is built from scratch using wgpu for rendering,
//! Oxide ECS abstractions for entity-component-system architecture, and glam for math.

pub mod app;
pub mod asset;
pub mod camera;
pub mod ecs;
pub mod event;
pub mod input;
pub mod light;
pub mod prelude;
pub mod scene;
pub mod time;
pub mod ui;
pub mod watcher;
pub mod window;

pub use oxide_math as math;
pub use oxide_renderer as renderer;

pub use oxide_ecs;
