# Crate Distribution Roadmap

Oxide follows a focused-crate distribution model: each domain crate owns a
small part of the engine, while `oxide_engine` acts as the high-level facade.
This is an organizational pattern only; Oxide does not depend on Bevy or other
engine frameworks.

## Current Direction

- `oxide_ecs`: custom ECS runtime, query APIs, resources, schedules, systems.
- `oxide_math`: math facade and engine math helpers.
- `oxide_transform`: transform and hierarchy components/systems.
- `oxide_asset`: generic handles, typed asset storage, and async asset server.
- `oxide_renderer`: GPU rendering abstractions and optional import helpers.
- `oxide_physics`: standalone in-house physics runtime.
- `oxide_engine`: app lifecycle, plugins, platform integration, and facade APIs.

## Boundary Goals

- Keep `oxide_physics` usable without `oxide_engine`.
- Keep `oxide_asset` free of renderer-specific asset types.
- Keep renderer importers optional so runtime builds can avoid glTF/image
  decoding when assets are preprocessed.
- Keep plugin wiring thin: engine-facing plugin adapters should not pull core
  simulation logic into the facade crate.

## Near-Term Work

- Continue moving importer/tooling concerns behind feature gates.
- Add native Oxide asset formats before making importers non-default.
- Keep examples on the high-level facade so user ergonomics remain clear while
  the internal crates become more independent.
