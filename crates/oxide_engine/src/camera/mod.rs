//! Camera module

mod controller;
mod uniform;

pub use controller::{camera_controller_system, CameraComponent, CameraController};
pub use uniform::{CameraBuffer, CameraUniform};
