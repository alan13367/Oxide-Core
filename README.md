# Oxide Core

A 3D game engine built from scratch in Rust, targeting macOS with Metal backend.

## Features

- **Rendering**: wgpu-based abstraction with Metal as primary backend
- **ECS**: custom `oxide_ecs` runtime for entity-component-system architecture
- **Math**: glam for fast 3D math operations
- **Physics**: optional in-house 3D backend via `oxide_physics` (fixed-step simulation, spatial-hash broadphase, warm-started contact manifolds, collision layers/events, OBB cuboid support, ray/sphere cast queries)
- **Audio**: `oxide_audio` playback, lightweight spatial panning/attenuation, software mixing, generated tones, and WAV clip loading via an engine `AudioPlugin`
- **Focused Runtime Crates**: camera, lighting, scene, UI, editor, audio, physics, asset, input, transform, renderer, and ECS code live outside the façade crate behind Oxide-owned APIs
- **Materials + Shaders**: built-in shader pack plus custom WGSL (inline/file) with fallback support
- **Native Asset Documents**: versioned `.oxscene` and `.oxmat` JSON wrappers with legacy descriptor loading support, nested scene entities, authored scene paths, structured scene instance spawn results, live-world scene/prefab export for editor save flows, scoped query/unload helpers, scene-declared dependencies/materials, registered sprite billboards, reusable scene prefabs with instance overrides, and descriptor validation diagnostics
- **Asset Dependency Tracking**: `AssetServer` records typed source paths, ready or async labeled sub-assets, and secondary source paths; `.oxscene` files can declare scene dependency paths, typed handles can be queried by changed file, native scene/material reloads can be routed together, known paths can be reloaded in place, and `Assets<T>` exposes per-handle revisions plus cursor-readable change records for cache invalidation
- **glTF Import Bridging**: imported glTF meshes, materials, and images are published as labeled Oxide assets, and spawned nodes retain lightweight mesh/material references that the automatic scene renderer can draw through `MeshFilter`
- **Native Scene Reloading**: `.oxscene` handles can be refreshed from direct file changes or dependency changes without duplicating spawned roots
- **Automatic Scene Renderer**: optional plugin that renders `RenderMesh` scene entities without app-owned pipelines
- **Scene Material Library**: reusable named scene materials can be registered directly, declared inside `.oxscene`, populated from loaded `.oxmat` descriptors, and referenced from `.oxscene` meshes
- **Camera Render Views**: ordered multi-camera scene rendering with active flags, normalized viewports, per-camera clear colors, and render-layer filtering
- **Render Layers**: filter meshes, terrain, and sprites by camera/renderable layer masks for world views, first-person overlays, editor-only helpers, and debug cameras
- **Ordered Render Passes**: plugins can register lightweight frame callbacks around stable built-in anchors for scene, text, app queue, and egui rendering
- **Native Sprites**: engine-owned RGBA/PNG sprite assets plus billboard and UI sprite components for actors, props, weapons, and overlays
- **Terrain + World Authoring**: heightfield terrain and configurable world descriptors for code-first maps
- **Game UI + Text**: camera-locked panels, buttons, bars, counters, reticles, native styled text widgets, and custom TrueType/OpenType font registration
- **Scene Editor Model**: hierarchy/inspector resource with spawn, select, duplicate, delete, picking, TRS gizmos, transform, and tint editing APIs
- **Descriptor Pipeline**: JSON, RON, TOML, and `.oxmat` material descriptors for built-in and project-level shader assets
- **Hot-Reloading**: Automatically reload shader assets during development
- **Robust Validation**: Static checks to ensure custom shaders comply with engine bindings
- **Plugin Architecture**: Group engine setup with `Plugin`/`DefaultPlugins`, plugin registration metadata, and duplicate guards to reduce app boilerplate
- **Ergonomic Systems**: Signature-driven systems via `IntoSystem` + params (`Res`, `ResMut`, `Query`, `Commands`) in both app stages and standalone ECS schedules
- **ECS Change Revisions**: `World` tracks component/resource mutation ticks so renderer, asset, editor, and gameplay caches can invalidate only changed data
- **Type Metadata Registry**: lightweight component/resource type identity for editor, scene, and tooling workflows without a reflection dependency
- **System Ordering**: Label systems, group them into sets, and register before/after constraints inside app stages or standalone schedules
- **Startup Schedule**: Register one-shot setup systems with normal ECS params through `AppStage::Startup`
- **System-Requested Exit**: Gameplay and tooling systems can request clean shutdown through the `AppExit` resource
- **Transform Tween Animation**: ECS components and systems animate local transforms with easing, looping, and ping-pong playback
- **Hierarchy Visibility**: Hide renderable entities or whole subtrees with `Visibility` and propagated `InheritedVisibility`
- **Action + Axis Input Mapping**: Bind game-defined actions and movement axes to keyboard/mouse triggers with `ActionBindings`, `AxisBindings`, and sync systems
- **Fixed-Step Scheduling**: Use `AppStage::FixedUpdate` with `FixedTime` for deterministic gameplay ticks inside the normal app runner
- **Runtime Diagnostics**: `DefaultPlugins` records frame time, FPS, and delta seconds into a lightweight `Diagnostics` resource used by tooling and the dev overlay
- **Deferred Commands**: Stage-scoped command queue for safe world mutation during iteration
- **State Gating**: Conditionally run systems with `.run_if(in_state(...))`
- **State Transition Hooks**: Use `state_entered(...)` and `state_exited(...)` for once-per-system mode setup/teardown
- **Dependency Policy**: Oxide owns its ECS, physics, scene hierarchy, and engine runtime; third-party engine/ECS/physics/gameplay frameworks are intentionally avoided

