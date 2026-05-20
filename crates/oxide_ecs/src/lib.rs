#![allow(clippy::type_complexity)]

extern crate self as oxide_ecs;

pub use oxide_ecs_derive::{Component, Resource, ScheduleLabel};

pub mod component {
    pub trait Component: 'static {}
}

pub mod resource {
    pub trait Resource: 'static {}
}

pub mod event {
    use crate::resource::Resource;

    /// FIFO event buffer for gameplay/system communication.
    pub struct Events<T> {
        events: Vec<T>,
    }

    impl<T> Default for Events<T> {
        fn default() -> Self {
            Self { events: Vec::new() }
        }
    }

    impl<T: 'static> Resource for Events<T> {}

    impl<T> Events<T> {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn send(&mut self, event: T) {
            self.events.push(event);
        }

        pub fn extend<I>(&mut self, events: I)
        where
            I: IntoIterator<Item = T>,
        {
            self.events.extend(events);
        }

        pub fn iter(&self) -> impl Iterator<Item = &T> {
            self.events.iter()
        }

        pub fn iter_from(&self, index: usize) -> impl Iterator<Item = &T> {
            self.events[index.min(self.events.len())..].iter()
        }

        pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
            self.events.drain(..)
        }

        pub fn clear(&mut self) {
            self.events.clear();
        }

        pub fn len(&self) -> usize {
            self.events.len()
        }

        pub fn is_empty(&self) -> bool {
            self.events.is_empty()
        }
    }
}

pub mod entity {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Entity {
        pub(crate) index: u32,
        pub(crate) generation: u32,
    }

    impl Entity {
        pub fn index(self) -> u32 {
            self.index
        }

        pub fn generation(self) -> u32 {
            self.generation
        }
    }
}

pub mod query {
    use std::marker::PhantomData;

    pub struct Added<T>(pub(crate) PhantomData<T>);
    pub struct Changed<T>(pub(crate) PhantomData<T>);
    pub struct With<T>(pub(crate) PhantomData<T>);
    pub struct Without<T>(pub(crate) PhantomData<T>);

    impl<T> Default for Added<T> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }

    impl<T> Default for Changed<T> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }

    impl<T> Default for With<T> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }

    impl<T> Default for Without<T> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }
}

pub mod schedule {
    use super::system::{CommandQueue, IntoSystem, System};
    use super::world::World;
    use std::collections::{HashMap, HashSet};

    pub trait ScheduleLabel: 'static {}

    pub type SystemFn = fn(&mut World);

    struct ScheduledSystem {
        system: System,
        label: Option<String>,
        sets: Vec<String>,
        before: Vec<String>,
        after: Vec<String>,
        insertion_index: usize,
    }

    #[derive(Clone, Debug)]
    struct SetConstraint {
        set: String,
        before: Vec<String>,
        after: Vec<String>,
    }

