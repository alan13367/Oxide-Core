//! World wrapper and system scheduling

pub use oxide_ecs::component::Component;
pub use oxide_ecs::entity::Entity;
pub use oxide_ecs::event::Events;
pub use oxide_ecs::prelude::Resource;
pub use oxide_ecs::query::{Added, Changed, With, Without};
pub use oxide_ecs::schedule::{Schedule, ScheduleLabel, ScheduleOrderDiagnostic};
pub use oxide_ecs::system::{
    in_state, state_entered, state_exited, CommandQueue, Commands, ComponentChanges, EventCursor,
    EventDrain, EventReader, EventWriter, IntoSystem, IntoSystemExt, Local, Query,
    RemovedComponents, Res, ResMut, ResourceCursor, State, StateTransition, System, SystemParam,
};
pub use oxide_ecs::world::{RemovedComponent, World};