## Requirements

- Rust 1.94 or later
- macOS 10.15+

## Building

```bash
cargo build
```

## Running Examples

The workspace includes several examples demonstrating the engine's rendering capabilities:
```bash
cargo run -p hello_window         # Interactive lit scene
cargo run -p physics_example      # In-house physics demo (falling/collision)
cargo run -p minimal_game         # Minimal code-first game template
cargo run -p zombie_shooter       # FPS zombie shooter with sprites, terrain, UI, audio, and physics
cargo run -p unlit_example        # Basic unlit rendering
cargo run -p sky_gradient_example # Skybox/gradient material demo
cargo run -p sprite_ui_example    # 2D Orthographic overlay material
```

Create a new project scaffold with:

```bash
cargo run -p oxide -- new my_game --name my_game
```

Apps using `SceneAuthoringPlugins` start without editor chrome; press `F1` to
toggle the egui authoring panels.

## Usage

Oxide Core uses a data-driven architecture powered by an Entity-Component-System (ECS). Applications are built by implementing the `App` trait and launched with the fluent app builder.

### 1. Start With Scene Authoring Plugins

For code-first games, use `DefaultPlugins` plus `SceneAuthoringPlugins`. The engine will render `RenderMesh` entities automatically, so small games do not need to own a `wgpu::RenderPipeline`, camera buffer, light buffer, depth texture, or primitive GPU mesh.
Plugins are unique by default; custom plugin authors can override `Plugin::name` for stable diagnostics and `Plugin::is_unique` for intentionally repeatable plugin types.

```rust
use oxide_engine::prelude::*;

struct MyApp {
    world: World,
}

impl App for MyApp {
    fn configure(world: &mut World) {
        world.init_resource::<Time>();
        world.init_resource::<KeyboardInput>();
        world.init_resource::<MouseInput>();
    }

    fn init(window: &Window, renderer: Renderer) -> Self {
        let mut world = World::new();
        Self::configure(&mut world);
        world.insert_resource(RendererResource::new(renderer));
        world.insert_resource(WindowResource::new(window.size().width, window.size().height));

        let _scene_handle = request_oxscene_spawn(
            &mut world,
            "assets/scenes/starter.oxscene",
        );

        Self { world }
    }

    fn world(&self) -> &World { &self.world }
    fn world_mut(&mut self) -> &mut World { &mut self.world }
    fn update(&mut self) {}
    fn on_event(&mut self, _event: EngineEvent) {}
}

fn main() {
    tracing_subscriber::fmt::init();
    app::<MyApp>()
        .add_plugins(DefaultPlugins)
        .add_plugins(SceneAuthoringPlugins)
        .add_system(AppStage::Startup, setup_level)
        .add_system(AppStage::PreUpdate, camera_controller_system)
        .run();
}
```

`AppStage::Startup` runs once after window/app initialization and engine
startup hooks. It uses the same `IntoSystem` signatures as other stages, so
setup code can request `Commands`, `Res`, `ResMut`, queries, events, and
`Res<Window>`:

```rust
fn setup_level(
    mut commands: Commands,
    window: Res<Window>,
    mut ui: ResMut<RuntimeUi>,
) {
    commands.spawn(Player::default());
    ui.label("window", format!("{}x{}", window.size().width, window.size().height));
}
```