    /// A non-fatal ordering issue found in a [`Schedule`].
    ///
    /// Schedules still run when diagnostics are present. Missing labels are
    /// ignored and cycles fall back to insertion order for the cyclic subset.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ScheduleOrderDiagnostic {
        pub message: String,
    }

    impl ScheduleOrderDiagnostic {
        fn new(message: impl Into<String>) -> Self {
            Self {
                message: message.into(),
            }
        }
    }

    #[derive(Default)]
    pub struct Schedule {
        systems: Vec<ScheduledSystem>,
        set_constraints: Vec<SetConstraint>,
        next_insertion_index: usize,
    }

    impl Schedule {
        pub fn new() -> Self {
            Self::default()
        }

        /// Adds a system to the end of this schedule.
        ///
        /// Systems use the same [`IntoSystem`] conversion as the engine app
        /// runner, so functions can request params such as `Res`, `ResMut`,
        /// `Query`, `Commands`, and event readers/writers.
        pub fn add_system<S, Marker>(&mut self, system: S) -> &mut Self
        where
            S: IntoSystem<Marker>,
        {
            self.push_system(
                system.into_system(),
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            self
        }

        /// Adds a system to a named set.
        ///
        /// Sets are ordering groups. A before/after target can reference either
        /// a system label or a set name.
        pub fn add_system_to_set<S, Marker>(
            &mut self,
            set: impl Into<String>,
            system: S,
        ) -> &mut Self
        where
            S: IntoSystem<Marker>,
        {
            self.push_system(
                system.into_system(),
                None,
                vec![set.into()],
                Vec::new(),
                Vec::new(),
            );
            self
        }

        /// Adds a system with a stable label that other systems can reference.
        pub fn add_labeled_system<S, Marker>(
            &mut self,
            label: impl Into<String>,
            system: S,
        ) -> &mut Self
        where
            S: IntoSystem<Marker>,
        {
            self.push_system(
                system.into_system(),
                Some(label.into()),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            self
        }

        /// Adds a labeled system to a named set.
        pub fn add_labeled_system_to_set<S, Marker>(
            &mut self,
            label: impl Into<String>,
            set: impl Into<String>,
            system: S,
        ) -> &mut Self
        where
            S: IntoSystem<Marker>,
        {
            self.push_system(
                system.into_system(),
                Some(label.into()),
                vec![set.into()],
                Vec::new(),
                Vec::new(),
            );
            self
        }

        /// Adds a system that should run before the system with `before_label`.
        pub fn add_system_before<S, Marker>(
            &mut self,
            before_label: impl Into<String>,
            system: S,
        ) -> &mut Self
        where
            S: IntoSystem<Marker>,
        {
            self.push_system(
                system.into_system(),
                None,
                Vec::new(),
                vec![before_label.into()],
                Vec::new(),
            );
            self
        }

        /// Adds a labeled system that should run before `before_label`.
        pub fn add_labeled_system_before<S, Marker>(
            &mut self,
            label: impl Into<String>,
            before_label: impl Into<String>,
            system: S,
        ) -> &mut Self
        where
            S: IntoSystem<Marker>,
        {
            self.push_system(
                system.into_system(),
                Some(label.into()),
                Vec::new(),
                vec![before_label.into()],
                Vec::new(),
            );
            self
        }

        /// Adds a system that should run after the system with `after_label`.
        pub fn add_system_after<S, Marker>(
            &mut self,
            after_label: impl Into<String>,
            system: S,
        ) -> &mut Self
        where
            S: IntoSystem<Marker>,
        {
            self.push_system(
                system.into_system(),
                None,
                Vec::new(),
                Vec::new(),
                vec![after_label.into()],
            );
            self
        }

        /// Adds a labeled system that should run after `after_label`.
        pub fn add_labeled_system_after<S, Marker>(
            &mut self,
            label: impl Into<String>,
            after_label: impl Into<String>,
            system: S,
        ) -> &mut Self
        where
            S: IntoSystem<Marker>,
        {
            self.push_system(
                system.into_system(),
                Some(label.into()),
                Vec::new(),
                Vec::new(),
                vec![after_label.into()],
            );
            self
        }

        /// Orders every system in `set` before systems matched by `before`.
        pub fn configure_set_before(
            &mut self,
            set: impl Into<String>,
            before: impl Into<String>,
        ) -> &mut Self {
            self.set_constraints.push(SetConstraint {
                set: set.into(),
                before: vec![before.into()],
                after: Vec::new(),
            });
            self
        }

        /// Orders every system in `set` after systems matched by `after`.
        pub fn configure_set_after(
            &mut self,
            set: impl Into<String>,
            after: impl Into<String>,
        ) -> &mut Self {
            self.set_constraints.push(SetConstraint {
                set: set.into(),
                before: Vec::new(),
                after: vec![after.into()],
            });
            self
        }

        /// Returns the number of systems currently registered.
        pub fn len(&self) -> usize {
            self.systems.len()
        }

        /// Returns true when the schedule has no systems.
        pub fn is_empty(&self) -> bool {
            self.systems.is_empty()
        }

        /// Returns non-fatal ordering diagnostics for missing labels, duplicate
        /// labels, and cycles.
        pub fn ordering_diagnostics(&self) -> Vec<ScheduleOrderDiagnostic> {
            let mut diagnostics = Vec::new();
            let mut labels = HashSet::new();
            for system in &self.systems {
                if let Some(label) = &system.label {
                    if !labels.insert(label.as_str()) {
                        diagnostics.push(ScheduleOrderDiagnostic::new(format!(
                            "duplicate system label '{label}'"
                        )));
                    }
                }
            }

            let targets = self.target_lookup();
            for system in &self.systems {
                for before in &system.before {
                    if !targets.contains_key(before.as_str()) {
                        diagnostics.push(ScheduleOrderDiagnostic::new(format!(
                            "system ordering references missing before-label '{before}'"
                        )));
                    }
                }
                for after in &system.after {
                    if !targets.contains_key(after.as_str()) {
                        diagnostics.push(ScheduleOrderDiagnostic::new(format!(
                            "system ordering references missing after-label '{after}'"
                        )));
                    }
                }
            }
            for constraint in &self.set_constraints {
                if !targets.contains_key(constraint.set.as_str()) {
                    diagnostics.push(ScheduleOrderDiagnostic::new(format!(
                        "set ordering references missing set '{}'",
                        constraint.set
                    )));
                }
                for before in &constraint.before {
                    if !targets.contains_key(before.as_str()) {
                        diagnostics.push(ScheduleOrderDiagnostic::new(format!(
                            "set ordering references missing before-label '{before}'"
                        )));
                    }
                }
                for after in &constraint.after {
                    if !targets.contains_key(after.as_str()) {
                        diagnostics.push(ScheduleOrderDiagnostic::new(format!(
                            "set ordering references missing after-label '{after}'"
                        )));
                    }
                }
            }

            if self.has_ordering_cycle() {
                diagnostics.push(ScheduleOrderDiagnostic::new(
                    "system ordering contains a cycle",
                ));
            }

            diagnostics
        }

        /// Runs all systems in insertion order and applies deferred commands at
        /// the end of the schedule. Labeled before/after constraints are used
        /// to derive a stable order when possible.
        pub fn run(&mut self, world: &mut World) {
            let mut commands = CommandQueue::new();
            for index in self.run_order() {
                self.systems[index].system.run(world, &mut commands);
            }
            commands.apply(world);
        }

        fn push_system(
            &mut self,
            system: System,
            label: Option<String>,
            sets: Vec<String>,
            before: Vec<String>,
            after: Vec<String>,
        ) {
            self.systems.push(ScheduledSystem {
                system,
                label,
                sets,
                before,
                after,
                insertion_index: self.next_insertion_index,
            });
            self.next_insertion_index += 1;
        }

        fn target_lookup(&self) -> HashMap<&str, Vec<usize>> {
            let mut lookup: HashMap<&str, Vec<usize>> = HashMap::new();
            for (index, system) in self.systems.iter().enumerate() {
                if let Some(label) = &system.label {
                    lookup.entry(label.as_str()).or_default().push(index);
                }
                for set in &system.sets {
                    lookup.entry(set.as_str()).or_default().push(index);
                }
            }
            lookup
        }

        fn ordering_edges(&self) -> Vec<(usize, usize)> {
            let lookup = self.target_lookup();
            let mut edges = Vec::new();
            let mut seen = HashSet::new();

            for (index, system) in self.systems.iter().enumerate() {
                for before in &system.before {
                    if let Some(targets) = lookup.get(before.as_str()) {
                        for &target in targets {
                            if index != target && seen.insert((index, target)) {
                                edges.push((index, target));
                            }
                        }
                    }
                }
                for after in &system.after {
                    if let Some(sources) = lookup.get(after.as_str()) {
                        for &source in sources {
                            if source != index && seen.insert((source, index)) {
                                edges.push((source, index));
                            }
                        }
                    }
                }
            }
            for constraint in &self.set_constraints {
                let Some(members) = lookup.get(constraint.set.as_str()) else {
                    continue;
                };
                for before in &constraint.before {
                    if let Some(targets) = lookup.get(before.as_str()) {
                        for &member in members {
                            for &target in targets {
                                if member != target && seen.insert((member, target)) {
                                    edges.push((member, target));
                                }
                            }
                        }
                    }
                }
                for after in &constraint.after {
                    if let Some(sources) = lookup.get(after.as_str()) {
                        for &source in sources {
                            for &member in members {
                                if source != member && seen.insert((source, member)) {
                                    edges.push((source, member));
                                }
                            }
                        }
                    }
                }
            }

            edges
        }

        fn has_ordering_cycle(&self) -> bool {
            self.topological_order(false).len() != self.systems.len()
        }

        fn run_order(&self) -> Vec<usize> {
            let mut order = self.topological_order(true);
            if order.len() < self.systems.len() {
                let emitted: HashSet<_> = order.iter().copied().collect();
                let mut remaining: Vec<_> = (0..self.systems.len())
                    .filter(|index| !emitted.contains(index))
                    .collect();
                remaining.sort_by_key(|index| self.systems[*index].insertion_index);
                order.extend(remaining);
            }
            order
        }

        fn topological_order(&self, allow_partial: bool) -> Vec<usize> {
            let edges = self.ordering_edges();
            let mut incoming = vec![0usize; self.systems.len()];
            let mut outgoing = vec![Vec::new(); self.systems.len()];
            for (source, target) in edges {
                incoming[target] += 1;
                outgoing[source].push(target);
            }

            let mut emitted = vec![false; self.systems.len()];
            let mut order = Vec::new();

            loop {
                let next = (0..self.systems.len())
                    .filter(|index| !emitted[*index] && incoming[*index] == 0)
                    .min_by_key(|index| self.systems[*index].insertion_index);

                let Some(index) = next else {
                    break;
                };

                emitted[index] = true;
                order.push(index);
                for target in &outgoing[index] {
                    incoming[*target] -= 1;
                }
            }

            if !allow_partial && order.len() != self.systems.len() {
                return order;
            }

            order
        }
    }
}

pub mod system {
    use std::any::{Any, TypeId};
    use std::collections::HashMap;
    use std::marker::PhantomData;
    use std::ops::{Deref, DerefMut};

    use crate::component::Component;
    use crate::entity::Entity;
    use crate::event::Events;
    use crate::resource::Resource;
    use crate::world::{Bundle, RemovedComponent, World};

    pub trait SystemParam: Sized {
        /// Fetches this parameter from raw world, command-queue, and local-state pointers.
        ///
        /// # Safety
        /// `world`, `commands`, and `locals` must be valid pointers for the duration of the call
        /// and must originate from the currently executing schedule stage.
        unsafe fn fetch(
            world: *mut World,
            commands: *mut CommandQueue,
            locals: *mut SystemLocals,
        ) -> Self;
    }

    trait DeferredCommand {
        fn apply(self: Box<Self>, world: &mut World);
    }

    impl<F> DeferredCommand for F
    where
        F: FnOnce(&mut World) + 'static,
    {
        fn apply(self: Box<Self>, world: &mut World) {
            (*self)(world);
        }
    }

    #[derive(Default)]
    pub struct CommandQueue {
        commands: Vec<Box<dyn DeferredCommand>>,
    }

    impl CommandQueue {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn push<F>(&mut self, command: F)
        where
            F: FnOnce(&mut World) + 'static,
        {
            self.commands.push(Box::new(command));
        }

        pub fn is_empty(&self) -> bool {
            self.commands.is_empty()
        }

        pub fn apply(&mut self, world: &mut World) {
            let commands = std::mem::take(&mut self.commands);
            for command in commands {
                command.apply(world);
            }
        }
    }

    pub struct Commands {
        world: *mut World,
        queue: *mut CommandQueue,
    }

    impl Commands {
        pub fn new(world: *mut World, queue: *mut CommandQueue) -> Self {
            Self { world, queue }
        }

        /// Reserves an entity immediately and inserts the bundle when commands apply.
        ///
        /// The returned [`EntityCommands`] can be used to queue additional edits
        /// against the same entity before it has visible components.
        pub fn spawn<B>(&mut self, bundle: B) -> EntityCommands
        where
            B: Bundle + 'static,
        {
            let entity = unsafe { &mut *self.world }.reserve_entity();
            unsafe { &mut *self.queue }.push(move |world| {
                if world.contains(entity) {
                    world.entity_mut(entity).insert(bundle);
                }
            });
            EntityCommands {
                entity,
                queue: self.queue,
            }
        }

        pub fn entity(&mut self, entity: Entity) -> EntityCommands {
            EntityCommands {
                entity,
                queue: self.queue,
            }
        }

        pub fn despawn(&mut self, entity: Entity) {
            unsafe { &mut *self.queue }.push(move |world| {
                let _ = world.despawn(entity);
            });
        }

        /// Queues an event to be sent when deferred commands are applied.
        ///
        /// If the matching [`Events<T>`] resource is missing, it is inserted
        /// before the event is sent. The event becomes visible after the
        /// current schedule/stage applies its command queue.
        pub fn send_event<T>(&mut self, event: T)
        where
            T: 'static,
        {
            unsafe { &mut *self.queue }.push(move |world| {
                if !world.contains_resource::<Events<T>>() {
                    world.insert_resource(Events::<T>::new());
                }
                world.resource_mut::<Events<T>>().send(event);
            });
        }

        /// Queues an arbitrary world mutation for the end of the current schedule/stage.
        ///
        /// This is intended for higher-level engine crates that need to expose
        /// domain-specific deferred commands without adding new command queue
        /// types.
        pub fn run<F>(&mut self, command: F)
        where
            F: FnOnce(&mut World) + 'static,
        {
            unsafe { &mut *self.queue }.push(command);
        }

        /// Inserts or replaces a resource when the command queue is applied.
        pub fn insert_resource<T>(&mut self, value: T)
        where
            T: Resource + 'static,
        {
            unsafe { &mut *self.queue }.push(move |world| {
                world.insert_resource(value);
            });
        }

        /// Inserts a default resource value when the resource is missing.
        pub fn init_resource<T>(&mut self)
        where
            T: Resource + Default + 'static,
        {
            unsafe { &mut *self.queue }.push(move |world| {
                world.init_resource::<T>();
            });
        }

        /// Removes a resource when the command queue is applied.
        pub fn remove_resource<T>(&mut self)
        where
            T: Resource + 'static,
        {
            unsafe { &mut *self.queue }.push(move |world| {
                let _ = world.remove_resource::<T>();
            });
        }
    }

    pub struct EntityCommands {
        entity: Entity,
        queue: *mut CommandQueue,
    }

    impl EntityCommands {
        pub fn id(&self) -> Entity {
            self.entity
        }

        pub fn insert<B>(&mut self, bundle: B) -> &mut Self
        where
            B: Bundle + 'static,
        {
            let entity = self.entity;
            unsafe { &mut *self.queue }.push(move |world| {
                if world.contains(entity) {
                    world.entity_mut(entity).insert(bundle);
                }
            });
            self
        }

        pub fn remove<T>(&mut self) -> &mut Self
        where
            T: Component,
        {
            let entity = self.entity;
            unsafe { &mut *self.queue }.push(move |world| {
                if world.contains(entity) {
                    let _ = world.remove::<T>(entity);
                }
            });
            self
        }

        pub fn despawn(&mut self) -> &mut Self {
            let entity = self.entity;
            unsafe { &mut *self.queue }.push(move |world| {
                let _ = world.despawn(entity);
            });
            self
        }
    }

    pub struct Res<T>(*const T, PhantomData<T>);

    impl<T> Deref for Res<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            unsafe { &*self.0 }
        }
    }

    pub struct ResMut<T>(*mut T, PhantomData<T>);

    impl<T> Deref for ResMut<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            unsafe { &*self.0 }
        }
    }

    impl<T> DerefMut for ResMut<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            unsafe { &mut *self.0 }
        }
    }

    pub struct Local<T>(*mut T, PhantomData<T>);

    impl<T> Deref for Local<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            unsafe { &*self.0 }
        }
    }

    impl<T> DerefMut for Local<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            unsafe { &mut *self.0 }
        }
    }

    struct ComponentChangesState<T> {
        last_seen_revision: u64,
        marker: PhantomData<T>,
    }

    impl<T> Default for ComponentChangesState<T> {
        fn default() -> Self {
            Self {
                last_seen_revision: 0,
                marker: PhantomData,
            }
        }
    }

    /// Per-system cursor for observing added and changed components.
    pub struct ComponentChanges<T> {
        world: *mut World,
        since_revision: u64,
        current_revision: u64,
        state: *mut ComponentChangesState<T>,
        marker: PhantomData<T>,
    }

    impl<T: Component> ComponentChanges<T> {
        /// Iterates components inserted since this system last marked revisions seen.
        pub fn added(&mut self) -> impl Iterator<Item = (Entity, &T)> {
            let world = unsafe { &mut *self.world };
            world
                .query::<(Entity, &T)>()
                .iter_added_since(&*world, self.since_revision)
        }

        /// Iterates components changed since this system last marked revisions seen.
        ///
        /// Newly inserted components are also changed because insertion writes
        /// the component's current revision.
        pub fn changed(&mut self) -> impl Iterator<Item = (Entity, &T)> {
            let world = unsafe { &mut *self.world };
            world
                .query::<(Entity, &T)>()
                .iter_changed_since(&*world, self.since_revision)
        }

        /// Iterates added components and marks the current world revision as seen.
        pub fn read_added(&mut self) -> impl Iterator<Item = (Entity, &T)> {
            self.mark_seen();
            let world = unsafe { &mut *self.world };
            world
                .query::<(Entity, &T)>()
                .iter_added_since(&*world, self.since_revision)
        }

        /// Iterates changed components and marks the current world revision as seen.
        pub fn read_changed(&mut self) -> impl Iterator<Item = (Entity, &T)> {
            self.mark_seen();
            let world = unsafe { &mut *self.world };
            world
                .query::<(Entity, &T)>()
                .iter_changed_since(&*world, self.since_revision)
        }

        /// Marks the current world revision as seen by this system.
        pub fn mark_seen(&mut self) {
            unsafe { &mut *self.state }.last_seen_revision = self.current_revision;
        }

        /// Returns the world revision this cursor compares from.
        pub fn last_seen_revision(&self) -> u64 {
            self.since_revision
        }

        /// Returns the current world revision captured for this system run.
        pub fn current_revision(&self) -> u64 {
            self.current_revision
        }

        /// Resets this system cursor so retained changes are visible again.
        pub fn reset(&mut self) {
            unsafe { &mut *self.state }.last_seen_revision = 0;
            self.since_revision = 0;
        }
    }

    struct RemovedComponentsState<T> {
        last_seen_revision: u64,
        marker: PhantomData<T>,
    }

    impl<T> Default for RemovedComponentsState<T> {
        fn default() -> Self {
            Self {
                last_seen_revision: 0,
                marker: PhantomData,
            }
        }
    }

    /// Per-system reader for components removed from entities.
    pub struct RemovedComponents<T> {
        records: Vec<RemovedComponent>,
        latest_revision: u64,
        state: *mut RemovedComponentsState<T>,
    }

    impl<T> RemovedComponents<T> {
        /// Iterates over removals this system has not seen yet and advances its cursor.
        pub fn read(&mut self) -> impl Iterator<Item = RemovedComponent> + '_ {
            unsafe { &mut *self.state }.last_seen_revision = self.latest_revision;
            self.records.iter().copied()
        }

        /// Returns true when no unseen removals are available.
        pub fn is_empty(&self) -> bool {
            self.records.is_empty()
        }

        /// Returns the number of unseen removals available to this system.
        pub fn len(&self) -> usize {
            self.records.len()
        }

        /// Returns the last removal revision this system marked as seen.
        pub fn last_seen_revision(&self) -> u64 {
            unsafe { &*self.state }.last_seen_revision
        }

        /// Resets this system cursor so the next read sees retained removals again.
        pub fn reset(&mut self) {
            unsafe { &mut *self.state }.last_seen_revision = 0;
        }
    }

    struct ResourceCursorState<T> {
        last_seen_revision: u64,
        marker: PhantomData<T>,
    }

    impl<T> Default for ResourceCursorState<T> {
        fn default() -> Self {
            Self {
                last_seen_revision: 0,
                marker: PhantomData,
            }
        }
    }

    /// Per-system cursor for observing resource changes without mutating the resource.
    pub struct ResourceCursor<T> {
        value: *const T,
        current_revision: u64,
        state: *mut ResourceCursorState<T>,
    }

    impl<T> ResourceCursor<T> {
        /// Returns the current resource value.
        pub fn get(&self) -> &T {
            unsafe { &*self.value }
        }

        /// Returns the current resource mutation revision.
        pub fn revision(&self) -> u64 {
            self.current_revision
        }

        /// Returns the last revision this system marked as seen.
        pub fn last_seen_revision(&self) -> u64 {
            unsafe { &*self.state }.last_seen_revision
        }

        /// Returns true when the resource changed since this system last marked it seen.
        pub fn is_changed(&self) -> bool {
            self.current_revision > self.last_seen_revision()
        }

        /// Marks the current revision as seen by this system.
        pub fn mark_seen(&mut self) {
            unsafe { &mut *self.state }.last_seen_revision = self.current_revision;
        }

        /// Returns the resource only when it changed, then marks it as seen.
        pub fn read_if_changed(&mut self) -> Option<&T> {
            if self.is_changed() {
                self.mark_seen();
                Some(self.get())
            } else {
                None
            }
        }

        /// Resets this system cursor so the next read treats the resource as changed.
        pub fn reset(&mut self) {
            unsafe { &mut *self.state }.last_seen_revision = 0;
        }
    }

    /// Read-only access to an [`Events<T>`] resource from a system.
    pub struct EventReader<T>(*const Events<T>, PhantomData<T>);

    impl<T> EventReader<T> {
        /// Iterates over currently queued events without consuming them.
        pub fn iter(&self) -> impl Iterator<Item = &T> {
            unsafe { &*self.0 }.iter()
        }

        /// Returns the number of currently queued events.
        pub fn len(&self) -> usize {
            unsafe { &*self.0 }.len()
        }

        /// Returns `true` when no events are queued.
        pub fn is_empty(&self) -> bool {
            unsafe { &*self.0 }.is_empty()
        }
    }

    struct EventCursorState<T> {
        cursor: usize,
        marker: PhantomData<T>,
    }

    impl<T> Default for EventCursorState<T> {
        fn default() -> Self {
            Self {
                cursor: 0,
                marker: PhantomData,
            }
        }
    }

    /// Incremental, non-consuming access to events this system has not read yet.
    pub struct EventCursor<T> {
        events: *const Events<T>,
        state: *mut EventCursorState<T>,
    }

    impl<T> EventCursor<T> {
        /// Iterates over unread events for this system instance and advances
        /// the cursor to the end of the current buffer.
        pub fn read(&mut self) -> impl Iterator<Item = &T> {
            let events = unsafe { &*self.events };
            let state = unsafe { &mut *self.state };
            let len = events.len();
            let start = if state.cursor > len { 0 } else { state.cursor };
            state.cursor = len;
            events.iter_from(start)
        }

        /// Resets this system's cursor so the next read sees the full buffer.
        pub fn reset(&mut self) {
            unsafe { &mut *self.state }.cursor = 0;
        }

        /// Returns the current cursor position for diagnostics and tests.
        pub fn cursor(&self) -> usize {
            unsafe { &*self.state }.cursor
        }
    }

    /// Write access to an [`Events<T>`] resource from a system.
    pub struct EventWriter<T>(*mut Events<T>, PhantomData<T>);

    impl<T> EventWriter<T> {
        /// Queues one event.
        pub fn send(&mut self, event: T) {
            unsafe { &mut *self.0 }.send(event);
        }

        /// Queues multiple events.
        pub fn extend<I>(&mut self, events: I)
        where
            I: IntoIterator<Item = T>,
        {
            unsafe { &mut *self.0 }.extend(events);
        }
    }

    /// Destructive access to an [`Events<T>`] resource from a system.
    pub struct EventDrain<T>(*mut Events<T>, PhantomData<T>);

    impl<T> EventDrain<T> {
        /// Drains queued events, consuming them.
        pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
            unsafe { &mut *self.0 }.drain()
        }

        /// Removes all queued events without yielding them.
        pub fn clear(&mut self) {
            unsafe { &mut *self.0 }.clear();
        }
    }

    pub struct Query<Q> {
        world: *mut World,
        marker: PhantomData<Q>,
    }

    impl<Q> Query<Q> {
        fn new(world: *mut World) -> Self {
            Self {
                world,
                marker: PhantomData,
            }
        }
    }

    impl<T: Component> Query<&T> {
        pub fn iter(&mut self) -> impl Iterator<Item = &T> {
            let world = unsafe { &mut *self.world };
            world.query::<&T>().iter(&*world)
        }
    }

    impl<T: Component> Query<&mut T> {
        pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
            let world = unsafe { &mut *self.world };
            world.query::<&mut T>().iter_mut(world)
        }
    }

    impl<T: Component> Query<(Entity, &T)> {
        pub fn iter(&mut self) -> impl Iterator<Item = (Entity, &T)> {
            let world = unsafe { &mut *self.world };
            world.query::<(Entity, &T)>().iter(&*world)
        }

        /// Iterates entities whose `T` component was inserted after `revision`.
        pub fn iter_added_since(&mut self, revision: u64) -> impl Iterator<Item = (Entity, &T)> {
            let world = unsafe { &mut *self.world };
            world
                .query::<(Entity, &T)>()
                .iter_added_since(&*world, revision)
        }

        /// Iterates entities whose `T` component changed after `revision`.
        pub fn iter_changed_since(&mut self, revision: u64) -> impl Iterator<Item = (Entity, &T)> {
            let world = unsafe { &mut *self.world };
            world
                .query::<(Entity, &T)>()
                .iter_changed_since(&*world, revision)
        }
    }

    impl<T: Component> Query<(Entity, &mut T)> {
        pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut T)> {
            let world = unsafe { &mut *self.world };
            world.query::<(Entity, &mut T)>().iter_mut(world)
        }
    }

    impl<A: Component, B: Component> Query<(&A, &B)> {
        pub fn iter(&mut self) -> impl Iterator<Item = (&A, &B)> {
            let world = unsafe { &mut *self.world };
            world.query::<(&A, &B)>().iter(&*world)
        }
    }

    impl<A: Component, B: Component> Query<(&mut A, &mut B)> {
        pub fn iter_mut(&mut self) -> impl Iterator<Item = (&mut A, &mut B)> {
            let world = unsafe { &mut *self.world };
            world.query::<(&mut A, &mut B)>().iter_mut(world)
        }
    }

    impl<A: Component, B: Component> Query<(&mut A, &B)> {
        pub fn iter_mut(&mut self) -> impl Iterator<Item = (&mut A, &B)> {
            let world = unsafe { &mut *self.world };
            world.query::<(&mut A, &B)>().iter_mut(world)
        }
    }

    impl<A: Component, B: Component> Query<(&A, &mut B)> {
        pub fn iter_mut(&mut self) -> impl Iterator<Item = (&A, &mut B)> {
            let world = unsafe { &mut *self.world };
            world.query::<(&A, &mut B)>().iter_mut(world)
        }
    }

    #[derive(Default)]
    pub struct SystemLocals {
        values: HashMap<TypeId, Box<dyn Any>>,
    }

    impl SystemLocals {
        fn get_or_init<T>(&mut self) -> *mut T
        where
            T: Default + 'static,
        {
            let value = self
                .values
                .entry(TypeId::of::<T>())
                .or_insert_with(|| Box::<T>::default());
            value
                .downcast_mut::<T>()
                .expect("system local type mismatch") as *mut T
        }
    }

    impl<T: 'static> SystemParam for Res<T> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            _locals: *mut SystemLocals,
        ) -> Self {
            let world = unsafe { &mut *world };
            let value = world.resource::<T>() as *const T;
            Res(value, PhantomData)
        }
    }

    impl<T: 'static> SystemParam for ResMut<T> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            _locals: *mut SystemLocals,
        ) -> Self {
            let world = unsafe { &mut *world };
            let value = world.resource_mut::<T>() as *mut T;
            ResMut(value, PhantomData)
        }
    }

    impl<T: 'static> SystemParam for Option<Res<T>> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            _locals: *mut SystemLocals,
        ) -> Self {
            let world = unsafe { &mut *world };
            world
                .get_resource::<T>()
                .map(|value| Res(value as *const T, PhantomData))
        }
    }

    impl<T: 'static> SystemParam for Option<ResMut<T>> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            _locals: *mut SystemLocals,
        ) -> Self {
            let world = unsafe { &mut *world };
            world
                .get_resource_mut::<T>()
                .map(|value| ResMut(value as *mut T, PhantomData))
        }
    }

    impl<T: Default + 'static> SystemParam for Local<T> {
        unsafe fn fetch(
            _world: *mut World,
            _commands: *mut CommandQueue,
            locals: *mut SystemLocals,
        ) -> Self {
            let value = unsafe { &mut *locals }.get_or_init::<T>();
            Local(value, PhantomData)
        }
    }

    impl<T: Component> SystemParam for ComponentChanges<T> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            locals: *mut SystemLocals,
        ) -> Self {
            let state = unsafe { &mut *locals }.get_or_init::<ComponentChangesState<T>>();
            let since_revision = unsafe { &*state }.last_seen_revision;
            let current_revision = unsafe { &*world }.change_tick();
            ComponentChanges {
                world,
                since_revision,
                current_revision,
                state,
                marker: PhantomData,
            }
        }
    }

    impl<T: 'static> SystemParam for RemovedComponents<T> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            locals: *mut SystemLocals,
        ) -> Self {
            let world = unsafe { &mut *world };
            let state = unsafe { &mut *locals }.get_or_init::<RemovedComponentsState<T>>();
            let last_seen_revision = unsafe { &*state }.last_seen_revision;
            let records: Vec<_> = world
                .removed_components_since::<T>(last_seen_revision)
                .collect();
            let latest_revision = records
                .last()
                .map(|record| record.revision)
                .unwrap_or(last_seen_revision);
            RemovedComponents {
                records,
                latest_revision,
                state,
            }
        }
    }

    impl<T: 'static> SystemParam for ResourceCursor<T> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            locals: *mut SystemLocals,
        ) -> Self {
            let world = unsafe { &mut *world };
            let value = world.resource::<T>() as *const T;
            let current_revision = world.resource_revision::<T>().expect("resource not found");
            let state = unsafe { &mut *locals }.get_or_init::<ResourceCursorState<T>>();
            ResourceCursor {
                value,
                current_revision,
                state,
            }
        }
    }

    impl<T: 'static> SystemParam for Option<ResourceCursor<T>> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            locals: *mut SystemLocals,
        ) -> Self {
            let world = unsafe { &mut *world };
            let value = world.get_resource::<T>()? as *const T;
            let current_revision = world.resource_revision::<T>()?;
            let state = unsafe { &mut *locals }.get_or_init::<ResourceCursorState<T>>();
            Some(ResourceCursor {
                value,
                current_revision,
                state,
            })
        }
    }

    impl<T: 'static> SystemParam for EventReader<T> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            _locals: *mut SystemLocals,
        ) -> Self {
            let world = unsafe { &mut *world };
            let events = world.resource::<Events<T>>() as *const Events<T>;
            EventReader(events, PhantomData)
        }
    }

    impl<T: 'static> SystemParam for EventCursor<T> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            locals: *mut SystemLocals,
        ) -> Self {
            let world = unsafe { &mut *world };
            let events = world.resource::<Events<T>>() as *const Events<T>;
            let state = unsafe { &mut *locals }.get_or_init::<EventCursorState<T>>();
            EventCursor { events, state }
        }
    }

    impl<T: 'static> SystemParam for EventWriter<T> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            _locals: *mut SystemLocals,
        ) -> Self {
            let world = unsafe { &mut *world };
            let events = world.resource_mut::<Events<T>>() as *mut Events<T>;
            EventWriter(events, PhantomData)
        }
    }

    impl<T: 'static> SystemParam for EventDrain<T> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            _locals: *mut SystemLocals,
        ) -> Self {
            let world = unsafe { &mut *world };
            let events = world.resource_mut::<Events<T>>() as *mut Events<T>;
            EventDrain(events, PhantomData)
        }
    }

    impl<Q: 'static> SystemParam for Query<Q> {
        unsafe fn fetch(
            world: *mut World,
            _commands: *mut CommandQueue,
            _locals: *mut SystemLocals,
        ) -> Self {
            Query::new(world)
        }
    }

    impl SystemParam for Commands {
        unsafe fn fetch(
            _world: *mut World,
            commands: *mut CommandQueue,
            _locals: *mut SystemLocals,
        ) -> Self {
            Commands::new(_world, commands)
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct StateTransition<'a, T> {
        pub from: &'a T,
        pub to: &'a T,
        pub revision: u64,
    }

    #[derive(Clone, Debug)]
    pub struct State<T> {
        current: T,
        previous: Option<T>,
        next: Option<T>,
        transition_revision: u64,
    }

    impl<T> State<T> {
        pub fn new(initial: T) -> Self {
            Self {
                current: initial,
                previous: None,
                next: None,
                transition_revision: 0,
            }
        }

        pub fn current(&self) -> &T {
            &self.current
        }

        pub fn previous(&self) -> Option<&T> {
            self.previous.as_ref()
        }

        pub fn next(&self) -> Option<&T> {
            self.next.as_ref()
        }

        pub fn transition_revision(&self) -> u64 {
            self.transition_revision
        }

        pub fn transition(&self) -> Option<StateTransition<'_, T>> {
            self.previous.as_ref().map(|from| StateTransition {
                from,
                to: &self.current,
                revision: self.transition_revision,
            })
        }

        pub fn set(&mut self, value: T) {
            self.previous = Some(std::mem::replace(&mut self.current, value));
            self.next = None;
            self.transition_revision = self.transition_revision.saturating_add(1);
        }

        pub fn set_next(&mut self, value: T) {
            self.next = Some(value);
        }

        pub fn apply_transition(&mut self) -> bool {
            if let Some(next) = self.next.take() {
                self.previous = Some(std::mem::replace(&mut self.current, next));
                self.transition_revision = self.transition_revision.saturating_add(1);
                true
            } else {
                false
            }
        }
    }

    impl<T: 'static> Resource for State<T> {}

    pub struct System {
        run: Box<dyn FnMut(&mut World, &mut CommandQueue, &mut SystemLocals)>,
        run_condition: Option<Box<dyn FnMut(&World) -> bool>>,
        locals: SystemLocals,
    }

    impl System {
        pub fn new<F>(mut run: F) -> Self
        where
            F: FnMut(&mut World, &mut CommandQueue) + 'static,
        {
            Self::new_with_locals(move |world, commands, _locals| run(world, commands))
        }

        fn new_with_locals<F>(run: F) -> Self
        where
            F: FnMut(&mut World, &mut CommandQueue, &mut SystemLocals) + 'static,
        {
            Self {
                run: Box::new(run),
                run_condition: None,
                locals: SystemLocals::default(),
            }
        }

        pub fn with_condition<F>(mut self, condition: F) -> Self
        where
            F: FnMut(&World) -> bool + 'static,
        {
            self.run_condition = Some(Box::new(condition));
            self
        }

        pub fn run(&mut self, world: &mut World, commands: &mut CommandQueue) {
            if let Some(condition) = self.run_condition.as_mut() {
                if !(condition)(&*world) {
                    return;
                }
            }

            (self.run)(world, commands, &mut self.locals);
        }
    }

    pub trait IntoSystem<Marker = ()> {
        fn into_system(self) -> System;
    }

    impl IntoSystem for System {
        fn into_system(self) -> System {
            self
        }
    }

    impl<F> IntoSystem<fn(&mut World)> for F
    where
        F: FnMut(&mut World) + 'static,
    {
        fn into_system(mut self) -> System {
            System::new(move |world, _commands| self(world))
        }
    }

    macro_rules! impl_into_system {
        ($($param:ident),+) => {
            impl<Func, $($param),+> IntoSystem<fn($($param),+)> for Func
            where
                Func: FnMut($($param),+) + 'static,
                $($param: SystemParam + 'static),+
            {
                #[allow(non_snake_case)]
                fn into_system(mut self) -> System {
                    System::new_with_locals(move |world, commands, locals| {
                        let world_ptr = world as *mut World;
                        let commands_ptr = commands as *mut CommandQueue;
                        let locals_ptr = locals as *mut SystemLocals;
                        unsafe {
                            $(let $param = <$param as SystemParam>::fetch(world_ptr, commands_ptr, locals_ptr);)+
                            self($($param),+);
                        }
                    })
                }
            }
        };
    }

    impl_into_system!(A);
    impl_into_system!(A, B);
    impl_into_system!(A, B, C);
    impl_into_system!(A, B, C, D);
    impl_into_system!(A, B, C, D, E);

    pub trait IntoSystemExt<Marker>: IntoSystem<Marker> + Sized {
        fn run_if<C>(self, condition: C) -> System
        where
            C: FnMut(&World) -> bool + 'static,
        {
            self.into_system().with_condition(condition)
        }
    }

    impl<T, Marker> IntoSystemExt<Marker> for T where T: IntoSystem<Marker> + Sized {}

    pub fn in_state<T>(target: T) -> impl FnMut(&World) -> bool
    where
        T: Clone + PartialEq + 'static,
    {
        move |world: &World| world.resource::<State<T>>().current().eq(&target)
    }

    pub fn state_entered<T>(target: T) -> impl FnMut(&World) -> bool
    where
        T: Clone + PartialEq + 'static,
    {
        let mut last_seen_revision = 0;
        move |world: &World| {
            let state = world.resource::<State<T>>();
            let revision = state.transition_revision();
            if revision <= last_seen_revision {
                return false;
            }
            last_seen_revision = revision;
            state.current().eq(&target)
        }
    }

    pub fn state_exited<T>(target: T) -> impl FnMut(&World) -> bool
    where
        T: Clone + PartialEq + 'static,
    {
        let mut last_seen_revision = 0;
        move |world: &World| {
            let state = world.resource::<State<T>>();
            let revision = state.transition_revision();
            if revision <= last_seen_revision {
                return false;
            }
            last_seen_revision = revision;
            state
                .previous()
                .map(|previous| previous.eq(&target))
                .unwrap_or(false)
        }
    }
}

