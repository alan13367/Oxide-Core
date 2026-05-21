# oxide_renderer

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/alan13367/Oxide-Core#license)
[![Crates.io](https://img.shields.io/crates/v/oxide-core-renderer.svg)](https://crates.io/crates/oxide-core-renderer)
[![Docs](https://docs.rs/oxide-core-renderer/badge.svg)](https://docs.rs/oxide-core-renderer/latest/oxide_renderer/)

Low-level rendering abstraction for the Oxide Core game engine.

`Mesh3D` owns GPU buffers plus CPU-side vertex/index mirrors. Imported glTF
primitives can preserve `MeshSkinning` joint/weight attributes, and the renderer
crate exposes a small software skinning helper for future dynamic upload and GPU
palette paths.
