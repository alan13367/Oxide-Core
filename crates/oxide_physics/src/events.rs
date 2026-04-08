//! Collision events for gameplay logic.
//!
//! Provides events that fire when collisions start, persist, and end.
//! These can be used for triggers, sound effects, damage, etc.

use glam::Vec3;
use oxide_ecs::Resource;
use std::collections::HashMap;

use crate::components::{BodyId, ColliderId};

/// A contact point in a collision event.
#[derive(Clone, Copy, Debug)]
pub struct EventContact {
    /// World-space position of the contact.
    pub position: Vec3,
    /// Normal from body A to body B.
    pub normal: Vec3,
    /// Penetration depth.
    pub penetration: f32,
}

/// Types of collision events.
#[derive(Clone, Debug)]
pub enum CollisionEvent {
    /// Collision just started this frame.
    Started {
        body_a: BodyId,
        body_b: BodyId,
        collider_a: ColliderId,
        collider_b: ColliderId,
        contacts: Vec<EventContact>,
    },
    /// Collision persisted from previous frame.
    Persisted {
        body_a: BodyId,
        body_b: BodyId,
        collider_a: ColliderId,
        collider_b: ColliderId,
        contacts: Vec<EventContact>,
    },
    /// Collision ended this frame.
    Ended {
        body_a: BodyId,
        body_b: BodyId,
        collider_a: ColliderId,
        collider_b: ColliderId,
    },
}

impl CollisionEvent {
    /// Get the body IDs involved in this event.
    pub fn bodies(&self) -> (BodyId, BodyId) {
        match self {
            CollisionEvent::Started { body_a, body_b, .. } => (*body_a, *body_b),
            CollisionEvent::Persisted { body_a, body_b, .. } => (*body_a, *body_b),
            CollisionEvent::Ended { body_a, body_b, .. } => (*body_a, *body_b),
        }
    }

    /// Get the collider IDs involved in this event.
    pub fn colliders(&self) -> (ColliderId, ColliderId) {
        match self {
            CollisionEvent::Started {
                collider_a,
                collider_b,
                ..
            } => (*collider_a, *collider_b),
            CollisionEvent::Persisted {
                collider_a,
                collider_b,
                ..
            } => (*collider_a, *collider_b),
            CollisionEvent::Ended {
                collider_a,
                collider_b,
                ..
            } => (*collider_a, *collider_b),
        }
    }

    /// Check if this is a start event.
    pub fn is_start(&self) -> bool {
        matches!(self, CollisionEvent::Started { .. })
    }

    /// Check if this is an end event.
    pub fn is_end(&self) -> bool {
        matches!(self, CollisionEvent::Ended { .. })
    }
}

/// Resource for storing collision events.
#[derive(Resource, Default)]
pub struct CollisionEvents {
    /// Events from the current frame.
    events: Vec<CollisionEvent>,
    /// Active collisions from previous frame for tracking ended events.
    /// Stores collider pair -> (body_a, body_b).
    active_collisions: HashMap<(ColliderId, ColliderId), (BodyId, BodyId)>,
}

impl CollisionEvents {
    /// Create a new collision events buffer.
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            active_collisions: HashMap::new(),
        }
    }

    /// Add an event to the buffer.
    pub fn push(&mut self, event: CollisionEvent) {
        self.events.push(event);
    }

    /// Get all events from the current frame.
    pub fn iter(&self) -> impl Iterator<Item = &CollisionEvent> {
        self.events.iter()
    }

    /// Drain all events from the buffer.
    pub fn drain(&mut self) -> impl Iterator<Item = CollisionEvent> + '_ {
        self.events.drain(..)
    }

    /// Clear all events.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Get the number of events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if there are no events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Record an active collision for tracking ended events.
    pub fn record_active(
        &mut self,
        collider_a: ColliderId,
        collider_b: ColliderId,
        body_a: BodyId,
        body_b: BodyId,
    ) {
        let key = if collider_a.0 <= collider_b.0 {
            (collider_a, collider_b)
        } else {
            (collider_b, collider_a)
        };
        self.active_collisions.insert(key, (body_a, body_b));
    }

    /// Check if a collision was active in the previous frame.
    pub fn was_active(&self, collider_a: ColliderId, collider_b: ColliderId) -> bool {
        let key = if collider_a.0 <= collider_b.0 {
            (collider_a, collider_b)
        } else {
            (collider_b, collider_a)
        };
        self.active_collisions.contains_key(&key)
    }

    /// Clear active collisions at the start of a frame.
    pub fn clear_active(&mut self) {
        self.active_collisions.clear();
    }

    /// Get active collisions for generating ended events.
    pub fn take_active(&mut self) -> HashMap<(ColliderId, ColliderId), (BodyId, BodyId)> {
        std::mem::take(&mut self.active_collisions)
    }
}