pub mod world {
    use super::component::Component;
    use super::entity::Entity;
    use super::query::{Added, Changed, With, Without};
    use std::any::{Any, TypeId};
    use std::collections::HashMap;
    use std::marker::PhantomData;

    trait StorageDyn: Any {
        fn remove_entity(&mut self, entity: Entity) -> bool;
        fn as_any(&self) -> &dyn Any;
        fn as_any_mut(&mut self) -> &mut dyn Any;
    }

    struct Storage<T: Component> {
        sparse: Vec<Option<usize>>,
        dense_entities: Vec<Entity>,
        dense_data: Vec<T>,
        dense_added_revisions: Vec<u64>,
        dense_revisions: Vec<u64>,
    }

    impl<T: Component> Default for Storage<T> {
        fn default() -> Self {
            Self {
                sparse: Vec::new(),
                dense_entities: Vec::new(),
                dense_data: Vec::new(),
                dense_added_revisions: Vec::new(),
                dense_revisions: Vec::new(),
            }
        }
    }

    impl<T: Component> Storage<T> {
        fn ensure_sparse_capacity(&mut self, entity: Entity) {
            let index = entity.index as usize;
            if self.sparse.len() <= index {
                self.sparse.resize(index + 1, None);
            }
        }

        fn insert(&mut self, entity: Entity, component: T, revision: u64) {
            self.ensure_sparse_capacity(entity);
            let index = entity.index as usize;

            if let Some(dense_index) = self.sparse[index] {
                if self.dense_entities.get(dense_index).copied() == Some(entity) {
                    self.dense_data[dense_index] = component;
                    self.dense_revisions[dense_index] = revision;
                    return;
                }
            }

            let dense_index = self.dense_data.len();
            self.dense_entities.push(entity);
            self.dense_data.push(component);
            self.dense_added_revisions.push(revision);
            self.dense_revisions.push(revision);
            self.sparse[index] = Some(dense_index);
        }

        fn get(&self, entity: Entity) -> Option<&T> {
            let dense_index = self.sparse.get(entity.index as usize).copied().flatten()?;
            if self.dense_entities.get(dense_index).copied() == Some(entity) {
                self.dense_data.get(dense_index)
            } else {
                None
            }
        }

