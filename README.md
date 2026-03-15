# Oxide Core

A 3D game engine built from scratch in Rust, targeting macOS with Metal backend.

## Features

- **Rendering**: wgpu-based abstraction with Metal as primary backend
- **ECS**: custom `oxide_ecs` runtime for entity-component-system architecture
- **Math**: glam for fast 3D math operations
- **Materials + Shaders**: built-in shader pack plus custom WGSL (inline/file) with fallback support
- **Descriptor Pipeline**: JSON, RON, and TOML material descriptors for built-in and project-level shader assets
- **Hot-Reloading**: Automatically reload shader assets during development
- **Robust Validation**: Static checks to ensure custom shaders comply with engine bindings
- **Plugin Architecture**: Group engine setup with `Plugin`/`DefaultPlugins` to reduce app boilerplate
- **Ergonomic Systems**: Signature-driven systems via `IntoSystem` + params (`Res`, `ResMut`, `Query`, `Commands`)
- **Deferred Commands**: Stage-scoped command queue for safe world mutation during iteration
- **State Gating**: Conditionally run systems with `.run_if(in_state(...))`

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
cargo run -p unlit_example        # Basic unlit rendering
cargo run -p sky_gradient_example # Skybox/gradient material demo
cargo run -p sprite_ui_example    # 2D Orthographic overlay material
```

## Usage

Oxide Core uses a data-driven architecture powered by an Entity-Component-System (ECS). Applications are built by implementing the `App` trait and launched with the fluent app builder.

### 1. Implement the `App` Trait

The `App` trait provides a complete lifecycle for setting up resources, running the simulation, rendering the frame, and handling window events:

```rust
use oxide_engine::prelude::*;

struct MyApp {
    world: World,
    pipeline: wgpu::RenderPipeline,
    // ... other resources
}

impl App for MyApp {
    // 1. Setup fundamental ECS resources
    fn configure(world: &mut World) {
        world.init_resource::<Time>();
        world.init_resource::<KeyboardInput>();
        world.init_resource::<MouseInput>();
    }

    // 2. Initialize application state, spawn entities, build pipelines
    fn init(window: &Window, renderer: Renderer) -> Self {
        let mut world = World::new();
        Self::configure(&mut world);
        
        world.insert_resource(RendererResource::new(renderer));
        
        // Spawn ECS entities here...
        
        Self { world, /* ... */ }
    }

    // Required accessors
    fn world(&self) -> &World { &self.world }
    fn world_mut(&mut self) -> &mut World { &mut self.world }
    
    // 3. Update ECS state (called every frame)
    fn update(&mut self) {
        let time = self.world.resource::<Time>();
        // Query components and run logic...
    }

    // 4. Extract transient render data from the main world
    fn extract(&mut self) {}

    // 5. Prepare GPU data (uniform/storage buffers, bind groups, etc.)
    fn prepare(&mut self) {}

    // 6. Queue draw calls into the engine-provided frame context
    fn queue(&mut self, frame: &mut RenderFrame) {
        // Build render passes using frame.encoder and frame.view
    }

    // 7. Handle system and windowing events
    fn on_event(&mut self, event: EngineEvent) {
        if let EngineEvent::Resized { width, height } = event {
            // Resize renderer and depth textures
        }
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    app::<MyApp>()
        .add_plugins(DefaultPlugins)
        .add_system(AppStage::PreUpdate, camera_controller_system)
        .run();
}
```

### 2. Ergonomic Systems + Deferred Commands

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

### 3. State-based Execution

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

### 4. Async glTF Scene Spawn Pipeline

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

## Crates

### crates.io package names

- `oxide-core-engine` (library crate name: `oxide_engine`)
- `oxide-core-renderer` (library crate name: `oxide_renderer`)
- `oxide-core-asset` (library crate name: `oxide_asset`)
- `oxide-core-input` (library crate name: `oxide_input`)
- `oxide-core-transform` (library crate name: `oxide_transform`)
- `oxide-core-math` (library crate name: `oxide_math`)
- `oxide_ecs`
- `oxide_ecs_derive`

| Crate | Description |
|-------|-------------|
| `oxide_engine` | Facade crate exposing prelude and high-level engine APIs |
| `oxide_input` | Layout-stable keyboard/mouse input resources (`PhysicalKey`-based) |
| `oxide_ecs` | Custom ECS runtime (world, entities, storage, resources, queries) |
| `oxide_ecs_derive` | Proc-macro derives for ECS traits (`Component`, `Resource`, `ScheduleLabel`) |
| `oxide_asset` | Asset handles and runtime mesh cache primitives |
| `oxide_transform` | Transform + hierarchy components and dirty-aware propagation |
| `oxide_renderer` | Low-level wgpu rendering abstraction and material descriptors |
| `oxide_math` | Math types and utilities leveraging `glam` |

The crate layout is being evolved toward a Bevy-style distribution of focused domain crates plus a stable facade. See `docs/bevy_style_crate_distribution.md` for the active structure plan.

## License

MIT OR Apache-2.0
