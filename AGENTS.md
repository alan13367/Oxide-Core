# AGENTS.md - Oxide Core Context

## Project Overview
Oxide Core is a high-performance 3D game engine built from scratch in Rust. It is specifically optimized for macOS, leveraging the Metal graphics API through the `wgpu` abstraction layer. The engine follows a modular, data-driven architecture using an Entity-Component-System (ECS) pattern.

### Core Technologies
- **Language**: Rust (Edition 2021)
- **Graphics**: `wgpu` (Metal backend prioritized)
- **ECS**: `oxide_ecs` custom runtime for state management and logic
- **Systems**: `IntoSystem` + `SystemParam` (`Res`, `ResMut`, `Query`, `Commands`) for ergonomic app-stage and standalone `Schedule` signatures, with optional labels, sets, and before/after ordering constraints
- **Schedules**: `AppStage::Startup`, `PreUpdate`, `FixedUpdate`, `Update`, `PostUpdate`, `Extract`, and `Prepare` are available; `Startup` runs once with normal ECS params after window/app initialization, and `FixedUpdate` is driven by the `FixedTime` resource. Built-in ordering labels include `OXSCENE_SPAWN_SYSTEM`, `GLTF_SCENE_SPAWN_SYSTEM`, and `TRANSFORM_PROPAGATE_SYSTEM`.
- **Render Pass Ordering**: Plugins can register lightweight frame callbacks with `add_render_pass`, `add_render_pass_before`, `add_render_pass_after`, and render pass sets. Built-in anchors are `RENDER_PASS_SCENE`, `RENDER_PASS_GAME_TEXT`, `RENDER_PASS_APP_QUEUE`, and `RENDER_PASS_EGUI`.
- **Input Mapping**: `oxide_input` supports raw keyboard/mouse resources plus semantic `ActionBindings<T>`/`ActionInput<T>` synced by `sync_action_input_system::<T>`
- **Diagnostics**: `FrameDiagnosticsPlugin` records `DELTA_SECONDS`, `FRAME_TIME_MS`, and `FPS` into the `Diagnostics` resource. `DefaultPlugins` installs it, and scene authoring UI uses it for the debug overlay.
- **Math**: `glam` for efficient 3D linear algebra
- **Physics**: in-house `oxide_physics` runtime for rigid body simulation
- **Audio**: `oxide_audio` for Oxide-owned audio playback, WAV clips, generated tones, and software mixing over the `cpal` platform backend
- **Windowing**: `winit` for cross-platform windowing and event handling
- **Diagnostics**: `tracing` and `tracing-subscriber` for logging and instrumentation
- **Serialization**: `serde` and `serde_json` for material, terrain, world, `.oxscene`, and `.oxmat` descriptors
- **Sprite Images**: `image` is allowed as import/runtime utility plumbing behind Oxide-owned `SpriteImage` APIs
- **Dependency Policy**: Oxide owns its engine, ECS, physics, scene hierarchy, and gameplay-facing runtime. Do not add third-party game engines, ECS runtimes, physics engines, scene graphs, or gameplay frameworks; utility/platform crates are allowed behind Oxide-owned APIs.

## Project Architecture
The workspace is divided into several specialized crates:

- **`oxide_engine`**: The core orchestration/facade layer. It defines the `App` trait, plugin APIs (`Plugin`, `DefaultPlugins`, `SceneAuthoringPlugins`) with registration metadata and duplicate guards, manages the main loop via `winit`, integrates `oxide_ecs` schedules/systems, handles input/events, provides an `AssetWatcher`, native `.oxscene` reload helpers, material descriptor asset helpers for hot-reloading, and lightweight transform tween animation, and owns engine-facing plugin wiring/re-exports for scene, UI, editor, camera, light, audio, and renderer systems.
- **`oxide_renderer`**: A low-level abstraction over `wgpu`. It handles device/queue initialization, swapchain management (Surface), and provides primitives for meshes, pipelines, descriptor-driven materials, and shaders.
- **`oxide_math`**: Provides math types and utilities, re-exporting `glam` types and adding engine-specific transforms and camera math.
- **`oxide_asset`**: Generic typed handles, revision-tracked handle-indexed asset storage with added/modified/removed change records and per-consumer change cursors, async loading/reload primitives, load status, typed path identity, and dependency path metadata for hot reload/import invalidation. Renderer-specific caches and imported asset types should live in renderer or engine-facing crates.
- **`oxide_audio`**: Audio playback, generated tones, WAV clip loading, software mixing, volume control, and sound instance handles. Engine integration lives in `oxide_engine::audio::AudioPlugin`.
- **`oxide_camera`**: Camera components, FPS controller system, and GPU camera buffer helpers. Engine integration is a compatibility re-export.
- **`oxide_light`**: Ambient/directional/point light components plus GPU light uniform/buffer helpers. Engine integration is a compatibility re-export.
- **`oxide_scene`**: Scene descriptors, versioned `.oxscene` documents, validation diagnostics, nested entity/prefab/sprite spawning, renderable scene components, camera/renderable `RenderLayers`, native RGBA/PNG sprites, terrain/world descriptors, transform hierarchy re-exports, scene gizmo line overlays, and the automatic `SceneRenderer`. Engine integration lives in `oxide_engine::scene::SceneRendererPlugin`.
- **`oxide_ui`**: Native `GameUi`, camera-locked sprite widgets, text/font rendering, engine-owned egui-wgpu pass, runtime UI data, and debug overlay data. Engine integration lives in `oxide_engine::ui` plugin wrappers.
- **`oxide_editor`**: Runtime `SceneEditor` model, egui hierarchy/inspector surface, viewport picking, and translate/rotate/scale gizmo state. Engine integration lives in `oxide_engine::scene::SceneEditorPlugin`.
- **`oxide_physics`**: In-house 3D physics crate providing ECS components/resources/systems and a `PhysicsPlugin` for `AppStage::Update` (fixed-step simulation, spatial-hash broadphase, warm-started manifold solver, collision layers/events, joints, and OBB-aware cuboid collisions).
- **`examples/`**: Contains demonstration projects. The primary example is `hello_window`, which serves as a full-featured interactive 3D scene with FPS-style camera controls. Other examples include `minimal_game`, `zombie_shooter`, `physics_example`, `unlit_example`, `sky_gradient_example`, and `sprite_ui_example`.