        fn get_mut(&mut self, entity: Entity, revision: u64) -> Option<&mut T> {
            let dense_index = self.sparse.get(entity.index as usize).copied().flatten()?;
            if self.dense_entities.get(dense_index).copied() == Some(entity) {
                self.dense_revisions[dense_index] = revision;
                self.dense_data.get_mut(dense_index)
            } else {
                None
            }
        }

        fn get_mut_ptr(&mut self, entity: Entity, revision: u64) -> Option<*mut T> {
            self.get_mut(entity, revision).map(|value| value as *mut T)
        }

        fn revision(&self, entity: Entity) -> Option<u64> {
            let dense_index = self.sparse.get(entity.index as usize).copied().flatten()?;
            if self.dense_entities.get(dense_index).copied() == Some(entity) {
                self.dense_revisions.get(dense_index).copied()
            } else {
                None
            }
        }

        fn added_revision(&self, entity: Entity) -> Option<u64> {
            let dense_index = self.sparse.get(entity.index as usize).copied().flatten()?;
            if self.dense_entities.get(dense_index).copied() == Some(entity) {
                self.dense_added_revisions.get(dense_index).copied()
            } else {
                None
            }
        }

        fn changed_since(&self, entity: Entity, revision: u64) -> bool {
            self.revision(entity)
                .map(|component_revision| component_revision > revision)
                .unwrap_or(false)
        }

        fn added_since(&self, entity: Entity, revision: u64) -> bool {
            self.added_revision(entity)
                .map(|component_revision| component_revision > revision)
                .unwrap_or(false)
        }

        fn remove(&mut self, entity: Entity) -> Option<T> {
            let index = entity.index as usize;
            let dense_index = self.sparse.get(index).copied().flatten()?;

            if self.dense_entities.get(dense_index).copied() != Some(entity) {
                return None;
            }

            self.sparse[index] = None;

            let removed_entity = self.dense_entities.swap_remove(dense_index);
            let removed_component = self.dense_data.swap_remove(dense_index);
            self.dense_added_revisions.swap_remove(dense_index);
            self.dense_revisions.swap_remove(dense_index);

            debug_assert_eq!(removed_entity, entity);

            if dense_index < self.dense_entities.len() {
                let moved_entity = self.dense_entities[dense_index];
                self.sparse[moved_entity.index as usize] = Some(dense_index);
            }

            Some(removed_component)
        }

        fn entities(&self) -> &[Entity] {
            &self.dense_entities
        }

        fn values(&self) -> impl Iterator<Item = &T> {
            self.dense_data.iter()
        }

        fn values_mut(&mut self, revision: u64) -> impl Iterator<Item = &mut T> {
            for component_revision in &mut self.dense_revisions {
                *component_revision = revision;
            }
            self.dense_data.iter_mut()
        }

        fn contains_entity(&self, entity: Entity) -> bool {
            self.get(entity).is_some()
        }
    }

    impl<T: Component> StorageDyn for Storage<T> {
        fn remove_entity(&mut self, entity: Entity) -> bool {
            self.remove(entity).is_some()
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    pub trait Bundle {
        fn insert_into(self, world: &mut World, entity: Entity);
    }

    impl<C: Component> Bundle for C {
        fn insert_into(self, world: &mut World, entity: Entity) {
            world.insert_component(entity, self);
        }
    }

    macro_rules! impl_bundle_tuple {
        ($($name:ident),+) => {
            impl<$($name: Component),+> Bundle for ($($name,)+) {
                #[allow(non_snake_case)]
                fn insert_into(self, world: &mut World, entity: Entity) {
                    let ($($name,)+) = self;
                    $(world.insert_component(entity, $name);)+
                }
            }
        };
    }

    impl_bundle_tuple!(A, B);
    impl_bundle_tuple!(A, B, C);
    impl_bundle_tuple!(A, B, C, D);
    impl_bundle_tuple!(A, B, C, D, E);

    struct ResourceEntry {
        value: Box<dyn Any>,
        revision: u64,
    }

    /// A record that an entity lost a component at a specific world revision.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct RemovedComponent {
        pub entity: Entity,
        pub revision: u64,
    }

    pub struct World {
        next_index: u32,
        generations: Vec<u32>,
        free_indices: Vec<u32>,
        storages: HashMap<TypeId, Box<dyn StorageDyn>>,
        removed_components: HashMap<TypeId, Vec<RemovedComponent>>,
        resources: HashMap<TypeId, ResourceEntry>,
        non_send_resources: HashMap<TypeId, Box<dyn Any>>,
        change_tick: u64,
    }

    impl Default for World {
        fn default() -> Self {
            Self::new()
        }
    }

    impl World {
        pub fn new() -> Self {
            Self {
                next_index: 0,
                generations: Vec::new(),
                free_indices: Vec::new(),
                storages: HashMap::new(),
                removed_components: HashMap::new(),
                resources: HashMap::new(),
                non_send_resources: HashMap::new(),
                change_tick: 0,
            }
        }

        pub fn spawn<B: Bundle>(&mut self, bundle: B) -> EntityMut<'_> {
            let entity = self.reserve_entity();
            bundle.insert_into(self, entity);
            EntityMut {
                world: self,
                entity,
            }
        }

        /// Allocates an alive entity ID without inserting any components.
        ///
        /// This is primarily used by deferred commands so systems can refer to
        /// newly spawned entities before queued component insertion is applied.
        pub fn reserve_entity(&mut self) -> Entity {
            self.alloc_entity()
        }

        pub fn entity_mut(&mut self, entity: Entity) -> EntityMut<'_> {
            assert!(self.contains(entity), "entity {:?} is not alive", entity);
            EntityMut {
                world: self,
                entity,
            }
        }

        pub fn contains(&self, entity: Entity) -> bool {
            self.generations
                .get(entity.index as usize)
                .map(|g| *g == entity.generation)
                .unwrap_or(false)
        }

        pub fn entity_count(&self) -> usize {
            self.generations
                .len()
                .saturating_sub(self.free_indices.len())
        }

        pub fn resource_count(&self) -> usize {
            self.resources.len()
        }

        /// Returns the current monotonic ECS mutation tick.
        ///
        /// Component inserts/replacements, mutable component access, resource
        /// inserts, and mutable resource access advance this value. Store a
        /// tick from one frame and compare against revisions in a later frame
        /// to invalidate caches without scanning data manually.
        pub fn change_tick(&self) -> u64 {
            self.change_tick
        }

        pub fn despawn(&mut self, entity: Entity) -> bool {
            if !self.contains(entity) {
                return false;
            }
            let revision = self.next_change_tick();
            let mut removed = Vec::new();
            for (type_id, storage) in self.storages.iter_mut() {
                if storage.remove_entity(entity) {
                    removed.push((*type_id, RemovedComponent { entity, revision }));
                }
            }
            for (type_id, record) in removed {
                self.removed_components
                    .entry(type_id)
                    .or_default()
                    .push(record);
            }
            if let Some(generation) = self.generations.get_mut(entity.index as usize) {
                *generation = generation.saturating_add(1);
            }
            self.free_indices.push(entity.index);
            true
        }

        pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
            if !self.contains(entity) {
                return None;
            }
            self.storage::<T>()?.get(entity)
        }

        pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
            if !self.contains(entity) {
                return None;
            }
            if !self
                .storage::<T>()
                .map(|storage| storage.contains_entity(entity))
                .unwrap_or(false)
            {
                return None;
            }
            let revision = self.next_change_tick();
            self.storage_mut::<T>()?.get_mut(entity, revision)
        }

        pub fn remove<T: Component>(&mut self, entity: Entity) -> Option<T> {
            if !self.contains(entity) {
                return None;
            }
            if !self
                .storage::<T>()
                .map(|storage| storage.contains_entity(entity))
                .unwrap_or(false)
            {
                return None;
            }
            let revision = self.next_change_tick();
            let removed = self.storage_mut::<T>()?.remove(entity);
            if removed.is_some() {
                self.record_removed_component::<T>(entity, revision);
            }
            removed
        }

        /// Returns the last mutation revision recorded for `entity`'s `T`.
        pub fn component_revision<T: Component>(&self, entity: Entity) -> Option<u64> {
            if !self.contains(entity) {
                return None;
            }
            self.storage::<T>()?.revision(entity)
        }

        /// Returns the insertion revision recorded for `entity`'s `T`.
        pub fn component_added_revision<T: Component>(&self, entity: Entity) -> Option<u64> {
            if !self.contains(entity) {
                return None;
            }
            self.storage::<T>()?.added_revision(entity)
        }

        /// Returns true when `entity`'s `T` was changed after `revision`.
        pub fn component_changed_since<T: Component>(&self, entity: Entity, revision: u64) -> bool {
            if !self.contains(entity) {
                return false;
            }
            self.storage::<T>()
                .map(|storage| storage.changed_since(entity, revision))
                .unwrap_or(false)
        }

        /// Returns true when `entity`'s `T` was inserted after `revision`.
        pub fn component_added_since<T: Component>(&self, entity: Entity, revision: u64) -> bool {
            if !self.contains(entity) {
                return false;
            }
            self.storage::<T>()
                .map(|storage| storage.added_since(entity, revision))
                .unwrap_or(false)
        }

        /// Iterates removals recorded for component `T`.
        pub fn removed_components<T: 'static>(
            &self,
        ) -> impl Iterator<Item = RemovedComponent> + '_ {
            self.removed_components
                .get(&TypeId::of::<T>())
                .into_iter()
                .flat_map(|records| records.iter().copied())
        }

        /// Iterates removals recorded for component `T` after `revision`.
        pub fn removed_components_since<T: 'static>(
            &self,
            revision: u64,
        ) -> impl Iterator<Item = RemovedComponent> + '_ {
            self.removed_components::<T>()
                .filter(move |record| record.revision > revision)
        }

        /// Clears retained removal records for component `T`.
        pub fn clear_removed_components<T: 'static>(&mut self) {
            self.removed_components.remove(&TypeId::of::<T>());
        }

        /// Drops removal records for component `T` at or before `revision`.
        pub fn prune_removed_components_through<T: 'static>(&mut self, revision: u64) {
            let type_id = TypeId::of::<T>();
            if let Some(records) = self.removed_components.get_mut(&type_id) {
                records.retain(|record| record.revision > revision);
                if records.is_empty() {
                    self.removed_components.remove(&type_id);
                }
            }
        }

        /// Clears retained removal records for every component type.
        pub fn clear_all_removed_components(&mut self) {
            self.removed_components.clear();
        }

        /// Drops removal records for every component type at or before `revision`.
        pub fn prune_all_removed_components_through(&mut self, revision: u64) {
            self.removed_components.retain(|_, records| {
                records.retain(|record| record.revision > revision);
                !records.is_empty()
            });
        }

        pub fn insert_resource<T: 'static>(&mut self, value: T) {
            let revision = self.next_change_tick();
            self.resources.insert(
                TypeId::of::<T>(),
                ResourceEntry {
                    value: Box::new(value),
                    revision,
                },
            );
        }

        pub fn resource<T: 'static>(&self) -> &T {
            self.resources
                .get(&TypeId::of::<T>())
                .and_then(|entry| entry.value.downcast_ref::<T>())
                .expect("resource not found")
        }

        pub fn resource_mut<T: 'static>(&mut self) -> &mut T {
            let revision = self.next_change_tick();
            let entry = self
                .resources
                .get_mut(&TypeId::of::<T>())
                .expect("resource not found");
            entry.revision = revision;
            entry.value.downcast_mut::<T>().expect("resource not found")
        }

        /// Returns a shared resource reference, or `None` when the resource is absent.
        pub fn get_resource<T: 'static>(&self) -> Option<&T> {
            self.resources
                .get(&TypeId::of::<T>())
                .and_then(|entry| entry.value.downcast_ref::<T>())
        }

        /// Returns a mutable resource reference, or `None` when the resource is absent.
        pub fn get_resource_mut<T: 'static>(&mut self) -> Option<&mut T> {
            if !self.resources.contains_key(&TypeId::of::<T>()) {
                return None;
            }
            let revision = self.next_change_tick();
            let entry = self.resources.get_mut(&TypeId::of::<T>())?;
            entry.revision = revision;
            entry.value.downcast_mut::<T>()
        }

        pub fn remove_resource<T: 'static>(&mut self) -> Option<T> {
            let removed = self
                .resources
                .remove(&TypeId::of::<T>())
                .and_then(|entry| entry.value.downcast::<T>().ok())
                .map(|boxed| *boxed);
            if removed.is_some() {
                self.next_change_tick();
            }
            removed
        }

        /// Returns the last mutation revision for resource `T`, if present.
        pub fn resource_revision<T: 'static>(&self) -> Option<u64> {
            self.resources
                .get(&TypeId::of::<T>())
                .map(|entry| entry.revision)
        }

        /// Returns true when resource `T` was changed after `revision`.
        pub fn resource_changed_since<T: 'static>(&self, revision: u64) -> bool {
            self.resource_revision::<T>()
                .map(|resource_revision| resource_revision > revision)
                .unwrap_or(false)
        }

        pub fn init_resource<T: Default + 'static>(&mut self) {
            if !self.resources.contains_key(&TypeId::of::<T>()) {
                self.insert_resource(T::default());
            }
        }

        pub fn contains_resource<T: 'static>(&self) -> bool {
            self.resources.contains_key(&TypeId::of::<T>())
        }

        pub fn insert_non_send_resource<T: 'static>(&mut self, value: T) {
            self.non_send_resources
                .insert(TypeId::of::<T>(), Box::new(value));
        }

        pub fn get_non_send_resource<T: 'static>(&self) -> Option<&T> {
            self.non_send_resources
                .get(&TypeId::of::<T>())
                .and_then(|boxed| boxed.downcast_ref::<T>())
        }

        pub fn get_non_send_resource_mut<T: 'static>(&mut self) -> Option<&mut T> {
            self.non_send_resources
                .get_mut(&TypeId::of::<T>())
                .and_then(|boxed| boxed.downcast_mut::<T>())
        }

        pub fn remove_non_send_resource<T: 'static>(&mut self) -> Option<T> {
            self.non_send_resources
                .remove(&TypeId::of::<T>())
                .and_then(|boxed| boxed.downcast::<T>().ok())
                .map(|boxed| *boxed)
        }

        pub fn query<Q>(&mut self) -> QueryState<Q> {
            QueryState {
                marker: PhantomData,
            }
        }

        pub fn query_filtered<Q, F>(&mut self) -> FilteredQueryState<Q, F> {
            FilteredQueryState {
                marker: PhantomData,
            }
        }

        fn alloc_entity(&mut self) -> Entity {
            if let Some(index) = self.free_indices.pop() {
                let generation = self.generations[index as usize];
                return Entity { index, generation };
            }

            let index = self.next_index;
            self.next_index += 1;
            self.generations.push(0);
            Entity {
                index,
                generation: 0,
            }
        }

        fn insert_component<T: Component>(&mut self, entity: Entity, component: T) {
            assert!(self.contains(entity), "entity {:?} is not alive", entity);
            let revision = self.next_change_tick();
            self.ensure_storage::<T>()
                .insert(entity, component, revision);
        }

        fn next_change_tick(&mut self) -> u64 {
            self.change_tick = self.change_tick.saturating_add(1);
            self.change_tick
        }

        fn record_removed_component<T: 'static>(&mut self, entity: Entity, revision: u64) {
            self.removed_components
                .entry(TypeId::of::<T>())
                .or_default()
                .push(RemovedComponent { entity, revision });
        }

        fn ensure_storage<T: Component>(&mut self) -> &mut Storage<T> {
            let type_id = TypeId::of::<T>();
            self.storages
                .entry(type_id)
                .or_insert_with(|| Box::new(Storage::<T>::default()));
            self.storage_mut::<T>().expect("storage created")
        }

        fn storage<T: Component>(&self) -> Option<&Storage<T>> {
            self.storages
                .get(&TypeId::of::<T>())
                .and_then(|storage| storage.as_any().downcast_ref::<Storage<T>>())
        }

        fn storage_mut<T: Component>(&mut self) -> Option<&mut Storage<T>> {
            self.storages
                .get_mut(&TypeId::of::<T>())
                .and_then(|storage| storage.as_any_mut().downcast_mut::<Storage<T>>())
        }
    }

    pub struct EntityMut<'w> {
        world: &'w mut World,
        entity: Entity,
    }

    impl<'w> EntityMut<'w> {
        pub fn id(&self) -> Entity {
            self.entity
        }

        pub fn insert<B: Bundle>(&mut self, bundle: B) -> &mut Self {
            bundle.insert_into(self.world, self.entity);
            self
        }

        pub fn get<T: Component>(&self) -> Option<&T> {
            self.world.get::<T>(self.entity)
        }

        pub fn get_mut<T: Component>(&mut self) -> Option<&mut T> {
            self.world.get_mut::<T>(self.entity)
        }

        pub fn remove<T: Component>(&mut self) -> Option<T> {
            self.world.remove::<T>(self.entity)
        }
    }

    pub struct QueryState<Q> {
        marker: PhantomData<Q>,
    }

    impl<T: Component> QueryState<&T> {
        pub fn iter<'w>(&mut self, world: &'w World) -> impl Iterator<Item = &'w T> {
            world
                .storage::<T>()
                .map(|storage| storage.values())
                .into_iter()
                .flatten()
        }
    }

    impl<T: Component> QueryState<&mut T> {
        pub fn iter_mut<'w>(&mut self, world: &'w mut World) -> impl Iterator<Item = &'w mut T> {
            let revision = world.next_change_tick();
            world
                .storage_mut::<T>()
                .map(|storage| storage.values_mut(revision))
                .into_iter()
                .flatten()
        }
    }

    pub struct EntityRefIter<'w, T: Component> {
        entities: Vec<Entity>,
        index: usize,
        storage: Option<&'w Storage<T>>,
    }

    impl<'w, T: Component> Iterator for EntityRefIter<'w, T> {
        type Item = (Entity, &'w T);

        fn next(&mut self) -> Option<Self::Item> {
            let storage = self.storage?;
            while self.index < self.entities.len() {
                let entity = self.entities[self.index];
                self.index += 1;
                if let Some(component) = storage.get(entity) {
                    return Some((entity, component));
                }
            }
            None
        }
    }

    pub struct ChangedEntityRefIter<'w, T: Component> {
        entities: Vec<Entity>,
        index: usize,
        revision: u64,
        storage: Option<&'w Storage<T>>,
    }

    pub struct AddedEntityRefIter<'w, T: Component> {
        entities: Vec<Entity>,
        index: usize,
        revision: u64,
        storage: Option<&'w Storage<T>>,
    }

    impl<'w, T: Component> Iterator for AddedEntityRefIter<'w, T> {
        type Item = (Entity, &'w T);

        fn next(&mut self) -> Option<Self::Item> {
            let storage = self.storage?;
            while self.index < self.entities.len() {
                let entity = self.entities[self.index];
                self.index += 1;
                if storage.added_since(entity, self.revision) {
                    if let Some(component) = storage.get(entity) {
                        return Some((entity, component));
                    }
                }
            }
            None
        }
    }

    impl<'w, T: Component> Iterator for ChangedEntityRefIter<'w, T> {
        type Item = (Entity, &'w T);

        fn next(&mut self) -> Option<Self::Item> {
            let storage = self.storage?;
            while self.index < self.entities.len() {
                let entity = self.entities[self.index];
                self.index += 1;
                if storage.changed_since(entity, self.revision) {
                    if let Some(component) = storage.get(entity) {
                        return Some((entity, component));
                    }
                }
            }
            None
        }
    }

    pub struct EntityMutIter<'w, T: Component> {
        entities: Vec<Entity>,
        index: usize,
        storage: *mut Storage<T>,
        revision: u64,
        marker: PhantomData<&'w mut T>,
    }

    impl<'w, T: Component> Iterator for EntityMutIter<'w, T> {
        type Item = (Entity, &'w mut T);

        fn next(&mut self) -> Option<Self::Item> {
            if self.storage.is_null() {
                return None;
            }

            while self.index < self.entities.len() {
                let entity = self.entities[self.index];
                self.index += 1;
                unsafe {
                    let storage = &mut *self.storage;
                    let component_ptr = match storage.get_mut_ptr(entity, self.revision) {
                        Some(value) => value,
                        None => continue,
                    };
                    return Some((entity, &mut *component_ptr));
                }
            }

            None
        }
    }

    impl<T: Component> QueryState<(Entity, &T)> {
        pub fn iter<'w>(&mut self, world: &'w World) -> EntityRefIter<'w, T> {
            let storage = world.storage::<T>();
            let entities = storage
                .map(|component_storage| component_storage.entities().to_vec())
                .unwrap_or_default();

            EntityRefIter {
                entities,
                index: 0,
                storage,
            }
        }

        /// Iterates entities whose `T` component was inserted after `revision`.
        pub fn iter_added_since<'w>(
            &mut self,
            world: &'w World,
            revision: u64,
        ) -> AddedEntityRefIter<'w, T> {
            let storage = world.storage::<T>();
            let entities = storage
                .map(|component_storage| component_storage.entities().to_vec())
                .unwrap_or_default();

            AddedEntityRefIter {
                entities,
                index: 0,
                revision,
                storage,
            }
        }

        /// Iterates entities whose `T` component changed after `revision`.
        pub fn iter_changed_since<'w>(
            &mut self,
            world: &'w World,
            revision: u64,
        ) -> ChangedEntityRefIter<'w, T> {
            let storage = world.storage::<T>();
            let entities = storage
                .map(|component_storage| component_storage.entities().to_vec())
                .unwrap_or_default();

            ChangedEntityRefIter {
                entities,
                index: 0,
                revision,
                storage,
            }
        }
    }

    impl<T: Component> QueryState<(Entity, &mut T)> {
        pub fn iter_mut<'w>(&mut self, world: &'w mut World) -> EntityMutIter<'w, T> {
            let revision = world.next_change_tick();
            let storage = match world.storages.get_mut(&TypeId::of::<T>()) {
                Some(storage) => storage
                    .as_any_mut()
                    .downcast_mut::<Storage<T>>()
                    .expect("storage type mismatch")
                    as *mut Storage<T>,
                None => std::ptr::null_mut(),
            };

            let entities = if storage.is_null() {
                Vec::new()
            } else {
                unsafe { (&*storage).entities().to_vec() }
            };

            EntityMutIter {
                entities,
                index: 0,
                storage,
                revision,
                marker: PhantomData,
            }
        }
    }

    pub struct TupleIter2<'w, A: Component, B: Component> {
        entities: Vec<Entity>,
        index: usize,
        a: Option<&'w Storage<A>>,
        b: Option<&'w Storage<B>>,
    }

    impl<'w, A: Component, B: Component> Iterator for TupleIter2<'w, A, B> {
        type Item = (&'w A, &'w B);

        fn next(&mut self) -> Option<Self::Item> {
            let (Some(a_storage), Some(b_storage)) = (self.a, self.b) else {
                return None;
            };

            while self.index < self.entities.len() {
                let entity = self.entities[self.index];
                self.index += 1;
                if let (Some(a), Some(b)) = (a_storage.get(entity), b_storage.get(entity)) {
                    return Some((a, b));
                }
            }
            None
        }
    }

    pub struct TupleIterMut2<'w, A: Component, B: Component> {
        entities: Vec<Entity>,
        index: usize,
        a: *mut Storage<A>,
        b: *mut Storage<B>,
        revision: u64,
        marker: PhantomData<&'w mut (A, B)>,
    }

    impl<'w, A: Component, B: Component> Iterator for TupleIterMut2<'w, A, B> {
        type Item = (&'w mut A, &'w mut B);

        fn next(&mut self) -> Option<Self::Item> {
            if self.a.is_null() || self.b.is_null() {
                return None;
            }

            while self.index < self.entities.len() {
                let entity = self.entities[self.index];
                self.index += 1;
                unsafe {
                    let a_storage = &mut *self.a;
                    let b_storage = &mut *self.b;
                    let a_ptr = match a_storage.get_mut_ptr(entity, self.revision) {
                        Some(value) => value,
                        None => continue,
                    };
                    let b_ptr = match b_storage.get_mut_ptr(entity, self.revision) {
                        Some(value) => value,
                        None => continue,
                    };
                    return Some((&mut *a_ptr, &mut *b_ptr));
                }
            }
            None
        }
    }

    impl<A: Component, B: Component> QueryState<(&A, &B)> {
        pub fn iter<'w>(&mut self, world: &'w World) -> TupleIter2<'w, A, B> {
            let a_storage = world.storage::<A>();
            let b_storage = world.storage::<B>();

            let entities = a_storage
                .map(|storage| storage.entities().to_vec())
                .unwrap_or_default();

            TupleIter2 {
                entities,
                index: 0,
                a: a_storage,
                b: b_storage,
            }
        }
    }

    impl<A: Component, B: Component> QueryState<(&mut A, &mut B)> {
        pub fn iter_mut<'w>(&mut self, world: &'w mut World) -> TupleIterMut2<'w, A, B> {
            assert_ne!(
                TypeId::of::<A>(),
                TypeId::of::<B>(),
                "duplicate mutable query type"
            );

            let a_type = TypeId::of::<A>();
            let b_type = TypeId::of::<B>();
            let revision = world.next_change_tick();

            let a_storage = {
                let Some(a_dyn) = world.storages.get_mut(&a_type) else {
                    return TupleIterMut2 {
                        entities: Vec::new(),
                        index: 0,
                        a: std::ptr::null_mut(),
                        b: std::ptr::null_mut(),
                        revision,
                        marker: PhantomData,
                    };
                };

                a_dyn
                    .as_any_mut()
                    .downcast_mut::<Storage<A>>()
                    .expect("storage type mismatch") as *mut Storage<A>
            };

            let b_storage = {
                let Some(b_dyn) = world.storages.get_mut(&b_type) else {
                    return TupleIterMut2 {
                        entities: Vec::new(),
                        index: 0,
                        a: std::ptr::null_mut(),
                        b: std::ptr::null_mut(),
                        revision,
                        marker: PhantomData,
                    };
                };

                b_dyn
                    .as_any_mut()
                    .downcast_mut::<Storage<B>>()
                    .expect("storage type mismatch") as *mut Storage<B>
            };

            if a_storage.is_null() || b_storage.is_null() {
                return TupleIterMut2 {
                    entities: Vec::new(),
                    index: 0,
                    a: std::ptr::null_mut(),
                    b: std::ptr::null_mut(),
                    revision,
                    marker: PhantomData,
                };
            }

            let entities = unsafe { (&*a_storage).entities().to_vec() };

            TupleIterMut2 {
                entities,
                index: 0,
                a: a_storage,
                b: b_storage,
                revision,
                marker: PhantomData,
            }
        }
    }

    pub struct TupleIterMutRef2<'w, A: Component, B: Component> {
        entities: Vec<Entity>,
        index: usize,
        a: *mut Storage<A>,
        b: *const Storage<B>,
        revision: u64,
        marker: PhantomData<(&'w mut A, &'w B)>,
    }

    impl<'w, A: Component, B: Component> Iterator for TupleIterMutRef2<'w, A, B> {
        type Item = (&'w mut A, &'w B);

        fn next(&mut self) -> Option<Self::Item> {
            if self.a.is_null() || self.b.is_null() {
                return None;
            }

            while self.index < self.entities.len() {
                let entity = self.entities[self.index];
                self.index += 1;
                unsafe {
                    let a_storage = &mut *self.a;
                    let b_storage = &*self.b;
                    let a_ptr = match a_storage.get_mut_ptr(entity, self.revision) {
                        Some(value) => value,
                        None => continue,
                    };
                    let b_ref = match b_storage.get(entity) {
                        Some(value) => value,
                        None => continue,
                    };
                    return Some((&mut *a_ptr, b_ref));
                }
            }
            None
        }
    }

    impl<A: Component, B: Component> QueryState<(&mut A, &B)> {
        pub fn iter_mut<'w>(&mut self, world: &'w mut World) -> TupleIterMutRef2<'w, A, B> {
            assert_ne!(
                TypeId::of::<A>(),
                TypeId::of::<B>(),
                "mixed mutable/immutable query cannot use the same component type"
            );

            let a_type = TypeId::of::<A>();
            let b_type = TypeId::of::<B>();
            let revision = world.next_change_tick();

            let a_storage = {
                let Some(a_dyn) = world.storages.get_mut(&a_type) else {
                    return TupleIterMutRef2 {
                        entities: Vec::new(),
                        index: 0,
                        a: std::ptr::null_mut(),
                        b: std::ptr::null(),
                        revision,
                        marker: PhantomData,
                    };
                };

                a_dyn
                    .as_any_mut()
                    .downcast_mut::<Storage<A>>()
                    .expect("storage type mismatch") as *mut Storage<A>
            };

            let b_storage = {
                let Some(b_dyn) = world.storages.get(&b_type) else {
                    return TupleIterMutRef2 {
                        entities: Vec::new(),
                        index: 0,
                        a: std::ptr::null_mut(),
                        b: std::ptr::null(),
                        revision,
                        marker: PhantomData,
                    };
                };

                b_dyn
                    .as_any()
                    .downcast_ref::<Storage<B>>()
                    .expect("storage type mismatch") as *const Storage<B>
            };

            let entities = unsafe { (&*a_storage).entities().to_vec() };

            TupleIterMutRef2 {
                entities,
                index: 0,
                a: a_storage,
                b: b_storage,
                revision,
                marker: PhantomData,
            }
        }
    }

    pub struct TupleIterRefMut2<'w, A: Component, B: Component> {
        entities: Vec<Entity>,
        index: usize,
        a: *const Storage<A>,
        b: *mut Storage<B>,
        revision: u64,
        marker: PhantomData<(&'w A, &'w mut B)>,
    }

    impl<'w, A: Component, B: Component> Iterator for TupleIterRefMut2<'w, A, B> {
        type Item = (&'w A, &'w mut B);

        fn next(&mut self) -> Option<Self::Item> {
            if self.a.is_null() || self.b.is_null() {
                return None;
            }

            while self.index < self.entities.len() {
                let entity = self.entities[self.index];
                self.index += 1;
                unsafe {
                    let a_storage = &*self.a;
                    let b_storage = &mut *self.b;
                    let a_ref = match a_storage.get(entity) {
                        Some(value) => value,
                        None => continue,
                    };
                    let b_ptr = match b_storage.get_mut_ptr(entity, self.revision) {
                        Some(value) => value,
                        None => continue,
                    };
                    return Some((a_ref, &mut *b_ptr));
                }
            }
            None
        }
    }

    impl<A: Component, B: Component> QueryState<(&A, &mut B)> {
        pub fn iter_mut<'w>(&mut self, world: &'w mut World) -> TupleIterRefMut2<'w, A, B> {
            assert_ne!(
                TypeId::of::<A>(),
                TypeId::of::<B>(),
                "mixed immutable/mutable query cannot use the same component type"
            );

            let a_type = TypeId::of::<A>();
            let b_type = TypeId::of::<B>();
            let revision = world.next_change_tick();

            let a_storage = {
                let Some(a_dyn) = world.storages.get(&a_type) else {
                    return TupleIterRefMut2 {
                        entities: Vec::new(),
                        index: 0,
                        a: std::ptr::null(),
                        b: std::ptr::null_mut(),
                        revision,
                        marker: PhantomData,
                    };
                };

                a_dyn
                    .as_any()
                    .downcast_ref::<Storage<A>>()
                    .expect("storage type mismatch") as *const Storage<A>
            };

            let b_storage = {
                let Some(b_dyn) = world.storages.get_mut(&b_type) else {
                    return TupleIterRefMut2 {
                        entities: Vec::new(),
                        index: 0,
                        a: std::ptr::null(),
                        b: std::ptr::null_mut(),
                        revision,
                        marker: PhantomData,
                    };
                };

                b_dyn
                    .as_any_mut()
                    .downcast_mut::<Storage<B>>()
                    .expect("storage type mismatch") as *mut Storage<B>
            };

            let entities = unsafe { (&*a_storage).entities().to_vec() };

            TupleIterRefMut2 {
                entities,
                index: 0,
                a: a_storage,
                b: b_storage,
                revision,
                marker: PhantomData,
            }
        }
    }

    pub struct FilteredQueryState<Q, F> {
        marker: PhantomData<(Q, F)>,
    }

    pub struct EntityWithWithoutIter {
        entities: Vec<Entity>,
        index: usize,
    }

    impl Iterator for EntityWithWithoutIter {
        type Item = Entity;

        fn next(&mut self) -> Option<Self::Item> {
            if self.index >= self.entities.len() {
                return None;
            }
            let item = self.entities[self.index];
            self.index += 1;
            Some(item)
        }
    }

    impl<T: Component, U: Component> FilteredQueryState<Entity, (With<T>, Without<U>)> {
        pub fn iter(&mut self, world: &World) -> EntityWithWithoutIter {
            let mut entities = Vec::new();
            if let Some(with_storage) = world.storage::<T>() {
                for entity in with_storage.entities().iter().copied() {
                    let has_without = world
                        .storage::<U>()
                        .map(|storage| storage.contains_entity(entity))
                        .unwrap_or(false);
                    if !has_without {
                        entities.push(entity);
                    }
                }
            }
            EntityWithWithoutIter { entities, index: 0 }
        }
    }

    impl<T: Component> FilteredQueryState<Entity, Changed<T>> {
        /// Iterates entities whose `T` component changed after `revision`.
        pub fn iter_since(&mut self, world: &World, revision: u64) -> EntityWithWithoutIter {
            let mut entities = Vec::new();
            if let Some(storage) = world.storage::<T>() {
                for entity in storage.entities().iter().copied() {
                    if storage.changed_since(entity, revision) {
                        entities.push(entity);
                    }
                }
            }
            EntityWithWithoutIter { entities, index: 0 }
        }
    }

    impl<T: Component> FilteredQueryState<Entity, Added<T>> {
        /// Iterates entities whose `T` component was inserted after `revision`.
        pub fn iter_since(&mut self, world: &World, revision: u64) -> EntityWithWithoutIter {
            let mut entities = Vec::new();
            if let Some(storage) = world.storage::<T>() {
                for entity in storage.entities().iter().copied() {
                    if storage.added_since(entity, revision) {
                        entities.push(entity);
                    }
                }
            }
            EntityWithWithoutIter { entities, index: 0 }
        }
    }
}

