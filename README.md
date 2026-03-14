# Oxide Core

A 3D game engine built from scratch in Rust, targeting macOS with Metal backend.

## Features

- **Rendering**: wgpu-based abstraction with Metal as primary backend
- **ECS**: bevy_ecs for entity-component-system architecture
- **Math**: glam for fast 3D math operations
- **Materials + Shaders**: built-in shader pack plus custom WGSL (inline/file) with fallback support
- **Descriptor Pipeline**: JSON, RON, and TOML material descriptors for built-in and project-level shader assets
- **Hot-Reloading**: Automatically reload shader assets during development
- **Robust Validation**: Static checks to ensure custom shaders comply with engine bindings

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

Oxide Core uses a data-driven architecture powered by an Entity-Component-System (ECS). Applications are built by implementing the `App` trait.

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

    // 4. Submit draw calls via the renderer (called after update)
    fn render(&mut self) {
        // Build wgpu command encoders, render passes, and draw meshes
    }

    // 5. Handle system and windowing events
    fn on_event(&mut self, event: EngineEvent) {
        if let EngineEvent::Resized { width, height } = event {
            // Resize renderer and depth textures
        }
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    run_app::<MyApp>(); // Bootstrap the event loop
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

| Crate | Description |
|-------|-------------|
| `oxide_engine` | Core engine systems, `App` orchestration, and ECS integration |
| `oxide_renderer` | Low-level wgpu rendering abstraction and material descriptors |
| `oxide_math` | Math types and utilities leveraging `glam` |

## License

MIT OR Apache-2.0