# Scene Authoring

Oxide now has a code-first authoring path for small games and prototypes:
describe entities with ECS components, let the engine render common scene
primitives automatically, and layer editor tooling over the same world data.

The canonical runtime types live in focused crates: `oxide_scene` owns
`SceneDescriptor`, `RenderMesh`, `SpriteAssets`, `SpriteBillboard`, `Terrain`,
`SceneWorldDescriptor`, and `SceneRenderer`; `oxide_ui` owns `GameUi`, text
rendering, fonts, runtime UI, and debug overlay data; `oxide_editor` owns
`SceneEditor`. `oxide_engine` provides plugin wiring and prelude re-exports.

## Automatic Scene Rendering

`SceneRendererPlugin` installs a non-send `SceneRenderer` resource. The app
runner detects it and automatically:

- updates camera and light GPU buffers during prepare,
- collects `RenderMesh` + `TransformComponent`/`GlobalTransform` entities,
- batches cube and sphere primitives into instance buffers,
- turns `Terrain` components into heightfield meshes,
- batches `SpriteBillboard` entities into world or overlay sprite passes,
- prepares one draw packet per active `CameraRenderView`, in ascending order,
- queues the scene pass at the `RENDER_PASS_SCENE` anchor before
  `App::queue`,
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

Plugins that need custom drawing can target stable render pass anchors instead
of replacing the automatic scene renderer:

```rust
app::<MyGame>()
    .add_plugins(DefaultPlugins)
    .add_plugins(SceneAuthoringPlugins)
    .add_render_pass_before("game.capture", RENDER_PASS_EGUI, queue_capture_overlay)
    .run();
```

The built-in anchors are `RENDER_PASS_SCENE`, `RENDER_PASS_GAME_TEXT`,
`RENDER_PASS_APP_QUEUE`, and `RENDER_PASS_EGUI`.

## Scene Components

- `SceneDescriptor` is the data format for small native scenes and prefabs.
- `RenderMesh` describes a primitive, material intent, and tint.
- `SceneSpriteDescriptor` describes sprite billboard entities inside `.oxscene`
  files by referencing a registered `SpriteId`.
- `SpriteAssets` stores engine-native RGBA/PNG sprite images by `SpriteId`.
- `SpriteBillboard` attaches a registered sprite to an entity as a world
  billboard, fixed-orientation sprite, or overlay weapon/HUD sprite.
- `Terrain` stores a heightfield mesh with tint/material intent.
- `Visibility` hides renderable entities and propagates through hierarchy
  children via `InheritedVisibility`.
- `RenderLayers` filters meshes, terrain, and sprites against the active
  camera's layer mask. Cameras and renderables default to layer `0`.
- `CameraRenderView` renders active cameras deterministically by `order`,
  supports disabled cameras, normalized viewports, and scene clear overrides.
- `TerrainDescriptor` and `SceneWorldDescriptor` describe terrain and blockout
  objects for code-first maps.
- `SceneMaterialDescriptor` includes a `color` field used by the automatic
  renderer.
- `Name`, `Parent`, `Children`, `TransformComponent`, and `GlobalTransform`
  provide scene identity and hierarchy.

## Prefabs And Children

`.oxscene` files can define reusable prefabs in `SceneDescriptor::prefabs` and
instantiate them with `"type": "prefab"`. Prefab instances spawn as empty root
entities; the prefab contents are attached as children so translating, rotating,
or scaling the instance root moves the whole reusable object.
Scene entities also support a `visible` field; setting it to `false` spawns
`Visibility::Hidden` and hides that entity's subtree from the scene renderer
without despawning it. Add `render_layers` with a raw bit mask when authored
entities should only render through cameras on matching layers.
Camera entities also support `order`, `active`, `viewport`, and `clear_color`
fields for ordered multi-camera views, normalized target rectangles, and frame
clear behavior. The first active view clears the frame; later views load the
existing color target so viewports can be composited.

```json
{
  "format": "oxide.oxscene",
  "version": 1,
  "scene": {
    "prefabs": [
      {
        "id": "crate_pair",
        "entities": [
          {
            "name": "Crate Base",
            "type": "mesh",
            "primitive": "cube",
            "render_layers": 1,
            "children": [
              {
                "name": "Crate Top",
                "transform": { "position": [0.0, 1.15, 0.0] },
                "type": "mesh",
                "primitive": "cube"
              }
            ]
          }
        ]
      }
    ],
    "entities": [
      {
        "name": "Crate Pair",
        "transform": { "position": [2.0, 0.0, -1.5] },
        "type": "prefab",
        "id": "crate_pair"
      },
      {
        "name": "Gameplay Camera",
        "type": "camera",
        "order": -1,
        "active": true,
        "viewport": [0.75, 0.0, 0.25, 0.25],
        "clear_color": [0.02, 0.03, 0.04, 1.0],
        "render_layers": 1
      }
    ]
  }
}
```