pub mod prelude {
    pub use crate::component::Component;
    pub use crate::entity::Entity;
    pub use crate::event::Events;
    pub use crate::query::{Added, Changed, With, Without};
    pub use crate::resource::Resource;
    pub use crate::schedule::{Schedule, ScheduleLabel, ScheduleOrderDiagnostic};
    pub use crate::system::{
        in_state, state_entered, state_exited, CommandQueue, Commands, ComponentChanges,
        EventCursor, EventDrain, EventReader, EventWriter, IntoSystem, IntoSystemExt, Local, Query,
        RemovedComponents, Res, ResMut, ResourceCursor, State, StateTransition, System,
        SystemParam,
    };
    pub use crate::world::{RemovedComponent, World};
}

#[cfg(test)]
mod tests {
    use crate::event::Events;
    use crate::query::{Added, Changed, With, Without};
    use crate::schedule::Schedule;
    use crate::system::{
        in_state, state_entered, state_exited, CommandQueue, Commands, ComponentChanges,
        EventCursor, EventDrain, EventReader, EventWriter, IntoSystem, IntoSystemExt, Local, Query,
        RemovedComponents, Res, ResMut, ResourceCursor, State,
    };
    use crate::world::World;
    use crate::{Component, Resource};

    #[derive(Component, Debug, PartialEq)]
    struct Position(i32);

    #[derive(Component, Debug, PartialEq)]
    struct Velocity(i32);

    #[derive(Resource, Default)]
    struct Tick(u64);

    #[derive(Resource, Default)]
    struct Seen {
        optional_tick: Option<u64>,
        mutated: bool,
    }

    #[derive(Resource, Default)]
    struct EventStats {
        read_sum: i32,
        drained_sum: i32,
        cursor_sum: i32,
        cursor_reads: u64,
    }

    #[derive(Resource, Default)]
    struct ResourceCursorStats {
        changed: bool,
        optional_present: bool,
        last_seen_before: u64,
        last_seen_after: u64,
        values: Vec<u64>,
    }

    #[derive(Resource, Default)]
    struct RemovedStats {
        entities: Vec<crate::entity::Entity>,
        revisions: Vec<u64>,
        reads: u64,
        last_seen_before: u64,
        last_seen_after: u64,
    }

    #[derive(Resource, Default)]
    struct ComponentChangeStats {
        added: Vec<i32>,
        changed: Vec<i32>,
        reads: u64,
        last_seen_before: u64,
        current_revision: u64,
    }

    #[derive(Resource, Default)]
    struct ScheduleTrace(Vec<&'static str>);

    #[derive(Resource, Default)]
    struct LastSpawned(Option<crate::entity::Entity>);

    #[test]
    fn spawn_insert_remove_and_despawn_work() {
        let mut world = World::new();
        let entity = world.spawn((Position(1), Velocity(2))).id();

        assert_eq!(world.get::<Position>(entity).map(|v| v.0), Some(1));
        assert_eq!(world.get::<Velocity>(entity).map(|v| v.0), Some(2));

        let removed = world.remove::<Velocity>(entity);
        assert_eq!(removed.map(|v| v.0), Some(2));
        assert!(world.get::<Velocity>(entity).is_none());

        assert!(world.despawn(entity));
        assert!(!world.contains(entity));
        assert!(world.get::<Position>(entity).is_none());
    }

    #[test]
    fn component_removals_are_recorded_for_remove_and_despawn() {
        let mut world = World::new();
        let first = world.spawn((Position(1), Velocity(1))).id();
        let second = world.spawn((Position(2), Velocity(2))).id();
        let checkpoint = world.change_tick();

        assert_eq!(world.remove::<Position>(first), Some(Position(1)));
        let direct_removals: Vec<_> = world
            .removed_components_since::<Position>(checkpoint)
            .collect();
        assert_eq!(direct_removals.len(), 1);
        assert_eq!(direct_removals[0].entity, first);
        assert!(direct_removals[0].revision > checkpoint);

        assert!(world.despawn(second));
        let position_removals: Vec<_> = world.removed_components::<Position>().collect();
        let velocity_removals: Vec<_> = world.removed_components::<Velocity>().collect();
        assert_eq!(
            position_removals
                .iter()
                .map(|record| record.entity)
                .collect::<Vec<_>>(),
            vec![first, second]
        );
        assert_eq!(velocity_removals.len(), 1);
        assert_eq!(velocity_removals[0].entity, second);

        world.clear_removed_components::<Position>();
        assert_eq!(world.removed_components::<Position>().count(), 0);
        assert_eq!(world.removed_components::<Velocity>().count(), 1);
        world.clear_all_removed_components();
        assert_eq!(world.removed_components::<Velocity>().count(), 0);
    }

    #[test]
    fn component_removal_records_can_be_pruned_by_revision() {
        let mut world = World::new();
        let first = world.spawn((Position(1), Velocity(1))).id();
        let second = world.spawn((Position(2), Velocity(2))).id();
        let third = world.spawn((Position(3), Velocity(3))).id();

        assert_eq!(world.remove::<Position>(first), Some(Position(1)));
        let first_revision = world
            .removed_components::<Position>()
            .last()
            .map(|record| record.revision)
            .unwrap();
        assert_eq!(world.remove::<Position>(second), Some(Position(2)));
        let second_revision = world
            .removed_components::<Position>()
            .last()
            .map(|record| record.revision)
            .unwrap();
        assert!(world.despawn(third));

        world.prune_removed_components_through::<Position>(first_revision);
        let position_removals: Vec<_> = world.removed_components::<Position>().collect();
        assert_eq!(
            position_removals
                .iter()
                .map(|record| record.entity)
                .collect::<Vec<_>>(),
            vec![second, third]
        );

        world.prune_all_removed_components_through(second_revision);
        let position_removals: Vec<_> = world.removed_components::<Position>().collect();
        let velocity_removals: Vec<_> = world.removed_components::<Velocity>().collect();
        assert_eq!(
            position_removals
                .iter()
                .map(|record| record.entity)
                .collect::<Vec<_>>(),
            vec![third]
        );
        assert_eq!(velocity_removals.len(), 1);
        assert_eq!(velocity_removals[0].entity, third);
    }

    #[test]
    fn resources_and_non_send_resources_work() {
        let mut world = World::new();
        world.init_resource::<Tick>();
        world.resource_mut::<Tick>().0 = 7;
        assert_eq!(world.resource::<Tick>().0, 7);
        assert_eq!(world.get_resource::<Tick>().map(|tick| tick.0), Some(7));
        assert_eq!(world.get_resource::<Seen>().map(|seen| seen.mutated), None);
        world.get_resource_mut::<Tick>().unwrap().0 = 8;
        assert_eq!(world.resource::<Tick>().0, 8);
        assert_eq!(world.remove_resource::<Tick>().map(|tick| tick.0), Some(8));
        assert!(!world.contains_resource::<Tick>());

        world.insert_non_send_resource(String::from("watcher"));
        assert_eq!(
            world.get_non_send_resource::<String>().map(String::as_str),
            Some("watcher")
        );
        assert_eq!(
            world.remove_non_send_resource::<String>().as_deref(),
            Some("watcher")
        );
        assert!(world.get_non_send_resource::<String>().is_none());
    }

    #[test]
    fn resources_track_mutation_revisions() {
        let mut world = World::new();
        assert_eq!(world.change_tick(), 0);
        assert_eq!(world.resource_revision::<Tick>(), None);

        world.insert_resource(Tick(1));
        let inserted_revision = world.resource_revision::<Tick>().unwrap();
        assert_eq!(inserted_revision, world.change_tick());
        assert!(world.resource_changed_since::<Tick>(0));
        assert!(!world.resource_changed_since::<Tick>(inserted_revision));

        world.resource_mut::<Tick>().0 = 2;
        let mutated_revision = world.resource_revision::<Tick>().unwrap();
        assert!(mutated_revision > inserted_revision);
        assert!(world.resource_changed_since::<Tick>(inserted_revision));

        let before_missing_lookup = world.change_tick();
        assert!(world.get_resource_mut::<Seen>().is_none());
        assert_eq!(world.change_tick(), before_missing_lookup);
    }

    fn observe_tick_changes(
        mut tick: ResourceCursor<Tick>,
        mut stats: ResMut<ResourceCursorStats>,
    ) {
        stats.changed = tick.is_changed();
        stats.last_seen_before = tick.last_seen_revision();
        if let Some(tick) = tick.read_if_changed() {
            stats.values.push(tick.0);
        }
        stats.last_seen_after = tick.last_seen_revision();
    }

    fn observe_optional_tick_changes(
        tick: Option<ResourceCursor<Tick>>,
        mut stats: ResMut<ResourceCursorStats>,
    ) {
        stats.optional_present = tick.is_some();
    }

    #[test]
    fn resource_cursor_reads_changes_once_per_system_instance() {
        let mut world = World::new();
        world.insert_resource(Tick(1));
        world.insert_resource(ResourceCursorStats::default());
        let first_revision = world.resource_revision::<Tick>().unwrap();
        let mut queue = CommandQueue::new();
        let mut system = observe_tick_changes.into_system();

        system.run(&mut world, &mut queue);
        let stats = world.resource::<ResourceCursorStats>();
        assert!(stats.changed);
        assert_eq!(stats.last_seen_before, 0);
        assert_eq!(stats.last_seen_after, first_revision);
        assert_eq!(stats.values, vec![1]);

        system.run(&mut world, &mut queue);
        let stats = world.resource::<ResourceCursorStats>();
        assert!(!stats.changed);
        assert_eq!(stats.values, vec![1]);

        world.resource_mut::<Tick>().0 = 2;
        let second_revision = world.resource_revision::<Tick>().unwrap();
        system.run(&mut world, &mut queue);
        let stats = world.resource::<ResourceCursorStats>();
        assert!(stats.changed);
        assert_eq!(stats.last_seen_before, first_revision);
        assert_eq!(stats.last_seen_after, second_revision);
        assert_eq!(stats.values, vec![1, 2]);
    }

    #[test]
    fn optional_resource_cursor_is_none_when_resource_is_missing() {
        let mut world = World::new();
        world.insert_resource(ResourceCursorStats::default());
        let mut queue = CommandQueue::new();

        observe_optional_tick_changes
            .into_system()
            .run(&mut world, &mut queue);
        assert!(!world.resource::<ResourceCursorStats>().optional_present);

        world.insert_resource(Tick(4));
        observe_optional_tick_changes
            .into_system()
            .run(&mut world, &mut queue);
        assert!(world.resource::<ResourceCursorStats>().optional_present);
    }

    fn collect_removed_positions(
        mut removed: RemovedComponents<Position>,
        mut stats: ResMut<RemovedStats>,
    ) {
        stats.reads += 1;
        stats.last_seen_before = removed.last_seen_revision();
        for record in removed.read() {
            stats.entities.push(record.entity);
            stats.revisions.push(record.revision);
        }
        stats.last_seen_after = removed.last_seen_revision();
    }

    #[test]
    fn removed_components_reader_tracks_unseen_removals_per_system() {
        let mut world = World::new();
        let first = world.spawn(Position(1)).id();
        let second = world.spawn(Position(2)).id();
        world.insert_resource(RemovedStats::default());
        let mut queue = CommandQueue::new();
        let mut system = collect_removed_positions.into_system();

        assert_eq!(world.remove::<Position>(first), Some(Position(1)));
        let first_revision = world
            .removed_components::<Position>()
            .last()
            .map(|record| record.revision)
            .unwrap();
        system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<RemovedStats>().entities, vec![first]);
        assert_eq!(world.resource::<RemovedStats>().last_seen_before, 0);
        assert_eq!(
            world.resource::<RemovedStats>().last_seen_after,
            first_revision
        );

        system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<RemovedStats>().entities, vec![first]);
        assert_eq!(world.resource::<RemovedStats>().reads, 2);