Use `App::prepare` and `App::queue` only when a project needs app-owned render
work. Plugins can use ordered render passes instead of taking over
`App::queue`:

```rust
fn queue_damage_flash(world: &mut World, frame: &mut RenderFrame) {
    // Encode an overlay pass using resources prepared during AppStage::Prepare.
}

app::<MyApp>()
    .add_plugins(DefaultPlugins)
    .add_plugins(SceneAuthoringPlugins)
    .add_render_pass_after("game.damage_flash", RENDER_PASS_APP_QUEUE, queue_damage_flash)
    .run();
```

Render pass labels can target `RENDER_PASS_SCENE`, `RENDER_PASS_GAME_TEXT`,
`RENDER_PASS_APP_QUEUE`, and `RENDER_PASS_EGUI`. Use
`add_render_pass_before`, `add_render_pass_after`, render pass sets, and
`RenderPassSchedule::ordering_diagnostics` for plugin-owned overlays,
post-processing, capture passes, or debug drawing without copying the runner.

### 2. Register Native Sprites And Worlds

Runtime sprite identity is Oxide-native: register a `SpriteImage` in
`SpriteAssets`, then spawn entities with `SpriteBillboard`. The automatic scene
renderer batches those sprites as world billboards or overlay sprites.

```rust
register_sprite(
    &mut world,
    "player.weapon",
    SpriteImage::from_png_bytes(include_bytes!("../assets/sprites/weapon.png"))?,
);

let ui = world.resource_mut::<GameUi>();
ui.sprite(
    "weapon",
    "player.weapon",
    GameUiAnchor::BottomRight,
    [-0.16, 0.31],
    [0.32, 0.26],
    [1.0, 1.0, 1.0, 1.0],
);
```

Use `TerrainDescriptor` and `SceneWorldDescriptor` for data-driven terrain and
blockout geometry:

```rust
let world_descriptor = SceneWorldDescriptor {
    terrain: TerrainDescriptor {
        width: 48.0,
        depth: 48.0,
        columns: 56,
        rows: 56,
        height_scale: 1.0,
        waves: vec![TerrainWaveDescriptor::default()],
        ..Default::default()
    },
    objects: Vec::new(),
};

spawn_world_descriptor(&mut world, &world_descriptor);
```

### 3. Ergonomic Systems + Deferred Commands

You can now define systems by declaring dependencies directly in the function signature:

```rust
fn player_movement(
    mut time: ResMut<Time>,
    mut query: Query<(&mut TransformComponent, &Player)>,
    mut commands: Commands,
) {
    for (transform, _player) in query.iter_mut() {
        transform.transform.position.x += time.delta_secs();
    }

    // Deferred until the end of the stage. The entity ID is available now for
    // follow-up commands, hierarchy links, resources, or events.
    let player = commands
        .spawn(Player::default())
        .insert(TransformComponent::default())
        .id();
    commands.attach_child(parent_entity, player);
    commands.insert_resource(LastSpawnedAt(time.delta_secs()));
}
```

Events can be declared directly in systems. Use `EventWriter<T>` to queue,
`EventReader<T>` to inspect the whole buffer, `EventCursor<T>` to read only
events that system has not seen yet, and `EventDrain<T>` when a system owns
consuming the queued messages:

```rust
fn fire(mut writer: EventWriter<GameEvent>) {
    writer.send(GameEvent::Pulse);
}

fn spawn_and_fire(mut commands: Commands) {
    let entity = commands.spawn(Player::default()).id();
    commands.send_event(GameEvent::Spawned(entity));
}

fn handle(mut events: EventDrain<GameEvent>) {
    for event in events.drain() {
        // consume event
    }
}

fn observe_new(mut events: EventCursor<GameEvent>) {
    for event in events.read() {
        // non-consuming incremental read
    }
}
```

Events sent through `Commands::send_event` are deferred with other commands and
become visible after the current schedule or app stage applies its command
queue.

Optional resources are supported both in app code and system parameters. Use
`World::get_resource` / `get_resource_mut` outside systems, or
`Option<Res<T>>` / `Option<ResMut<T>>` in system signatures when a plugin-owned
resource may not be installed:

```rust
fn update_overlay(ui: Option<Res<RuntimeUi>>, mut ticks: Option<ResMut<SimulationTicks>>) {
    if let (Some(ui), Some(mut ticks)) = (ui, ticks) {
        if ui.clicked("step") {
            ticks.0 += 1;
        }
    }
}
```

