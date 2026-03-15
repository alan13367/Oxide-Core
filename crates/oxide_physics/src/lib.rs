//! Oxide Core in-house physics integration backend.

mod components;
mod plugin;
mod resources;
mod systems;

pub mod prelude;

pub use components::*;
pub use plugin::*;
pub use resources::*;
pub use systems::*;