Code can also instantiate a prefab from an already loaded descriptor:

```rust
let root = try_spawn_scene_prefab(
    world,
    &scene,
    "crate_pair",
    SceneTransform::from_position([4.0, 0.0, -3.0]),
)?;
```

Native scene loads validate authored data before publishing the descriptor.
Validation reports duplicate prefab IDs, missing prefab references, recursive
prefab graphs, and empty sprite IDs with descriptor paths such as
`entities[0].children[1].id`. Use `SceneDescriptor::validate()` in editor tools
and `try_spawn_scene_descriptor` / `try_spawn_scene_prefab` when code wants
structured `SceneValidationError` diagnostics before mutating the world.

## Native Sprites

Sprites are runtime-native Oxide assets. Import tools or game code create
`SpriteImage` RGBA data, register it under a stable ID, and entities reference
that ID through `SpriteBillboard`.

```rust
let image = SpriteImage::from_ascii(
    &[" A ", "AAA", " A "],
    &[(' ', [0, 0, 0, 0]), ('A', [80, 210, 70, 255])],
)?;
register_sprite(world, "enemy.basic", image);

world.spawn((
    Name("Enemy".to_string()),
    TransformComponent::from_position(Vec3::new(0.0, 0.9, -6.0)),
    GlobalTransform::default(),
    SpriteBillboard::new("enemy.basic", Vec2::new(1.2, 1.8))
        .with_facing(SpriteFacing::YBillboard)
        .with_depth(SpriteDepthMode::World),
));
```

The same registered sprite can be referenced from a native scene descriptor:

```json
{
  "name": "Enemy",
  "transform": { "position": [0.0, 0.9, -6.0] },
  "type": "sprite",
  "sprite": "enemy.basic",
  "size": [1.2, 1.8],
  "facing": "y_billboard",
  "depth": "world",
  "tint": [1.0, 1.0, 1.0, 1.0]
}
```

With the `image-import` feature enabled, games can load PNG/JPEG bytes directly
into a `SpriteImage`:

```rust
let zombie = SpriteImage::from_png_bytes(include_bytes!("../assets/sprites/zombie.png"))?;
register_sprite(world, "enemy.zombie", zombie);
```

`GameUi` can also place a registered sprite in the same camera-locked layer as
HUD bars, counters, and text:

```rust
ui.sprite(
    "weapon",
    "player.shotgun",
    GameUiAnchor::BottomRight,
    [-0.16, 0.31],
    [0.32, 0.26],
    [1.0, 1.0, 1.0, 1.0],
);
```

Use `SpriteDepthMode::Overlay` for first-person weapons or screen-space props
that should draw over the scene. Overlay sprite transform positions are
clip-space coordinates, so `Vec3::new(0.5, -0.58, 0.0)` places a sprite near the
lower-right of the screen. Re-registering the same `SpriteId` increments its
revision; the scene renderer refreshes the GPU texture automatically.

## Terrain And Worlds

`Terrain` is a heightfield component rendered by `SceneRenderer`. `Terrain`
can be edited directly with `set_height`, generated with `from_height_fn`, or
created from a serializable `TerrainDescriptor`.

```rust
let descriptor = SceneWorldDescriptor {
    terrain: TerrainDescriptor {
        width: 64.0,
        depth: 64.0,
        columns: 64,
        rows: 64,
        height_scale: 1.4,
        waves: vec![TerrainWaveDescriptor {
            direction: [1.0, 0.35],
            frequency: 5.0,
            amplitude: 0.08,
            phase: 0.0,
        }],
        ..Default::default()
    },
    objects: vec![WorldObjectDescriptor {
        name: "Cover".to_string(),
        position: [4.0, 0.6, -8.0],
        size: [2.5, 1.2, 2.5],
        tint: [0.38, 0.33, 0.27, 1.0],
        solid: true,
    }],
};

let spawned = spawn_world_descriptor(world, &descriptor);
world.insert_resource(spawned);
```

Physics stays explicit: games decide which spawned world objects are solid and
attach `oxide_physics` colliders that match their gameplay needs.

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
debug overlay reads `FRAME_TIME_MS` and `FPS` from the shared `Diagnostics`
resource when `DefaultPlugins` are installed, and tools can add their own
streams with `Diagnostics::record`.

The `examples/zombie_shooter` crate shows how to build a larger gameplay prototype
on this same path. The remaining UI/editor work is pointer-based widgets,
viewport picking, transform gizmos, richer font shaping/localization, and an
Oxide-owned egui-wgpu pass so apps do not need local UI rendering glue.
