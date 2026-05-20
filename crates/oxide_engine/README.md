# oxide_engine

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/alan13367/Oxide-Core#license)
[![Crates.io](https://img.shields.io/crates/v/oxide-core-engine.svg)](https://crates.io/crates/oxide-core-engine)
[![Docs](https://docs.rs/oxide-core-engine/badge.svg)](https://docs.rs/oxide-core-engine/latest/oxide_engine/)

Main game engine facade crate for Oxide Core. It owns the app runner, plugin
system, window/event loop integration, startup/render wiring, and prelude
re-exports for focused runtime crates such as `oxide_scene`, `oxide_ui`,
`oxide_camera`, `oxide_light`, `oxide_audio`, and `oxide_physics`.

The runner exposes explicit gameplay stages plus a lightweight ordered render
pass schedule. Plugins can register frame callbacks around stable anchors for
scene rendering, game text, `App::queue`, and egui without taking over the
entire app render hook.

`DefaultPlugins` also installs frame diagnostics. The shared `Diagnostics`
resource records `DELTA_SECONDS`, `FRAME_TIME_MS`, and `FPS` for overlays,
editor tooling, logs, and game-specific metric streams.
