# AGENTS.md - Oxide Core Context

## Project Overview
Oxide Core is a high-performance 3D game engine built from scratch in Rust. It is specifically optimized for macOS, leveraging the Metal graphics API through the `wgpu` abstraction layer. The engine follows a modular, data-driven architecture using an Entity-Component-System (ECS) pattern.

### Core Technologies
- **Language**: Rust (Edition 2021)
- **Graphics**: `wgpu` (Metal backend prioritized)
- **ECS**: `oxide_ecs` custom runtime for state management and logic
- **Systems**: `IntoSystem` + `SystemParam` (`Res`, `ResMut`, `Query`, `Commands`) for ergonomic signatures
- **Math**: `glam` for efficient 3D linear algebra
- **Physics**: in-house `oxide_physics` runtime for rigid body simulation
- **Windowing**: `winit` for cross-platform windowing and event handling
- **Diagnostics**: `tracing` and `tracing-subscriber` for logging and instrumentation
- **Serialization**: `serde` and `serde_json` for material descriptors

## Project Architecture
The workspace is divided into several specialized crates:

- **`oxide_engine`**: The core orchestration layer. It defines the `App` trait, plugin APIs (`Plugin`, `DefaultPlugins`), manages the main loop via `winit`, integrates `oxide_ecs`, handles input/events, and provides an `AssetWatcher` for hot-reloading.
- **`oxide_renderer`**: A low-level abstraction over `wgpu`. It handles device/queue initialization, swapchain management (Surface), and provides primitives for meshes, pipelines, descriptor-driven materials, and shaders.
- **`oxide_math`**: Provides math types and utilities, re-exporting `glam` types and adding engine-specific transforms and camera math.
- **`oxide_physics`**: In-house 3D physics crate providing ECS components/resources/systems and a `PhysicsPlugin` for `AppStage::Update` (fixed-step simulation, spatial-hash broadphase, warm-started manifold solver, collision layers/events, joints, and OBB-aware cuboid collisions).
- **`examples/`**: Contains demonstration projects. The primary example is `hello_window`, which serves as a full-featured interactive 3D scene with FPS-style camera controls. Other examples include `physics_example`, `unlit_example`, `sky_gradient_example`, and `sprite_ui_example`.

### Published package names (crates.io)
- `oxide-core-engine`, `oxide-core-renderer`, `oxide-core-asset`, `oxide-core-input`, `oxide-core-transform`, `oxide-core-math`, `oxide-core-physics`
- `oxide_ecs`, `oxide_ecs_derive`

## Building and Running

### Prerequisites
- Rust 1.94 or later
- macOS 10.15+ (for Metal support)

### Key Commands
- **Build Project**:
  ```bash
  cargo build
  ```
- **Run Interactive Demo**:
  ```bash
  cargo run -p hello_window
  ```
- **Run Tests**:
  ```bash
  cargo test
  ```
- **Linting**:
  ```bash
  cargo clippy
  ```

## Development Conventions

### Application Lifecycle
Engine users implement the `App` trait found in `oxide_engine::app`. The lifecycle follows these stages:
1. `configure`: Initialize ECS resources and schedules.
2. `init`: Create application state, load assets, and set up the initial scene.
3. Startup plugin systems run once (for `DefaultPlugins`, this includes default input/window resource wiring).
4. `update`: Process input and update ECS world state (called every frame). Hot-reloading checks can be performed here using `AssetWatcher`.
5. `extract` / `prepare` / `queue`: Render pipeline stages executed each frame.
6. `on_event`: Respond to windowing or system events.

### ECS Runtime Notes
- **Deferred Commands**: `Commands` queue world mutations and apply them at stage boundaries.
- **State Gating**: Use `State<T>` and `.run_if(in_state(...))` to conditionally execute systems.
- **Mixed Query Support**: `Query<(&mut A, &B)>` and `Query<(&A, &mut B)>` are supported in system params.
- **Entity-Aware Query Support**: `Query<(Entity, &T)>` and `Query<(Entity, &mut T)>` are supported for systems that need stable entity identity.
- **Async glTF Spawn Flow**: Queue loads via `request_gltf_scene_spawn(...)`; `gltf_scene_spawn_system` resolves handles and spawns hierarchy roots retrievable through `take_spawned_scene_roots(...)`.

### Coding Style
- **ECS-First**: Prefer storing data in Components and logic in Systems or `App` trait implementations.
- **Safety**: Leverage Rust's type system to ensure thread safety and memory management.
- **Explicit Imports**: Use `oxide_engine::prelude::*` for common types, but prefer explicit imports for crate-internal modules.
- **Error Handling**: Use `thiserror` for defining custom error types in library crates (`oxide_renderer`, `oxide_engine`).
- **Resource Management**: Large GPU resources (Buffers, Textures) should be managed through the `Renderer` or stored as ECS Resources.

### Materials and Shaders
- **Built-in Shaders**: Use `BuiltinShader` (`basic`, `lit`, `unlit`, `sky_gradient`, `sprite_ui`, `fallback`).
- **Custom Shaders**: Load custom shaders through `ShaderSource::File` or `ShaderSource::WgslOwned`.
- **Material Descriptors**: Materials can be defined using JSON descriptors and loaded via `load_material_descriptor`.

### Performance
- Development profiles use `opt-level = 1` for faster iteration, while dependencies are compiled with `opt-level = 3`.
- Release builds utilize Link Time Optimization (`lto = "thin"`) and single codegen units for maximum performance.

### IMPORTANT NOTES
After every major code refactoring/change make sure to keep README.md and AGENTS.md file updated.
