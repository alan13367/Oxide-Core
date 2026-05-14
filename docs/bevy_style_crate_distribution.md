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
- `oxide_audio`: audio playback, generated tones, WAV clips, and software
  mixing.
- `oxide_camera`: camera components, FPS controller logic, and GPU camera
  buffers.
- `oxide_light`: light components and GPU light buffers/uniforms.
- `oxide_scene`: scene descriptors, renderable scene components, native
  sprites, terrain/world descriptors, transform hierarchy re-exports, and
  automatic scene rendering.
- `oxide_ui`: native game UI widgets, text/font rendering, egui bridge helpers,
  runtime UI data, and debug overlay data.
- `oxide_editor`: runtime scene editor resource and egui hierarchy/inspector.
- `oxide_renderer`: GPU rendering abstractions and optional import helpers.
- `oxide_physics`: standalone in-house physics runtime.
- `oxide_engine`: app lifecycle, plugins, platform integration, startup/render
  wiring, and facade APIs.

## Boundary Goals

- Keep `oxide_physics` usable without `oxide_engine`.
- Keep `oxide_asset` free of renderer-specific asset types.
- Keep `oxide_audio` usable without `oxide_engine`; engine should only provide
  plugin/resource wiring.
- Keep `oxide_camera`, `oxide_light`, `oxide_scene`, `oxide_ui`, and
  `oxide_editor` usable without `oxide_engine`; engine should provide plugin
  adapters and frame/resource wiring only.
- Keep canonical gameplay-facing component types in their domain crates, with
  `oxide_engine::prelude` re-exporting them for stable app ergonomics.
- Keep renderer importers optional so runtime builds can avoid glTF/image
  decoding when assets are preprocessed.
- Keep plugin wiring thin: engine-facing plugin adapters should not pull core
  simulation logic into the facade crate.

## Near-Term Work

- Continue moving importer/tooling concerns behind feature gates.
- Add native Oxide asset formats before making importers non-default.
- Keep examples on the high-level facade so user ergonomics remain clear while
  the internal crates become more independent.
