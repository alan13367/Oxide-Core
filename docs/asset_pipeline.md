# Asset Pipeline Roadmap

Oxide currently supports direct importer paths for development, while the
runtime asset core stays generic.

## Current Shape

- `oxide_asset` owns generic handles, typed storage, and async loading.
- `oxide_renderer` owns GPU resources and optional glTF/image importer helpers.
- `oxide_scene` owns scene descriptors and renderable scene components.
- `oxide_engine` re-exports scene/asset APIs and wires importer systems into
  the app lifecycle.

## Direction

- Import external formats such as glTF, PNG, and JPEG as tooling inputs.
- Convert imported data into Oxide-owned runtime assets.
- Keep runtime systems depending on Oxide asset handles and render components,
  not on external file-format identities.

## First Runtime Asset Types

- `.oxscene`: scene descriptor with entities, transforms, lights, cameras, and
  render intent.
- `.oxmesh`: preprocessed mesh buffers and metadata.
- `.oxmat`: material descriptor targeting Oxide renderer pipelines.

The direct importer features stay enabled by default for examples until native
asset files exist.
