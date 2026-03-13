# Oxide Core

A 3D game engine built from scratch in Rust, targeting macOS with Metal backend.

## Features

- **Rendering**: wgpu-based abstraction with Metal as primary backend
- **ECS**: bevy_ecs for entity-component-system architecture
- **Math**: glam for fast 3D math operations

## Requirements

- Rust 1.75 or later
- macOS 10.15+

## Building

```bash
cargo build
```

## Running Examples

```bash
cargo run -p hello_window
```

## Crates

| Crate | Description |
|-------|-------------|
| `oxide_engine` | Core engine systems |
| `oxide_renderer` | wgpu rendering abstraction |
| `oxide_math` | Math types and utilities |

## License

MIT OR Apache-2.0