Use `Local<T>` for small per-system state that should persist across runs but
should not become a global world resource:

```rust
fn frame_counter(mut counter: Local<u64>, mut ui: ResMut<RuntimeUi>) {
    *counter += 1;
    ui.label("frames", format!("Frames: {}", *counter));
}
```

Store `World::change_tick()` when building a cache and compare it later with
`component_changed_since::<T>` or `resource_changed_since::<T>`. Mutable
component/resource access records a new revision automatically. Systems can use
`Query<(Entity, &T)>::iter_added_since(tick)` to initialize caches for newly
inserted components, `iter_changed_since(tick)` to refresh changed components,
`RemovedComponents<T>` to clean up removed component cache entries, and
`ResourceCursor<T>` to observe resource changes once per system instance:

```rust
let last_sync = world.change_tick();
let entity = world.spawn(TransformComponent::default()).id();

let mut query = world.query::<(Entity, &TransformComponent)>();
for (entity, transform) in query.iter_added_since(&world, last_sync) {
    // create renderer/editor cache entries for this entity and transform
}

let mut query = world.query::<(Entity, &TransformComponent)>();
for (entity, transform) in query.iter_changed_since(&world, last_sync) {
    // refresh a renderer/editor cache for this entity and transform
}

let mut changed_transforms = world.query_filtered::<(Entity, &TransformComponent), Changed<TransformComponent>>();
for (entity, transform) in changed_transforms.iter_since(&world, last_sync) {
    // same change filter through filtered-query syntax
}
```

```rust
fn sync_settings(mut settings: ResourceCursor<RenderSettings>) {
    if let Some(settings) = settings.read_if_changed() {
        // rebuild settings-dependent caches
    }
}
```

```rust
fn sync_transforms(mut transforms: ComponentChanges<TransformComponent>) {
    for (entity, transform) in transforms.added() {
        // create cache entries
    }
    for (entity, transform) in transforms.read_changed() {
        // refresh cache entries and advance this system's cursor
    }
}
```

```rust
fn cleanup_mesh_cache(mut removed: RemovedComponents<RenderMesh>, mut cache: ResMut<MeshCache>) {
    for record in removed.read() {
        cache.remove(record.entity);
    }
}
```

After every consumer has advanced, long-running tools can prune retained removal
history with `World::prune_removed_components_through::<T>(revision)`.

### 4. State-based Execution

```rust
#[derive(Clone, PartialEq, Eq)]
enum AppState {
    Menu,
    Playing,
}

app::<MyApp>()
    .add_system(AppStage::FixedUpdate, fixed_tick_system)
    .add_system(AppStage::Update, player_movement.run_if(in_state(AppState::Playing)))
    .add_system(AppStage::Update, open_menu.run_if(state_entered(AppState::Menu)))
    .add_system(AppStage::Update, close_menu.run_if(state_exited(AppState::Menu)))
    .run();
```

`State<T>` records the previous value and a transition revision whenever
`set(...)` or `apply_transition()` changes state. Use `state_entered(...)` and
`state_exited(...)` for once-per-system transition hooks such as menu setup,
level teardown, or audio stingers without adding a separate global event.

### 5. System Ordering

Systems run in insertion order unless a stage or standalone `Schedule` has
explicit labels, sets, and ordering constraints. Labels and sets are local to
one schedule or app stage, so plugins can expose stable targets without
coupling unrelated stages.

```rust
app::<MyApp>()
    .add_labeled_system_to_set(AppStage::PreUpdate, "game.input.actions", "game.input", sync_input)
    .add_system_after(AppStage::PreUpdate, "game.input", camera_controller_system)
    .add_system_before(AppStage::PostUpdate, TRANSFORM_PROPAGATE_SYSTEM, gameplay_follow_system)
    .run();
```

Before/after targets can reference either a system label or a set name.
`Schedule::ordering_diagnostics()` reports missing targets, duplicate labels,
and cycles. Schedules still run with a stable fallback order, which keeps
development builds usable while surfacing authoring mistakes to tooling.

### 6. Fixed-Step Systems

`DefaultPlugins` installs `FixedTime`, which drives `AppStage::FixedUpdate`
after `PreUpdate` and before normal `Update` systems. Fixed systems may run
zero or more times per rendered frame.

```rust
#[derive(Resource, Default)]
struct SimulationTicks(u64);

fn fixed_tick(mut ticks: ResMut<SimulationTicks>, fixed: Res<FixedTime>) {
    ticks.0 += 1;
    let dt = fixed.timestep_secs();
}

app::<MyApp>()
    .add_system(AppStage::FixedUpdate, fixed_tick)
    .run();
```

