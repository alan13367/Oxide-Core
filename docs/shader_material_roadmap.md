# Shader and Material Roadmap

The renderer currently supports built-in WGSL shaders, custom shader sources,
descriptor-driven materials, and fallback material creation. This roadmap keeps
that workflow while moving import-heavy pieces behind optional features.

## Current Status

- Built-in shaders: `basic`, `lit`, `unlit`, `sky_gradient`, `sprite_ui`, and
  `fallback`.
- Shader sources: built-in, WGSL file, or inline WGSL string.
- Material descriptors: JSON, RON, and TOML.
- Texture file loading: available through the `image-import` feature.
- glTF scene loading: available through the `gltf-import` feature.

## Direction

- Runtime rendering should operate on Oxide-owned GPU resources.
- File-format importers should be optional and eventually move toward offline
  asset processing.
- Material descriptors should remain stable enough for examples and small apps,
  but deeper production material schemas should wait for the asset pipeline.

## Compatibility Notes

- Default features keep the current examples working.
- `--no-default-features` builds are used to verify the lean runtime path.
- Disabling importer features should remove importer APIs rather than silently
  pretending the file formats are available.