### Published package names (crates.io)
- `oxide-core-engine`, `oxide-core-renderer`, `oxide-core-asset`, `oxide-core-audio`, `oxide-core-camera`, `oxide-core-light`, `oxide-core-scene`, `oxide-core-ui`, `oxide-core-editor`, `oxide-core-input`, `oxide-core-transform`, `oxide-core-math`, `oxide-core-physics`
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
3. Startup plugin hooks run once (for `DefaultPlugins`, this includes default input/window resource wiring), then `AppStage::Startup` systems run once with normal `IntoSystem` params.
4. `update`: Process input and update ECS world state (called every frame). Hot-reloading checks can be performed here using `AssetWatcher`.
5. `extract` / `prepare` / `queue`: Render pipeline stages executed each frame.
6. `on_event`: Respond to windowing or system events.
7. Systems can request clean shutdown through the `AppExit` resource inserted by the runner before startup.
8. If `SceneRendererPlugin` is installed, the runner automatically prepares and queues visible `RenderMesh`, `Terrain`, `SpriteBillboard`, and scene gizmo lines at the `RENDER_PASS_SCENE` anchor, filters renderables through the active camera's `RenderLayers`, then resizes the scene depth texture. If `GameUiPlugin` is installed, it also installs the native overlay text renderer and `GameFonts` registry used by `GameUi` text widgets. If `EguiPlugin` is installed, the runner owns egui input, frame execution, and rendering at `RENDER_PASS_EGUI`. Egui authoring panels are hidden by default and toggled with `F1`.

