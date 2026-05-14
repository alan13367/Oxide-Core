# Oxide Core

A 3D game engine built from scratch in Rust, targeting macOS with Metal backend.

## Features

- **Rendering**: wgpu-based abstraction with Metal as primary backend
- **ECS**: custom `oxide_ecs` runtime for entity-component-system architecture
- **Math**: glam for fast 3D math operations
- **Physics**: optional in-house 3D backend via `oxide_physics` (fixed-step simulation, spatial-hash broadphase, warm-started contact manifolds, collision layers/events, OBB cuboid support, ray/sphere cast queries)
- **Audio**: `oxide_audio` playback, software mixing, generated tones, and WAV clip loading via an engine `AudioPlugin`
- **Focused Runtime Crates**: camera, lighting, scene, UI, editor, audio, physics, asset, input, transform, renderer, and ECS code live outside the façade crate behind Oxide-owned APIs
- **Materials + Shaders**: built-in shader pack plus custom WGSL (inline/file) with fallback support
- **Automatic Scene Renderer**: optional plugin that renders `RenderMesh` scene entities without app-owned pipelines
- **Native Sprites**: engine-owned RGBA sprite assets plus billboard components for actors, props, weapons, and overlay sprites
- **Terrain + World Authoring**: heightfield terrain and configurable world descriptors for code-first maps
- **Game UI + Text**: camera-locked panels, buttons, bars, counters, reticles, native styled text widgets, and custom TrueType/OpenType font registration
- **Scene Editor Model**: hierarchy/inspector resource with spawn, select, duplicate, delete, transform, and tint editing APIs
- **Descriptor Pipeline**: JSON, RON, and TOML material descriptors for built-in and project-level shader assets
- **Hot-Reloading**: Automatically reload shader assets during development
- **Robust Validation**: Static checks to ensure custom shaders comply with engine bindings
- **Plugin Architecture**: Group engine setup with `Plugin`/`DefaultPlugins` to reduce app boilerplate
- **Ergonomic Systems**: Signature-driven systems via `IntoSystem` + params (`Res`, `ResMut`, `Query`, `Commands`)
- **Deferred Commands**: Stage-scoped command queue for safe world mutation during iteration
- **State Gating**: Conditionally run systems with `.run_if(in_state(...))`
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

## Usage

Oxide Core uses a data-driven architecture powered by an Entity-Component-System (ECS). Applications are built by implementing the `App` trait and launched with the fluent app builder.

### 1. Start With Scene Authoring Plugins

For code-first games, use `DefaultPlugins` plus `SceneAuthoringPlugins`. The engine will render `RenderMesh` entities automatically, so small games do not need to own a `wgpu::RenderPipeline`, camera buffer, light buffer, depth texture, or primitive GPU mesh.

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

        let scene = SceneDescriptor::starter_scene();
        let roots = spawn_scene_descriptor(&mut world, &scene);
        world.insert_resource(SceneSpawnResult { entities: roots });

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
        .add_system(AppStage::PreUpdate, camera_controller_system)
        .run();
}
```

Use `App::prepare` and `App::queue` only when a project needs a custom render pass or overlay on top of the automatic scene renderer.

### 2. Register Native Sprites And Worlds

Runtime sprite identity is Oxide-native: register a `SpriteImage` in
`SpriteAssets`, then spawn entities with `SpriteBillboard`. The automatic scene
renderer batches those sprites as world billboards or overlay sprites.

```rust
register_sprite(
    &mut world,
    "player.weapon",
    SpriteImage::solid(32, 16, [255, 255, 255, 255])?,
);

world.spawn((
    Name("Weapon".to_string()),
    // Overlay sprites use clip-space X/Y. (-1, -1) is bottom-left.
    TransformComponent::from_position(Vec3::new(0.5, -0.58, 0.0)),
    GlobalTransform::default(),
    SpriteBillboard::new("player.weapon", Vec2::new(0.64, 0.38))
        .with_facing(SpriteFacing::Camera)
        .with_depth(SpriteDepthMode::Overlay),
));
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

    // Deferred until the end of the stage
    commands.spawn(Player::default());
}
```

### 4. State-based Execution

```rust
#[derive(Clone, PartialEq, Eq)]
enum AppState {
    Menu,
    Playing,
}

app::<MyApp>()
    .add_system(AppStage::Update, player_movement.run_if(in_state(AppState::Playing)))
    .run();
```

### 5. Async glTF Scene Spawn Pipeline

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

### 6. Physics Plugin Integration

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
- Load descriptor-driven materials from files via `load_material_descriptor(...)` (Supports JSON, RON, and TOML)

### Hot-Reloading

For a better development experience, you can hot-reload shader assets automatically when files are modified:

```rust
#[cfg(debug_assertions)]
if let Ok(watcher) = AssetWatcher::new("assets/") {
    world.insert_non_send_resource(watcher);
}

// In your `update()` method:
if let Some(mut watcher) = self.world.get_non_send_resource_mut::<AssetWatcher>() {
    let changed_files = watcher.poll_changed_files();
    if !changed_files.is_empty() {
        // Rebuild MaterialPipelines
    }
}
```

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
| `oxide_audio` | Audio playback, generated tones, WAV clips, and software mixing |
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
