# oxide_transform

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/alan13367/Oxide-Core#license)
[![Crates.io](https://img.shields.io/crates/v/oxide-core-transform.svg)](https://crates.io/crates/oxide-core-transform)
[![Docs](https://docs.rs/oxide-core-transform/badge.svg)](https://docs.rs/oxide-core-transform/latest/oxide_transform/)

Transform and hierarchy primitives for the Oxide Core game engine.

`attach_child`, `detach_child`, and `mark_subtree_dirty` mutate hierarchy data
directly when a system owns `&mut World`. Systems that use deferred ECS commands
can import `HierarchyCommandsExt` and queue those same hierarchy edits:

```rust
use oxide_transform::HierarchyCommandsExt;

fn parent_pickup(mut commands: Commands, player: Res<PlayerEntity>) {
    let pickup = commands.spawn(Pickup).id();
    commands.attach_child(player.0, pickup);
}
```