### ECS Runtime Notes
- **Deferred Commands**: `Commands` queue entity/resource world mutations and optional `send_event` emissions, then apply them at stage boundaries.
- **Reserved Command Spawns**: `Commands::spawn` returns `EntityCommands` with a stable entity ID for follow-up queued edits, hierarchy links, events, or resources before component insertion is applied.
- **Hierarchy Commands**: Use `HierarchyCommandsExt` for deferred `attach_child`, `detach_child`, and subtree-dirty operations from normal ECS systems.
- **Standalone Schedules**: `oxide_ecs::Schedule` accepts the same `IntoSystem` signatures, run conditions, labels, sets, and before/after ordering constraints as app stages, then applies deferred commands after the schedule run.
- **Startup Systems**: Prefer `AppStage::Startup` for game/plugin setup that can use ECS params. Keep `add_startup_system_mut(fn(&mut World, &Window))` for low-level window-aware engine initialization.
- **Fixed-Step Systems**: Use `AppStage::FixedUpdate` for deterministic gameplay ticks. `DefaultPlugins` inserts `FixedTime`; read `FixedTime::timestep_secs()` instead of frame `Time::delta_secs()` inside fixed systems.
- **Render Passes**: Use `add_render_pass` for plugin-owned overlays or debug drawing that should run between built-in game text and `App::queue`. Use `add_render_pass_before` / `add_render_pass_after` for explicit ordering around built-in anchors, and keep long-lived GPU resources in non-send resources prepared before queueing.
- **Runtime Diagnostics**: Use `Diagnostics::record` for lightweight scalar metrics that need to appear in tooling, overlays, or logs. Prefer this resource for frame/runtime counters before adding a new profiling dependency.
- **Optional Resources**: Use `World::get_resource` / `get_resource_mut` or `Option<Res<T>>` / `Option<ResMut<T>>` for plugin-owned resources that may not be installed. Keep `resource` / `resource_mut` for required invariants.
- **Change Revisions + Metadata**: Use `World::change_tick()`, `component_revision`, `component_changed_since`, `component_added_revision`, `component_added_since`, `ComponentChanges<T>`, `removed_components_since`, `RemovedComponents<T>`, `prune_removed_components_through`, `resource_revision`, `resource_changed_since`, `Query<(Entity, &T)>::iter_added_since`, `Query<(Entity, &T)>::iter_changed_since`, `query_filtered::<&T, Changed<T>>()`, `query_filtered::<(Entity, &T), Added<T>>()`, and `ResourceCursor<T>` for lightweight cache invalidation in renderer, editor, tooling, and gameplay systems. Use `World::register_component_type::<T>()`, `World::register_resource_type::<T>()`, and `TypeRegistry` when tooling needs stable type identity without field-level reflection.
- **Local System State**: Use `Local<T>` for small persistent state owned by a single system; use resources for shared state.
- **Event Params**: Use `EventWriter<T>` to queue events immediately, `Commands::send_event` to defer event emission with other world edits, `EventReader<T>` to inspect the whole buffer, `EventCursor<T>` for per-system incremental non-consuming reads, and `EventDrain<T>` when one system owns consuming a FIFO event buffer.
- **State Gating**: Use `State<T>` and `.run_if(in_state(...))` to conditionally execute steady-state systems. Use `state_entered(...)` and `state_exited(...)` for once-per-system transition hooks; `State<T>` records previous/current values plus a transition revision after `set(...)` or `apply_transition()`.
- **Mixed Query Support**: `Query<(&mut A, &B)>` and `Query<(&A, &mut B)>` are supported in system params.
- **Entity-Aware Query Support**: `Query<(Entity, &T)>` and `Query<(Entity, &mut T)>` are supported for systems that need stable entity identity.
- **Async Native Scene Spawn Flow**: Queue `.oxscene` loads via `request_oxscene_spawn(...)`; `oxscene_spawn_system` resolves handles and spawns roots retrievable through `take_spawned_oxscene_roots(...)`.
- **Async glTF Spawn Flow**: Queue loads via `request_gltf_scene_spawn(...)`; `gltf_scene_spawn_system` resolves handles and spawns hierarchy roots retrievable through `take_spawned_scene_roots(...)`.
- **Game Authoring Path**: Prefer `.oxscene`, `SceneDescriptor`, `ScenePrefabDescriptor`, `SceneSpriteDescriptor`, `SceneDescriptor::validate`, `try_spawn_scene_prefab`, `spawn_scene_prefab`, `request_oxscene_spawn`, `reload_oxscene_path`, `reload_changed_oxscenes`, `request_material_descriptor_load`, `reload_changed_material_descriptors`, `RenderMesh`, `RenderLayers`, `SpriteImage`, `SpriteAssets`, `SpriteBillboard`, `Visibility`, `InheritedVisibility`, `TransformTween`, `GameUi::sprite`, `Terrain`, `TerrainDescriptor`, `SceneWorldDescriptor`, `SceneAuthoringPlugins`, `SceneRendererPlugin`, `GameUiPlugin`, `GameFonts`, `GameTextStyle`, `AudioPlugin`, `Audio`, `SceneEditorPlugin`, `Events<T>`, `Timer`, `RuntimeUiPlugin`, and `DevOverlayPlugin` for new examples and game-facing features. See `docs/building_games.md` and `docs/scene_authoring.md`.
- **Action Input Flow**: For button-style gameplay controls, define a small action enum, insert `ActionBindings<Action>` and `ActionInput<Action>`, then register `sync_action_input_system::<Action>` before gameplay systems that consume action state.
- **Axis Input Flow**: For movement/look/throttle controls, define a small axis enum, insert `AxisBindings<Axis>` and `AxisInput<Axis>`, then register `sync_axis_input_system::<Axis>` before gameplay systems that consume axis values.

### Coding Style
- **ECS-First**: Prefer storing data in Components and logic in Systems or `App` trait implementations.
- **Safety**: Leverage Rust's type system to ensure thread safety and memory management.
- **Explicit Imports**: Use `oxide_engine::prelude::*` for common types, but prefer explicit imports for crate-internal modules.
- **Error Handling**: Use `thiserror` for defining custom error types in library crates (`oxide_renderer`, `oxide_engine`, `oxide_ui`, etc.).
- **Resource Management**: Large GPU resources (Buffers, Textures) should be managed through the `Renderer` or stored as ECS Resources.

### Materials and Shaders
- **Built-in Shaders**: Use `BuiltinShader` (`basic`, `lit`, `unlit`, `sky_gradient`, `sprite_ui`, `fallback`).
- **Custom Shaders**: Load custom shaders through `ShaderSource::File` or `ShaderSource::WgslOwned`.
- **Material Descriptors**: Materials can be defined using legacy JSON/RON/TOML descriptors or versioned `.oxmat` wrappers and loaded via `load_material_descriptor`.

### Performance
- Development profiles use `opt-level = 1` for faster iteration, while dependencies are compiled with `opt-level = 3`.
- Release builds utilize Link Time Optimization (`lto = "thin"`) and single codegen units for maximum performance.

### IMPORTANT NOTES
After every major code refactoring/change make sure to keep README.md and AGENTS.md file updated.
