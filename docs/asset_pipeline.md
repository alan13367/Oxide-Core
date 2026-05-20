# Asset Pipeline Roadmap

Oxide supports direct importer paths for development plus first-class native
asset documents for scenes and materials.

## Current Shape

- `oxide_asset` owns generic handles, typed storage, async loading, load
  status, and typed path identity.
- `oxide_renderer` owns GPU resources and optional glTF/image importer helpers.
- `oxide_renderer` owns versioned `.oxmat` material documents.
- `oxide_scene` owns scene descriptors, renderable scene components, and
  versioned `.oxscene` documents.
- `oxide_engine` re-exports scene/asset APIs and wires importer systems into
  the app lifecycle.

## Direction

- Import external formats such as glTF, PNG, and JPEG as tooling inputs.
- Convert imported data into Oxide-owned runtime assets.
- Keep runtime systems depending on Oxide asset handles and render components,
  not on external file-format identities.
- Prefer versioned Oxide documents for game-authored scenes and materials.

## First Runtime Asset Types

- `.oxscene`: scene descriptor with entities, transforms, lights, cameras, and
  render intent. `load_scene_descriptor(...)` accepts both legacy raw scene JSON
  and wrapped `.oxscene`; `save_scene_descriptor(...)` writes the wrapper.
- `.oxmesh`: preprocessed mesh buffers and metadata.
- `.oxmat`: material descriptor targeting Oxide renderer pipelines.
  `load_material_descriptor(...)` accepts legacy JSON/RON/TOML and wrapped
  `.oxmat`; `save_material_descriptor(...)` writes the wrapper.

## Runtime Scene Loading

Use `request_oxscene_spawn(world, path)` to asynchronously load a native scene
document and spawn it once ready. Spawned root entities are available through
`take_spawned_oxscene_roots(world, handle)`. The system is installed by
`DefaultPlugins`, so it does not require glTF importer features.

`examples/minimal_game` demonstrates this path with
`assets/scenes/starter.oxscene`.

The direct importer features stay enabled by default for examples that still use
external tooling formats.