/// Generate collision events from contact manifolds.
///
/// Call this after collision detection but before the solver.
pub fn generate_collision_events(
    physics: &crate::resources::PhysicsWorld,
    events: &mut CollisionEvents,
) {
    // Track which collisions are active this frame
    let mut current_active = HashMap::new();

    for (key, manifold) in &physics.cached_manifolds {
        if manifold.is_empty() {
            continue;
        }

        let collider_a = key.collider_a;
        let collider_b = key.collider_b;

        // Record as active this frame
        let ordered_key = if collider_a.0 <= collider_b.0 {
            (collider_a, collider_b)
        } else {
            (collider_b, collider_a)
        };
        current_active.insert(ordered_key, (manifold.body_a, manifold.body_b));

        // Convert contacts
        let contacts: Vec<EventContact> = manifold
            .contacts
            .iter()
            .map(|c| EventContact {
                position: c.position,
                normal: c.normal,
                penetration: c.penetration,
            })
            .collect();

        // Determine if this is a start or persist event
        let event = if events.was_active(collider_a, collider_b) {
            CollisionEvent::Persisted {
                body_a: manifold.body_a,
                body_b: manifold.body_b,
                collider_a,
                collider_b,
                contacts,
            }
        } else {
            CollisionEvent::Started {
                body_a: manifold.body_a,
                body_b: manifold.body_b,
                collider_a,
                collider_b,
                contacts,
            }
        };

        events.push(event);
    }

    // Generate ended events for collisions that were active but no longer are
    let previous_active = events.take_active();
    for ((collider_a, collider_b), (body_a, body_b)) in previous_active {
        if !current_active.contains_key(&(collider_a, collider_b)) {
            // This collision ended
            events.push(CollisionEvent::Ended {
                body_a,
                body_b,
                collider_a,
                collider_b,
            });
        }
    }

    // Store current active for next frame
    for (key, (body_a, body_b)) in current_active {
        events.record_active(key.0, key.1, body_a, body_b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_event_bodies() {
        let event = CollisionEvent::Started {
            body_a: BodyId(1),
            body_b: BodyId(2),
            collider_a: ColliderId(1),
            collider_b: ColliderId(2),
            contacts: vec![],
        };

        let (a, b) = event.bodies();
        assert_eq!(a, BodyId(1));
        assert_eq!(b, BodyId(2));
    }

    #[test]
    fn collision_event_is_start() {
        let start = CollisionEvent::Started {
            body_a: BodyId(1),
            body_b: BodyId(2),
            collider_a: ColliderId(1),
            collider_b: ColliderId(2),
            contacts: vec![],
        };
        assert!(start.is_start());

        let ended = CollisionEvent::Ended {
            body_a: BodyId(1),
            body_b: BodyId(2),
            collider_a: ColliderId(1),
            collider_b: ColliderId(2),
        };
        assert!(!ended.is_start());
    }

    #[test]
    fn events_buffer_push_and_drain() {
        let mut events = CollisionEvents::new();

        events.push(CollisionEvent::Ended {
            body_a: BodyId(1),
            body_b: BodyId(2),
            collider_a: ColliderId(1),
            collider_b: ColliderId(2),
        });

        assert_eq!(events.len(), 1);

        let drained: Vec<_> = events.drain().collect();
        assert_eq!(drained.len(), 1);
        assert!(events.is_empty());
    }

    #[test]
    fn ended_events_preserve_body_ids_from_previous_frame() {
        let mut events = CollisionEvents::new();
        events.record_active(ColliderId(11), ColliderId(22), BodyId(5), BodyId(6));

        let physics = crate::resources::PhysicsWorld::default();
        generate_collision_events(&physics, &mut events);

        let drained: Vec<_> = events.drain().collect();
        assert_eq!(drained.len(), 1, "expected a single ended event");

        match &drained[0] {
            CollisionEvent::Ended {
                body_a,
                body_b,
                collider_a,
                collider_b,
            } => {
                assert_eq!(*body_a, BodyId(5));
                assert_eq!(*body_b, BodyId(6));
                assert_eq!(*collider_a, ColliderId(11));
                assert_eq!(*collider_b, ColliderId(22));
            }
            _ => panic!("expected ended collision event"),
        }
    }
}
