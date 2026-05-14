# Scene Authoring

Oxide now has a code-first authoring path for small games and prototypes:
describe entities with ECS components, let the engine render common scene
primitives automatically, and layer editor tooling over the same world data.

The canonical runtime types live in focused crates: `oxide_scene` owns
`SceneDescriptor`, `RenderMesh`, and `SceneRenderer`; `oxide_ui` owns `GameUi`,
text rendering, fonts, runtime UI, and debug overlay data; `oxide_editor` owns
`SceneEditor`. `oxide_engine` provides plugin wiring and prelude re-exports.

## Automatic Scene Rendering

`SceneRendererPlugin` installs a non-send `SceneRenderer` resource. The app
runner detects it and automatically:

- updates camera and light GPU buffers during prepare,
- collects `RenderMesh` + `TransformComponent`/`GlobalTransform` entities,
- batches cube and sphere primitives into instance buffers,
- queues the scene pass before `App::queue`,
- resizes the depth texture when the window resizes.

For the common case, a game only needs:

```rust
app::<MyGame>()
    .add_plugins(DefaultPlugins)
    .add_plugin(SceneRendererPlugin)
    .run();
```

Use `SceneAuthoringPlugins` when you also want editor/runtime UI resources:

```rust
app::<MyGame>()
    .add_plugins(DefaultPlugins)
    .add_plugins(SceneAuthoringPlugins)
    .run();
```

## Scene Components

- `SceneDescriptor` is the data format for small native scenes and prefabs.
- `RenderMesh` describes a primitive, material intent, and tint.
- `SceneMaterialDescriptor` includes a `color` field used by the automatic
  renderer.
- `Name`, `Parent`, `Children`, `TransformComponent`, and `GlobalTransform`
  provide scene identity and hierarchy.

## Game UI

`GameUiPlugin` inserts a `GameUi` resource, syncs primitive widgets into
camera-locked scene geometry every frame, and installs Oxide's native overlay
text renderer. It is intended for code-first game UI:

- panels for start, pause, and game-over screens,
- button rectangles for keyboard or gamepad-driven menus,
- bars for health, stamina, cooldowns, and reserve resources,
- counters for ammo, lives, charges, or inventory pips,
- reticles/crosshairs for shooters,
- configurable text labels with color, scale, tracking, line height, wrapping,
  font selection, and horizontal/vertical alignment.
- custom TrueType/OpenType fonts through the `GameFonts` registry.

Games usually clear and rebuild the `GameUi` resource each frame from gameplay
state. See `examples/zombie_shooter` for a complete start menu, pause menu, HUD,
ammo display, text labels, and crosshair.

```rust
let ui = world.resource_mut::<GameUi>();
ui.clear();
ui.text(
    "title",
    GameUiAnchor::Center,
    [0.0, 0.12],
    "ZOMBIE SHOOTER",
    GameTextStyle::new(0.046, [0.93, 1.0, 0.78, 1.0]).with_font("menu_title"),
);
ui.bar(
    "health",
    GameUiAnchor::TopLeft,
    [0.18, -0.08],
    [0.26, 0.035],
    health_ratio,
    [0.72, 0.18, 0.14, 1.0],
);
```

Register custom fonts during `App::init` before using their IDs in
`GameTextStyle`:

```rust
load_game_font(
    world,
    "menu_title",
    "assets/fonts/MenuTitle.ttf",
)?;
```

If a requested font ID is missing, `GameTextRenderer` falls back to the built-in
bitmap font so UI remains visible during development.

## Editor Resource

`SceneEditorPlugin` inserts a `SceneEditor` resource. It supports selection,
spawning cube/sphere entities, duplicating selected entities, recursive delete,
renaming, transform edits, and tint edits.

When an app owns an egui render pass, draw the built-in editor panels with:

```rust
show_scene_authoring_egui(world, egui_ctx);
```

This draws the hierarchy, inspector, runtime UI, and debug overlay models. The
`examples/zombie_shooter` crate shows how to build a larger gameplay prototype
on this same path. The remaining UI/editor work is pointer-based widgets,
viewport picking, transform gizmos, richer font shaping/localization, and an
Oxide-owned egui-wgpu pass so apps do not need local UI rendering glue.