        assert_eq!(world.remove::<Position>(second), Some(Position(2)));
        let second_revision = world
            .removed_components::<Position>()
            .last()
            .map(|record| record.revision)
            .unwrap();
        system.run(&mut world, &mut queue);
        let stats = world.resource::<RemovedStats>();
        assert_eq!(stats.entities, vec![first, second]);
        assert_eq!(stats.last_seen_before, first_revision);
        assert_eq!(stats.last_seen_after, second_revision);
    }

    #[test]
    fn query_and_filtered_query_work() {
        let mut world = World::new();
        let a = world.spawn((Position(1), Velocity(10))).id();
        let b = world.spawn(Position(2)).id();

        {
            let mut query = world.query::<(&mut Position, &mut Velocity)>();
            for (position, velocity) in query.iter_mut(&mut world) {
                position.0 += velocity.0;
            }
        }

        assert_eq!(world.get::<Position>(a).map(|v| v.0), Some(11));
        assert_eq!(world.get::<Position>(b).map(|v| v.0), Some(2));

        let mut filtered =
            world.query_filtered::<crate::entity::Entity, (With<Position>, Without<Velocity>)>();
        let entities: Vec<_> = filtered.iter(&world).collect();
        assert_eq!(entities, vec![b]);
    }

    fn collect_component_changes(
        mut changes: ComponentChanges<Position>,
        mut stats: ResMut<ComponentChangeStats>,
    ) {
        stats.reads += 1;
        stats.last_seen_before = changes.last_seen_revision();
        stats.current_revision = changes.current_revision();
        stats.added = changes
            .added()
            .map(|(_entity, position)| position.0)
            .collect();
        stats.changed = changes
            .read_changed()
            .map(|(_entity, position)| position.0)
            .collect();
    }

    #[test]
    fn component_changes_cursor_tracks_added_and_changed_per_system() {
        let mut world = World::new();
        let first = world.spawn(Position(1)).id();
        let second = world.spawn(Position(2)).id();
        world.insert_resource(ComponentChangeStats::default());
        let mut queue = CommandQueue::new();
        let mut system = collect_component_changes.into_system();

        system.run(&mut world, &mut queue);
        let first_seen_revision = world.resource::<ComponentChangeStats>().current_revision;
        assert_eq!(world.resource::<ComponentChangeStats>().last_seen_before, 0);
        assert_eq!(world.resource::<ComponentChangeStats>().added, vec![1, 2]);
        assert_eq!(world.resource::<ComponentChangeStats>().changed, vec![1, 2]);

        system.run(&mut world, &mut queue);
        assert_eq!(
            world.resource::<ComponentChangeStats>().last_seen_before,
            first_seen_revision
        );
        assert!(world.resource::<ComponentChangeStats>().added.is_empty());
        assert!(world.resource::<ComponentChangeStats>().changed.is_empty());

        world.get_mut::<Position>(first).unwrap().0 = 10;
        world.spawn(Position(3));
        system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<ComponentChangeStats>().added, vec![3]);
        assert_eq!(
            world.resource::<ComponentChangeStats>().changed,
            vec![10, 3]
        );

        assert_eq!(
            world.get::<Position>(second).map(|position| position.0),
            Some(2)
        );
    }

    #[test]
    fn components_track_mutation_revisions() {
        let mut world = World::new();
        let entity = world.spawn(Position(1)).id();
        let inserted_revision = world.component_revision::<Position>(entity).unwrap();
        assert_eq!(
            world.component_added_revision::<Position>(entity),
            Some(inserted_revision)
        );
        assert!(world.component_changed_since::<Position>(entity, 0));
        assert!(world.component_added_since::<Position>(entity, 0));
        assert!(!world.component_changed_since::<Position>(entity, inserted_revision));
        assert!(!world.component_added_since::<Position>(entity, inserted_revision));

        world.get_mut::<Position>(entity).unwrap().0 = 2;
        let direct_mutation_revision = world.component_revision::<Position>(entity).unwrap();
        assert!(direct_mutation_revision > inserted_revision);
        assert_eq!(
            world.component_added_revision::<Position>(entity),
            Some(inserted_revision)
        );
        assert!(world.component_changed_since::<Position>(entity, inserted_revision));
        assert!(!world.component_added_since::<Position>(entity, inserted_revision));

        {
            let mut query = world.query::<(crate::entity::Entity, &mut Position)>();
            for (queried_entity, position) in query.iter_mut(&mut world) {
                assert_eq!(queried_entity, entity);
                position.0 = 3;
            }
        }
        let query_revision = world.component_revision::<Position>(entity).unwrap();
        assert!(query_revision > direct_mutation_revision);

        let before_missing_lookup = world.change_tick();
        assert!(world.get_mut::<Velocity>(entity).is_none());
        assert_eq!(world.change_tick(), before_missing_lookup);

        assert_eq!(world.remove::<Position>(entity), Some(Position(3)));
        assert_eq!(world.component_revision::<Position>(entity), None);
        assert_eq!(world.component_added_revision::<Position>(entity), None);
    }

    #[test]
    fn added_and_changed_component_queries_track_distinct_revisions() {
        let mut world = World::new();
        let unchanged = world.spawn(Position(1)).id();
        let changed = world.spawn(Position(2)).id();
        let checkpoint = world.change_tick();
        let added_after_checkpoint = world.spawn(Position(3)).id();

        world.get_mut::<Position>(changed).unwrap().0 = 20;

        {
            let mut query = world.query::<(crate::entity::Entity, &Position)>();
            let changed_entities: Vec<_> = query
                .iter_changed_since(&world, checkpoint)
                .map(|(entity, position)| (entity, position.0))
                .collect();
            assert_eq!(
                changed_entities,
                vec![(changed, 20), (added_after_checkpoint, 3)]
            );
        }

        {
            let mut query = world.query::<(crate::entity::Entity, &Position)>();
            let added_entities: Vec<_> = query
                .iter_added_since(&world, checkpoint)
                .map(|(entity, position)| (entity, position.0))
                .collect();
            assert_eq!(added_entities, vec![(added_after_checkpoint, 3)]);
        }

        let mut changed_filter = world.query_filtered::<crate::entity::Entity, Changed<Position>>();
        let filtered_entities: Vec<_> = changed_filter.iter_since(&world, checkpoint).collect();
        assert_eq!(filtered_entities, vec![changed, added_after_checkpoint]);
        assert!(!filtered_entities.contains(&unchanged));

        let mut added_filter = world.query_filtered::<crate::entity::Entity, Added<Position>>();
        let filtered_entities: Vec<_> = added_filter.iter_since(&world, checkpoint).collect();
        assert_eq!(filtered_entities, vec![added_after_checkpoint]);
    }

    fn collect_changed_positions(
        mut query: Query<(crate::entity::Entity, &Position)>,
        mut seen: ResMut<SeenChangedPositions>,
    ) {
        let checkpoint = seen.last_revision;
        seen.values = query
            .iter_changed_since(checkpoint)
            .map(|(_entity, position)| position.0)
            .collect();
    }

    #[derive(Resource, Default)]
    struct SeenChangedPositions {
        last_revision: u64,
        values: Vec<i32>,
    }

    #[test]
    fn system_query_can_iterate_changed_components_since_revision() {
        let mut world = World::new();
        let first = world.spawn(Position(1)).id();
        let second = world.spawn(Position(2)).id();
        let checkpoint = world.change_tick();
        world.insert_resource(SeenChangedPositions {
            last_revision: checkpoint,
            values: Vec::new(),
        });

        world.get_mut::<Position>(second).unwrap().0 = 5;

        let mut queue = CommandQueue::new();
        collect_changed_positions
            .into_system()
            .run(&mut world, &mut queue);
        assert_eq!(world.resource::<SeenChangedPositions>().values, vec![5]);

        let second_revision = world.component_revision::<Position>(second).unwrap();
        world.resource_mut::<SeenChangedPositions>().last_revision = second_revision;
        world.get_mut::<Position>(first).unwrap().0 = 7;
        collect_changed_positions
            .into_system()
            .run(&mut world, &mut queue);
        assert_eq!(world.resource::<SeenChangedPositions>().values, vec![7]);
    }

    fn collect_added_positions(
        mut query: Query<(crate::entity::Entity, &Position)>,
        mut seen: ResMut<SeenChangedPositions>,
    ) {
        let checkpoint = seen.last_revision;
        seen.values = query
            .iter_added_since(checkpoint)
            .map(|(_entity, position)| position.0)
            .collect();
    }

    #[test]
    fn system_query_can_iterate_added_components_since_revision() {
        let mut world = World::new();
        let existing = world.spawn(Position(1)).id();
        let checkpoint = world.change_tick();
        let added = world.spawn(Position(2)).id();
        world.get_mut::<Position>(existing).unwrap().0 = 10;
        world.insert_resource(SeenChangedPositions {
            last_revision: checkpoint,
            values: Vec::new(),
        });

        let mut queue = CommandQueue::new();
        collect_added_positions
            .into_system()
            .run(&mut world, &mut queue);
        assert_eq!(world.resource::<SeenChangedPositions>().values, vec![2]);
        assert_eq!(
            world.component_added_revision::<Position>(added),
            world.component_revision::<Position>(added)
        );
        assert_ne!(
            world.component_added_revision::<Position>(existing),
            world.component_revision::<Position>(existing)
        );
    }

    fn movement_system(mut tick: ResMut<Tick>, mut query: Query<(&mut Position, &Velocity)>) {
        tick.0 += 1;
        for (position, velocity) in query.iter_mut() {
            position.0 += velocity.0;
        }
    }

    fn local_counter_system(mut local: Local<u64>, mut tick: ResMut<Tick>) {
        *local += 1;
        tick.0 = *local;
    }

    fn local_counter_system_a(mut local: Local<u64>, mut trace: ResMut<ScheduleTrace>) {
        *local += 1;
        trace.0.push("a");
    }

    fn local_counter_system_b(mut local: Local<u64>, mut trace: ResMut<ScheduleTrace>) {
        *local += 1;
        trace.0.push(if *local == 1 { "b1" } else { "b_other" });
    }

    #[test]
    fn into_system_extracts_params_from_signature() {
        let mut world = World::new();
        world.insert_resource(Tick::default());
        let entity = world.spawn((Position(1), Velocity(2))).id();

        let mut system = movement_system.into_system();
        let mut commands = CommandQueue::new();
        system.run(&mut world, &mut commands);
        commands.apply(&mut world);

        assert_eq!(world.resource::<Tick>().0, 1);
        assert_eq!(world.get::<Position>(entity).map(|p| p.0), Some(3));
    }

    #[test]
    fn local_system_param_persists_per_system_instance() {
        let mut world = World::new();
        world.insert_resource(Tick::default());
        let mut queue = CommandQueue::new();
        let mut system = local_counter_system.into_system();

        system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<Tick>().0, 1);

        system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<Tick>().0, 2);

        let mut separate_system = local_counter_system.into_system();
        separate_system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<Tick>().0, 1);
    }

    #[test]
    fn local_system_param_is_isolated_between_schedule_systems() {
        let mut world = World::new();
        world.insert_resource(ScheduleTrace::default());
        let mut schedule = Schedule::new();
        schedule.add_system(local_counter_system_a);
        schedule.add_system(local_counter_system_b);

        schedule.run(&mut world);
        schedule.run(&mut world);

        assert_eq!(
            world.resource::<ScheduleTrace>().0,
            vec!["a", "b1", "a", "b_other"]
        );
    }

    fn optional_resource_reader(tick: Option<Res<Tick>>, mut seen: ResMut<Seen>) {
        seen.optional_tick = tick.map(|tick| tick.0);
    }

    fn optional_resource_writer(tick: Option<ResMut<Tick>>, mut seen: ResMut<Seen>) {
        if let Some(mut tick) = tick {
            tick.0 += 1;
            seen.mutated = true;
        } else {
            seen.mutated = false;
        }
    }

    #[test]
    fn optional_resource_system_params_do_not_panic_when_missing() {
        let mut world = World::new();
        world.insert_resource(Seen::default());

        let mut reader = optional_resource_reader.into_system();
        let mut writer = optional_resource_writer.into_system();
        let mut queue = CommandQueue::new();

        reader.run(&mut world, &mut queue);
        writer.run(&mut world, &mut queue);
        assert_eq!(world.resource::<Seen>().optional_tick, None);
        assert!(!world.resource::<Seen>().mutated);

        world.insert_resource(Tick(4));
        reader.run(&mut world, &mut queue);
        writer.run(&mut world, &mut queue);
        assert_eq!(world.resource::<Seen>().optional_tick, Some(4));
        assert!(world.resource::<Seen>().mutated);
        assert_eq!(world.resource::<Tick>().0, 5);
    }

    fn write_events(mut writer: EventWriter<i32>) {
        writer.send(1);
        writer.extend([2, 3]);
    }

    fn read_events(reader: EventReader<i32>, mut stats: ResMut<EventStats>) {
        stats.read_sum = reader.iter().copied().sum();
    }

    fn drain_events(mut drain: EventDrain<i32>, mut stats: ResMut<EventStats>) {
        stats.drained_sum = drain.drain().sum();
    }

    fn cursor_events(mut cursor: EventCursor<i32>, mut stats: ResMut<EventStats>) {
        stats.cursor_sum += cursor.read().copied().sum::<i32>();
        stats.cursor_reads += 1;
    }

    fn command_send_events(mut commands: Commands) {
        commands.send_event(4_i32);
        commands.send_event(5_i32);
    }

    #[test]
    fn event_system_params_write_read_and_drain() {
        let mut world = World::new();
        world.insert_resource(Events::<i32>::default());
        world.insert_resource(EventStats::default());
        let mut queue = CommandQueue::new();

        write_events.into_system().run(&mut world, &mut queue);
        assert_eq!(world.resource::<Events<i32>>().len(), 3);

        read_events.into_system().run(&mut world, &mut queue);
        assert_eq!(world.resource::<EventStats>().read_sum, 6);
        assert_eq!(world.resource::<Events<i32>>().len(), 3);

        drain_events.into_system().run(&mut world, &mut queue);
        assert_eq!(world.resource::<EventStats>().drained_sum, 6);
        assert!(world.resource::<Events<i32>>().is_empty());
    }

    #[test]
    fn event_cursor_reads_only_new_events_without_consuming() {
        let mut world = World::new();
        world.insert_resource(Events::<i32>::default());
        world.insert_resource(EventStats::default());
        let mut queue = CommandQueue::new();
        let mut cursor_system = cursor_events.into_system();

        world.resource_mut::<Events<i32>>().extend([1, 2]);
        cursor_system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<EventStats>().cursor_sum, 3);
        assert_eq!(world.resource::<EventStats>().cursor_reads, 1);
        assert_eq!(world.resource::<Events<i32>>().len(), 2);

        cursor_system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<EventStats>().cursor_sum, 3);
        assert_eq!(world.resource::<EventStats>().cursor_reads, 2);

        world.resource_mut::<Events<i32>>().send(4);
        cursor_system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<EventStats>().cursor_sum, 7);
    }

    #[test]
    fn event_cursor_recovers_when_events_are_drained() {
        let mut world = World::new();
        world.insert_resource(Events::<i32>::default());
        world.insert_resource(EventStats::default());
        let mut queue = CommandQueue::new();
        let mut cursor_system = cursor_events.into_system();

        world.resource_mut::<Events<i32>>().extend([1, 2, 3]);
        cursor_system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<EventStats>().cursor_sum, 6);

        let _ = world.resource_mut::<Events<i32>>().drain().count();
        world.resource_mut::<Events<i32>>().send(5);
        cursor_system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<EventStats>().cursor_sum, 11);
    }

    #[test]
    fn commands_can_defer_events_and_create_event_resource() {
        let mut world = World::new();
        let mut queue = CommandQueue::new();

        command_send_events
            .into_system()
            .run(&mut world, &mut queue);
        assert!(!world.contains_resource::<Events<i32>>());

        queue.apply(&mut world);

        let events = world.resource::<Events<i32>>();
        assert_eq!(events.iter().copied().collect::<Vec<_>>(), vec![4, 5]);
    }

    #[test]
    fn schedule_command_events_are_visible_on_next_schedule_run() {
        let mut world = World::new();
        world.insert_resource(Events::<i32>::default());
        world.insert_resource(EventStats::default());

        let mut schedule = Schedule::new();
        schedule.add_system(command_send_events);
        schedule.add_system(read_events);

        schedule.run(&mut world);
        assert_eq!(world.resource::<EventStats>().read_sum, 0);

        schedule.run(&mut world);
        assert_eq!(world.resource::<EventStats>().read_sum, 9);
    }

    fn spawn_with_commands(mut commands: Commands) {
        commands.spawn(Position(42));
    }

    fn spawn_and_record_entity_with_commands(
        mut commands: Commands,
        mut last_spawned: ResMut<LastSpawned>,
    ) {
        let entity = commands.spawn(Position(7)).insert(Velocity(3)).id();
        last_spawned.0 = Some(entity);
    }

    fn insert_resource_with_commands(mut commands: Commands) {
        commands.insert_resource(Tick(12));
    }

    fn init_and_remove_resource_with_commands(mut commands: Commands) {
        commands.init_resource::<Seen>();
        commands.remove_resource::<Tick>();
    }

    fn custom_world_command(mut commands: Commands) {
        commands.run(|world| {
            world.insert_resource(Tick(99));
        });
    }

    #[test]
    fn commands_are_deferred_until_applied() {
        let mut world = World::new();

        let mut system = spawn_with_commands.into_system();
        let mut queue = CommandQueue::new();
        system.run(&mut world, &mut queue);

        {
            let mut query = world.query::<&Position>();
            assert!(query.iter(&world).next().is_none());
        }

        queue.apply(&mut world);

        let mut query = world.query::<&Position>();
        assert_eq!(query.iter(&world).count(), 1);
    }

    #[test]
    fn commands_spawn_returns_entity_for_followup_edits() {
        let mut world = World::new();
        world.insert_resource(LastSpawned::default());
        let mut queue = CommandQueue::new();

        spawn_and_record_entity_with_commands
            .into_system()
            .run(&mut world, &mut queue);
        let entity = world.resource::<LastSpawned>().0.unwrap();
        assert!(world.contains(entity));
        assert!(world.get::<Position>(entity).is_none());
        assert!(world.get::<Velocity>(entity).is_none());

        queue.apply(&mut world);
        assert_eq!(world.get::<Position>(entity), Some(&Position(7)));
        assert_eq!(world.get::<Velocity>(entity), Some(&Velocity(3)));
    }

    #[test]
    fn commands_can_defer_resource_mutations() {
        let mut world = World::new();
        let mut queue = CommandQueue::new();

        insert_resource_with_commands
            .into_system()
            .run(&mut world, &mut queue);
        assert!(world.get_resource::<Tick>().is_none());

        queue.apply(&mut world);
        assert_eq!(world.get_resource::<Tick>().map(|tick| tick.0), Some(12));

        let mut queue = CommandQueue::new();
        init_and_remove_resource_with_commands
            .into_system()
            .run(&mut world, &mut queue);
        assert!(world.get_resource::<Seen>().is_none());
        assert!(world.get_resource::<Tick>().is_some());

        queue.apply(&mut world);
        assert!(world.get_resource::<Seen>().is_some());
        assert!(world.get_resource::<Tick>().is_none());
    }

    #[test]
    fn commands_can_queue_custom_world_mutations() {
        let mut world = World::new();
        let mut queue = CommandQueue::new();

        custom_world_command
            .into_system()
            .run(&mut world, &mut queue);
        assert!(world.get_resource::<Tick>().is_none());

        queue.apply(&mut world);
        assert_eq!(world.resource::<Tick>().0, 99);
    }

    fn schedule_record_first(mut trace: ResMut<ScheduleTrace>, mut commands: Commands) {
        trace.0.push("first");
        commands.spawn(Position(10));
    }

    fn schedule_record_second(mut trace: ResMut<ScheduleTrace>, mut query: Query<&Position>) {
        trace.0.push("second");
        let sees_deferred_spawn = query.iter().any(|position| position.0 == 10);
        assert!(!sees_deferred_spawn);
    }

    fn schedule_insert_resource(mut commands: Commands) {
        commands.insert_resource(Tick(44));
    }

    fn schedule_record_target(mut trace: ResMut<ScheduleTrace>) {
        trace.0.push("target");
    }

    fn schedule_record_before(mut trace: ResMut<ScheduleTrace>) {
        trace.0.push("before");
    }

    fn schedule_record_after(mut trace: ResMut<ScheduleTrace>) {
        trace.0.push("after");
    }

    fn schedule_record_group_a(mut trace: ResMut<ScheduleTrace>) {
        trace.0.push("group_a");
    }

    fn schedule_record_group_b(mut trace: ResMut<ScheduleTrace>) {
        trace.0.push("group_b");
    }

    #[test]
    fn schedule_runs_param_systems_in_order_and_applies_commands_afterwards() {
        let mut world = World::new();
        world.insert_resource(ScheduleTrace::default());

        let mut schedule = Schedule::new();
        assert!(schedule.is_empty());
        schedule.add_system(schedule_record_first);
        schedule.add_system(schedule_record_second);
        assert_eq!(schedule.len(), 2);

        schedule.run(&mut world);

        assert_eq!(world.resource::<ScheduleTrace>().0, vec!["first", "second"]);
        let mut query = world.query::<&Position>();
        assert_eq!(query.iter(&world).count(), 1);
    }

    #[test]
    fn schedule_applies_deferred_resource_commands_afterwards() {
        let mut world = World::new();
        let mut schedule = Schedule::new();
        schedule.add_system(schedule_insert_resource);

        schedule.run(&mut world);

        assert_eq!(world.resource::<Tick>().0, 44);
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum AppState {
        Menu,
        Playing,
    }

    fn gated_tick(mut tick: ResMut<Tick>) {
        tick.0 += 1;
    }

    #[test]
    fn run_if_in_state_gates_system_execution() {
        let mut world = World::new();
        world.insert_resource(Tick::default());
        world.insert_resource(State::new(AppState::Menu));

        let mut system = gated_tick.run_if(in_state(AppState::Playing));
        let mut queue = CommandQueue::new();

        system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<Tick>().0, 0);

        world
            .resource_mut::<State<AppState>>()
            .set(AppState::Playing);
        system.run(&mut world, &mut queue);
        assert_eq!(world.resource::<Tick>().0, 1);
    }

    #[test]
    fn schedule_supports_run_conditions() {
        let mut world = World::new();
        world.insert_resource(Tick::default());
        world.insert_resource(State::new(AppState::Menu));

        let mut schedule = Schedule::new();
        schedule.add_system(gated_tick.run_if(in_state(AppState::Playing)));

        schedule.run(&mut world);
        assert_eq!(world.resource::<Tick>().0, 0);

        world
            .resource_mut::<State<AppState>>()
            .set(AppState::Playing);
        schedule.run(&mut world);
        assert_eq!(world.resource::<Tick>().0, 1);
    }

    #[test]
    fn state_records_transition_metadata_for_immediate_and_queued_changes() {
        let mut state = State::new(AppState::Menu);
        assert_eq!(state.current(), &AppState::Menu);
        assert_eq!(state.previous(), None);
        assert_eq!(state.transition_revision(), 0);
        assert!(state.transition().is_none());

        state.set(AppState::Playing);
        assert_eq!(state.current(), &AppState::Playing);
        assert_eq!(state.previous(), Some(&AppState::Menu));
        assert_eq!(state.transition_revision(), 1);
        let transition = state.transition().expect("transition");
        assert_eq!(transition.from, &AppState::Menu);
        assert_eq!(transition.to, &AppState::Playing);
        assert_eq!(transition.revision, 1);

        state.set_next(AppState::Menu);
        assert_eq!(state.next(), Some(&AppState::Menu));
        assert!(state.apply_transition());
        assert_eq!(state.current(), &AppState::Menu);
        assert_eq!(state.previous(), Some(&AppState::Playing));
        assert_eq!(state.transition_revision(), 2);
        assert!(!state.apply_transition());
        assert_eq!(state.transition_revision(), 2);
    }

    #[test]
    fn state_entered_and_exited_conditions_fire_once_per_transition() {
        let mut world = World::new();
        world.insert_resource(Tick::default());
        world.insert_resource(State::new(AppState::Menu));

        let mut entered_playing = gated_tick.run_if(state_entered(AppState::Playing));
        let mut exited_menu = gated_tick.run_if(state_exited(AppState::Menu));
        let mut queue = CommandQueue::new();

        entered_playing.run(&mut world, &mut queue);
        exited_menu.run(&mut world, &mut queue);
        assert_eq!(world.resource::<Tick>().0, 0);

        world
            .resource_mut::<State<AppState>>()
            .set(AppState::Playing);

        entered_playing.run(&mut world, &mut queue);
        exited_menu.run(&mut world, &mut queue);
        assert_eq!(world.resource::<Tick>().0, 2);

        entered_playing.run(&mut world, &mut queue);
        exited_menu.run(&mut world, &mut queue);
        assert_eq!(world.resource::<Tick>().0, 2);

        world.resource_mut::<State<AppState>>().set(AppState::Menu);
        entered_playing.run(&mut world, &mut queue);
        exited_menu.run(&mut world, &mut queue);
        assert_eq!(world.resource::<Tick>().0, 2);
    }

    #[test]
    fn schedule_orders_systems_by_before_and_after_labels() {
        let mut world = World::new();
        world.insert_resource(ScheduleTrace::default());

        let mut schedule = Schedule::new();
        schedule.add_system_after("target", schedule_record_after);
        schedule.add_labeled_system("target", schedule_record_target);
        schedule.add_system_before("target", schedule_record_before);

        schedule.run(&mut world);

        assert_eq!(
            world.resource::<ScheduleTrace>().0,
            vec!["before", "target", "after"]
        );
    }

    #[test]
    fn schedule_orders_systems_by_sets() {
        let mut world = World::new();
        world.insert_resource(ScheduleTrace::default());

        let mut schedule = Schedule::new();
        schedule.add_labeled_system("target", schedule_record_target);
        schedule.add_system_after("input", schedule_record_after);
        schedule.add_system_to_set("input", schedule_record_group_b);
        schedule.add_system_to_set("input", schedule_record_group_a);
        schedule.configure_set_before("input", "target");

        schedule.run(&mut world);

        assert_eq!(
            world.resource::<ScheduleTrace>().0,
            vec!["group_b", "group_a", "target", "after"]
        );
    }

    #[test]
    fn schedule_ordering_diagnostics_report_missing_labels_and_cycles() {
        let mut schedule = Schedule::new();
        schedule.add_labeled_system_after("a", "b", schedule_record_target);
        schedule.add_labeled_system_after("b", "a", schedule_record_after);
        schedule.add_system_before("missing", schedule_record_before);
        schedule.configure_set_after("missing_set", "a");

        let diagnostics = schedule.ordering_diagnostics();
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("missing before-label")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("missing set")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("cycle")));
    }

    #[test]
    fn events_buffer_and_world_counts_work() {
        let mut events = crate::event::Events::new();
        events.send(1);
        events.extend([2, 3]);
        assert_eq!(events.len(), 3);
        assert_eq!(events.iter().copied().sum::<i32>(), 6);
        assert_eq!(events.drain().collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(events.is_empty());

        let mut world = World::new();
        assert_eq!(world.entity_count(), 0);
        let entity = world.spawn(Position(1)).id();
        world.insert_resource(Tick(7));
        assert_eq!(world.entity_count(), 1);
        assert_eq!(world.resource_count(), 1);
        assert!(world.despawn(entity));
        assert_eq!(world.entity_count(), 0);
    }
}
