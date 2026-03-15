//! Oxide Core in-house physics integration backend.

mod components;
mod math;
mod plugin;
mod resources;
mod systems;

pub mod prelude;

pub use components::*;
pub use math::*;
pub use plugin::*;
pub use resources::*;
pub use systems::*;
