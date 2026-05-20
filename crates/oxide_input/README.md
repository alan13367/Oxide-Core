# oxide_input

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/alan13367/Oxide-Core#license)
[![Crates.io](https://img.shields.io/crates/v/oxide-core-input.svg)](https://crates.io/crates/oxide-core-input)
[![Docs](https://docs.rs/oxide-core-input/badge.svg)](https://docs.rs/oxide-core-input/latest/oxide_input/)

Keyboard, mouse, and semantic action/axis input primitives for the Oxide Core game engine.

`ActionBindings<T>` maps game-defined action enums to keyboard and mouse
triggers. `ActionInput<T>` stores pressed, just-pressed, and just-released
state after `sync_action_input_system::<T>` runs in an app schedule.

`AxisBindings<T>` maps game-defined axis enums to scaled keyboard or mouse
triggers. `AxisInput<T>` stores normalized `-1.0..=1.0` values after
`sync_axis_input_system::<T>` runs, which keeps WASD-style movement out of
gameplay systems.
