//! Hierarchy components for parent-child relationships

use oxide_ecs::{entity::Entity, Component};

/// Component that references the parent entity.
/// Automatically managed when using hierarchy commands.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parent(pub Entity);

/// Component that stores references to all child entities.
/// Automatically managed when using hierarchy commands.
#[derive(Component, Clone, Debug, Default)]
pub struct Children(pub Vec<Entity>);

impl Children {
    /// Creates an empty children component.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Creates a children component with the given entities.
    pub fn with(entities: Vec<Entity>) -> Self {
        Self(entities)
    }

    /// Adds a child entity.
    pub fn push(&mut self, entity: Entity) {
        self.0.push(entity);
    }

    /// Removes a child entity if present.
    pub fn remove(&mut self, entity: Entity) {
        self.0.retain(|&e| e != entity);
    }

    /// Returns the number of children.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if there are no children.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over child entities.
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.iter().copied()
    }
}
