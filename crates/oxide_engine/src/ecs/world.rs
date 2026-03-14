//! World wrapper and system scheduling

pub use oxide_ecs::component::Component;
pub use oxide_ecs::entity::Entity;
pub use oxide_ecs::prelude::Resource;
pub use oxide_ecs::schedule::{Schedule, ScheduleLabel};
pub use oxide_ecs::system::{
    in_state, CommandQueue, Commands, IntoSystem, IntoSystemExt, Query, Res, ResMut, State, System,
    SystemParam,
};
pub use oxide_ecs::world::World;