### 7. Runtime Diagnostics

`DefaultPlugins` installs `FrameDiagnosticsPlugin`, which records frame timing
into `Diagnostics` during `AppStage::PreUpdate`. These values are intentionally
simple scalar streams so gameplay code, editor UI, logging, and tests can share
the same source without a heavy profiling dependency.

```rust
fn debug_metrics(diagnostics: Res<Diagnostics>) {
    let fps = diagnostics.latest(FPS).unwrap_or_default();
    let frame_ms = diagnostics.average(FRAME_TIME_MS).unwrap_or_default();
}
```

Built-in labels include `DELTA_SECONDS`, `FRAME_TIME_MS`, and `FPS`. Custom
tools can call `diagnostics.record("game.visible_enemies", enemies as f64)` to
append their own rolling samples. `SceneAuthoringPlugins` feeds these frame
metrics into the egui dev overlay.

### 8. Action Input Mapping

Bind semantic game actions and axes once, then read input state from systems or
app logic without hard-coding raw keys everywhere:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum GameAction {
    Jump,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum GameAxis {
    MoveX,
}

world.insert_resource(ActionInput::<GameAction>::default());
world.insert_resource(AxisInput::<GameAxis>::default());
let mut bindings = ActionBindings::default();
bindings.bind_key(GameAction::Jump, KeyCode::Space);
world.insert_resource(bindings);

let mut axes = AxisBindings::default();
axes.bind_key_pair(GameAxis::MoveX, KeyCode::KeyA, KeyCode::KeyD);
world.insert_resource(axes);

app::<MyApp>()
    .add_system(AppStage::PreUpdate, sync_action_input_system::<GameAction>)
    .add_system(AppStage::PreUpdate, sync_axis_input_system::<GameAxis>)
    .run();
```

`ActionInput::pressed`, `just_pressed`, and `just_released` provide
button-style state for gameplay code. `AxisInput::value` provides clamped
`-1.0..=1.0` values for movement-style controls. `minimal_game` demonstrates
using Space as an action-bound spawn command and A/D as an axis-bound input.

### 9. Async glTF Scene Spawn Pipeline

`DefaultPlugins` registers asset resources and a glTF resolve/spawn system. Request a load, then consume spawned roots once ready:

```rust
let scene_handle = request_gltf_scene_spawn(
    &mut self.world,
    Arc::clone(&renderer.device),
    Arc::clone(&renderer.queue),
    "assets/models/scene.gltf",
);

if let Some(roots) = take_spawned_scene_roots(&mut self.world, scene_handle) {
    tracing::info!("Spawned {} root entities from glTF", roots.len());
}
```

Loaded glTF meshes are published into `MeshCache` as labeled mesh assets, and
glTF materials/images are published into `MaterialDescriptorAssets` and
`TextureImageAssets` as labeled assets when the source path is known. Spawned
mesh nodes receive `GltfMeshRef`/`GltfMaterialRef` plus
`MeshFilter`/`MaterialFilter` when stable handles are available. The automatic
scene renderer draws `MeshFilter` entities from `MeshCache`; if the entity also
has a `RenderMesh`, its material, tint, and labeled albedo texture are used for
the imported mesh instead of drawing a built-in primitive. Spawned glTF entities
are tagged with `GltfSceneInstance`, so reloading the same glTF handle replaces
the previous imported hierarchy instead of duplicating stale entities. External
glTF buffer and image URIs are recorded as dependencies, so sidecar `.bin` and
texture changes can drive `reload_changed_gltf_scenes(...)`.

### 10. Physics Plugin Integration

Physics is provided by `oxide_physics` and intentionally exported through `oxide_physics::prelude`:

```rust
use oxide_engine::prelude::*;
use oxide_physics::prelude::*;

app::<MyApp>()
    .add_plugins(DefaultPlugins)
    .add_plugin(PhysicsPlugin)
    .run();
```

Current physics runtime highlights include:

- Contact manifold generation with warm-started sequential impulses
- Spatial-hash broadphase candidate generation with collision-layer filtering
- Joint support (`Fixed`, `Hinge`, `BallSocket`, `Spring`)
- Query API (`raycast`, `raycast_all`, `sphere_cast`, `overlaps_sphere`, `overlaps_box`)
- Collision event lifecycle (`Started`, `Persisted`, `Ended`)

## Shader Workflow

- Use built-in shaders through `BuiltinShader` (`basic`, `lit`, `unlit`, `sky_gradient`, `sprite_ui`, `fallback`)
- Load custom shaders through `ShaderSource::File` or `ShaderSource::WgslOwned`
- Build pipelines through `MaterialPipeline` with optional fallback behavior
- Load descriptor-driven materials from files via `load_material_descriptor(...)` (supports legacy JSON/RON/TOML plus versioned `.oxmat`, including material `base_color`)
- Load material descriptors asynchronously with `request_material_descriptor_load(...)`; `DefaultPlugins` publishes ready descriptors into `MaterialDescriptorAssets`, registers them in `SceneMaterialLibrary` by descriptor name/base color, and tracks shader/texture dependencies for in-place reloads

### Hot-Reloading

For a better development experience, you can hot-reload shader assets automatically when files are modified:

```rust
#[cfg(debug_assertions)]
if let Ok(watcher) = AssetWatcher::new("assets/") {
    world.insert_non_send_resource(watcher);
}

// In your `update()` method:
let reloads = poll_native_asset_reloads(&mut self.world);
if !reloads.is_empty() {
    tracing::info!("Reloading native assets: {:?}", reloads.changed_paths);
}
```

glTF scenes that were loaded through the renderer can be reloaded with
`reload_gltf_scene_path(...)` or `reload_changed_gltf_scenes(...)` when the app
has access to the renderer device and queue.

See `docs/shader_material_roadmap.md` for roadmap, implementation status, and API semver guarantees.
See `docs/building_games.md` for the current game authoring path.
See `docs/scene_authoring.md` for the automatic scene renderer and editor model.

## Crates

### crates.io package names

- `oxide-core-engine` (library crate name: `oxide_engine`)
- `oxide-core-renderer` (library crate name: `oxide_renderer`)
- `oxide-core-asset` (library crate name: `oxide_asset`)
- `oxide-core-audio` (library crate name: `oxide_audio`)
- `oxide-core-camera` (library crate name: `oxide_camera`)
- `oxide-core-light` (library crate name: `oxide_light`)
- `oxide-core-scene` (library crate name: `oxide_scene`)
- `oxide-core-ui` (library crate name: `oxide_ui`)
- `oxide-core-editor` (library crate name: `oxide_editor`)
- `oxide-core-input` (library crate name: `oxide_input`)
- `oxide-core-transform` (library crate name: `oxide_transform`)
- `oxide-core-math` (library crate name: `oxide_math`)
- `oxide-core-physics` (library crate name: `oxide_physics`)
- `oxide_ecs`
- `oxide_ecs_derive`

| Crate | Description |
|-------|-------------|
| `oxide_engine` | Facade crate for app lifecycle, plugin wiring, window/event loop integration, and compatibility prelude |
| `oxide_input` | Layout-stable keyboard/mouse input resources (`PhysicalKey`-based) |
| `oxide_ecs` | Custom ECS runtime (world, entities, storage, resources, queries, events) |
| `oxide_ecs_derive` | Proc-macro derives for ECS traits (`Component`, `Resource`, `ScheduleLabel`) |
| `oxide_asset` | Generic asset handles, typed storage, and async asset loading primitives |
| `oxide_audio` | Audio playback, generated tones, WAV clips, spatial panning/attenuation, and software mixing |
| `oxide_camera` | Camera components, FPS controller system, and GPU camera buffer helpers |
| `oxide_light` | Light components, GPU light uniforms, and light buffer update helpers |
| `oxide_scene` | Scene descriptors, sprites, terrain/world descriptors, renderable scene components, transform re-exports, and automatic scene renderer |
| `oxide_ui` | Native game UI widgets, text/font rendering, egui bridge, runtime UI, and debug overlay data |
| `oxide_editor` | Runtime scene editor resource and egui hierarchy/inspector surface |
| `oxide_transform` | Transform + hierarchy components and dirty-aware propagation |
| `oxide_renderer` | Low-level wgpu rendering abstraction and material descriptors |
| `oxide_math` | Math types and utilities leveraging `glam` |
| `oxide_physics` | In-house 3D physics crate with ECS-first components, systems, and `PhysicsPlugin` |

The crate layout is being evolved toward a Bevy-style distribution of focused domain crates plus a stable facade. See `docs/bevy_style_crate_distribution.md` for the active structure plan, `docs/dependency_policy.md` for dependency boundary rules, and `docs/asset_pipeline.md` for the importer-to-runtime asset direction.

## License

MIT OR Apache-2.0
