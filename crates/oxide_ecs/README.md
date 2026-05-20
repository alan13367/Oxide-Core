# oxide_ecs

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/alan13367/Oxide-Core#license)
[![Crates.io](https://img.shields.io/crates/v/oxide_ecs.svg)](https://crates.io/crates/oxide_ecs)
[![Docs](https://docs.rs/oxide_ecs/badge.svg)](https://docs.rs/oxide_ecs/latest/oxide_ecs/)

Entity-Component-System runtime for the Oxide Core game engine.

The runtime includes typed entities/components/resources, standalone
`Schedule`s, deferred `Commands`, signature-driven systems with `Res`,
`ResMut`, `ResourceCursor`, `Query`, `ComponentChanges`, `RemovedComponents`,
event params (`EventReader`, `EventCursor`, `EventWriter`, `EventDrain`),
optional resources via `Option<Res<T>>` / `Option<ResMut<T>>`, persistent
per-system `Local<T>` state, component/resource mutation revisions, and labeled
or set-based system ordering with before/after constraints. `TypeRegistry`
provides lightweight component/resource type metadata for editor, scene, and
tooling code without pulling in a reflection framework.

```rust
use oxide_ecs::prelude::*;

#[derive(Resource, Default)]
struct Ticks(u64);

fn tick(mut ticks: ResMut<Ticks>) {
    ticks.0 += 1;
}

fn count_runs(mut local: Local<u64>, mut ticks: ResMut<Ticks>) {
    *local += 1;
    ticks.0 = *local;
}

let mut world = World::new();
world.insert_resource(Ticks::default());

let mut schedule = Schedule::new();
schedule.add_labeled_system_to_set("tick", "gameplay", tick);
schedule.add_system(count_runs);
schedule.run(&mut world);
```

`Schedule` uses the same `IntoSystem` conversion as the engine app runner, so
standalone schedules support system params, local state, run conditions, event
params, and deferred `Commands`. Use `add_labeled_system`, `add_system_before`, and
`add_system_after` when plugin-like schedules need stable ordering without
depending on insertion order. `add_system_to_set` and
`add_labeled_system_to_set` group related systems so a single before/after
target can order the whole set. `Schedule::ordering_diagnostics()` reports
missing labels or sets, duplicate labels, and cycles.

Use `EventCursor<T>` when a system should read only events it has not seen
before without consuming the shared event buffer. Use `EventReader<T>` to
inspect the whole buffer and `EventDrain<T>` only when one system owns consuming
it.

`State<T>` supports both `set(...)` and queued `set_next(...)` /
`apply_transition()` changes. Each applied transition records the previous
state and a monotonic transition revision. Use `in_state(...)` for steady-state
run conditions, and `state_entered(...)` / `state_exited(...)` for once-per-system
transition hooks.

`World::change_tick()`, `component_revision`, `component_changed_since`,
`component_added_revision`, `component_added_since`, `resource_revision`, and
`resource_changed_since` provide lightweight cache invalidation hooks. Component
insertion/replacement, mutable component access, resource insertion, and mutable
resource access record revisions automatically. Use
`Query<(Entity, &T)>::iter_added_since(tick)` to initialize caches for newly
inserted components, then `iter_changed_since(tick)` to refresh all changed
components. `query_filtered::<&T, Changed<T>>()` and
`query_filtered::<(Entity, &T), Added<T>>()` provide the same revision filters
through filtered-query syntax. Systems can use `ComponentChanges<T>` to keep
this revision cursor locally instead of storing a separate resource. Use
`ResourceCursor<T>` in
systems that need to observe a resource only when its revision changes;
`read_if_changed()` returns the resource once per system instance per revision.
Use `RemovedComponents<T>` to clean up cache entries after components are
removed or entities despawn. Long running tools can bound retained removal history with
`prune_removed_components_through::<T>(revision)` or
`prune_all_removed_components_through(revision)` after all relevant systems have
advanced past that revision.

Use `World::register_component_type::<T>()` and
`World::register_resource_type::<T>()` to populate the `TypeRegistry` resource.
The registry records `TypeId`, full Rust type name, short type name, and broad
kind (`Component`, `Resource`, or `Other`) so tooling can show stable type
identity without field-level reflection.

`Commands` can spawn/despawn entities, edit entity components, and insert,
initialize, or remove resources at the end of the current schedule/stage.
`Commands::spawn` reserves and returns an entity ID immediately, so systems can
queue follow-up component edits or store that ID in events/resources while the
spawned components remain deferred until commands are applied.
Use `Commands::send_event` to defer event emission with other command-buffered
world edits; it creates the matching `Events<T>` resource on apply if needed.
