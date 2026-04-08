//! Oxide Core in-house physics integration backend.

mod character_controller;
mod collision;
mod components;
mod events;
mod joints;
mod mass_properties;
mod plugin;
mod queries;
mod resources;
mod systems;

#[cfg(feature = "debug-render")]
mod debug_plugin;

pub mod prelude;

pub use character_controller::*;
pub use collision::*;
pub use components::*;
pub use events::*;
pub use joints::*;
pub use mass_properties::*;
pub use plugin::*;
pub use queries::*;
pub use resources::*;
pub use systems::*;

#[cfg(feature = "debug-render")]
pub use debug_plugin::*;
