//! Camera module

mod controller;
mod uniform;

pub use controller::{CameraComponent, CameraController, camera_controller_system};
pub use uniform::{CameraBuffer, CameraUniform};