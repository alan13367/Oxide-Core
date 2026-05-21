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
- `MaterialFilter` can point at a loaded `MaterialDescriptorAssets` handle; the
  automatic renderer resolves it before falling back to `RenderMesh.material`.
- `SceneMaterialLibrary` stores reusable named material intents. The automatic
  renderer resolves `RenderMaterial::Named` through it before batching.
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
- `RenderBounds` gives imported or procedural mesh entities a local-space
  sphere for per-camera frustum culling. Built-in primitives, terrain, and
  sprites infer bounds automatically. `RenderCullDistance` adds a simple
  maximum camera distance gate for large worlds.
- `SceneEntityPath` stores the slash-separated authored path assigned during
  scene and prefab spawning. `SceneInstanceId` scopes those paths and tags to
  one spawned copy. Use `entity_by_scene_path_in_instance`,
  `entities_under_scene_path_in_instance`, and `scene_entity_path` when code or
  tools need stable child lookup such as `Level/Player Spawn`. Use
  `despawn_scene_instance` to unload one spawned copy and detach preserved
  external hierarchy links.
- `Tags` stores stable authored labels such as `enemy`, `spawn_point`, or
  `pickup` for gameplay queries and editor/tooling filters. Use
  `entities_with_tag_in_instance`, `first_entity_with_tag_in_instance`, and
  `entity_has_tag` when code needs authored scene markers without depending on
  display names.
- `CameraRenderView` renders active cameras deterministically by `order`,
  supports disabled cameras, normalized viewports, and scene clear overrides.
- `TerrainDescriptor` and `SceneWorldDescriptor` describe terrain and blockout
  objects for code-first maps.
- `SceneMaterialDescriptor` includes `color`, `alpha_mode`, and optional
  albedo/normal/metallic/roughness texture fields used by the automatic
  renderer.
- `Name`, `Parent`, `Children`, `TransformComponent`, and `GlobalTransform`
  provide scene identity and hierarchy.

## Prefabs And Children

`.oxscene` files can define reusable prefabs in `SceneDescriptor::prefabs` and
instantiate them with `"type": "prefab"`. Prefab instances spawn as empty root
entities; the prefab contents are attached as children so translating, rotating,
or scaling the instance root moves the whole reusable object. Instances can also
provide `overrides` for named prefab children when one placement needs a
different child transform, visibility, render layer, render bounds, cull
distance, or mesh material without duplicating the whole prefab.
Scene entities also support a `visible` field; setting it to `false` spawns
`Visibility::Hidden` and hides that entity's subtree from the scene renderer
without despawning it. Add `render_layers` with a raw bit mask when authored
entities should only render through cameras on matching layers. Add
`render_bounds` as `{ "center": [x, y, z], "radius": r }` and
`render_cull_distance` when authored custom meshes need explicit culling data.
Camera entities also support `order`, `active`, `viewport`, and `clear_color`
fields for ordered multi-camera views, normalized target rectangles, and frame
clear behavior. The first active view clears the frame; later views load the
existing color target so viewports can be composited.

```json
{
  "format": "oxide.oxscene",
  "version": 1,
  "scene": {
    "dependencies": [
      "materials/crate_lit.oxmat"
    ],
    "materials": [
      {
        "name": "crate_lit",
        "shader": "lit",
        "alpha_mode": "mask",
        "color": [0.9, 0.7, 0.45, 1.0],
        "albedo_texture": "#crate_albedo"
      }
    ],
    "prefabs": [
      {
        "id": "crate_pair",
        "entities": [
          {
            "name": "Crate Base",
            "tags": ["prop", "crate"],
            "type": "mesh",
            "primitive": "cube",
            "render_layers": 1,
            "children": [
              {
                "name": "Crate Top",
                "transform": { "position": [0.0, 1.15, 0.0] },
                "type": "mesh",
                "primitive": "cube",
                "material": { "ref": "crate_lit", "color": [0.8, 0.7, 0.55, 1.0] }
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
        "id": "crate_pair",
        "overrides": [
          {
            "path": "Crate Base/Crate Top",
            "transform": { "position": [0.0, 1.35, 0.0] },
            "material": { "ref": "crate_lit", "color": [1.0, 0.45, 0.35, 1.0] }
          }
        ]
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
let spawned = try_spawn_scene_prefab_instance(
    world,
    &scene,
    "crate_pair",
    SceneTransform::from_position([4.0, 0.0, -3.0]),
)?;
let Some(spawned) = spawned else {
    return Ok(());
};
let root = spawned.root().unwrap();
```

Native scene loads validate authored data before publishing the descriptor.
Use top-level `dependencies` for material, sprite, import, or sidecar data files
that should trigger scene reloads when they change. Relative paths are resolved
from the `.oxscene` file location when the scene is loaded through the runtime
asset path. Non-virtual scene material `albedo_texture`, `normal_texture`,
`metallic_texture`, and `roughness_texture` paths are also recorded as
dependencies automatically and published into `TextureImageAssets` during native
scene loading.
Use entity `tags` for stable gameplay labels independent of display names.
Systems can query `Tags` directly or use `first_entity_with_tag(&mut world,
"spawn_point")` / `entities_with_tag(&mut world, "enemy")` instead of parsing
names.
Spawned entities also receive `SceneInstanceId` and `SceneEntityPath`
components. APIs such as `try_spawn_scene_prefab_instance` and
`spawn_scene_descriptor_instance` return a `SpawnedSceneInstance` with the
assigned instance ID and roots, so gameplay and editor code can keep the handle
for scoped lookups or later unloads. Named path segments use authored names,
while unnamed siblings use `#index`. When one scene can be spawned more than
once, use the returned instance ID or recover it from a root with
`scene_instance_id(&world, root)`, then call
`entity_by_scene_path_in_instance(&mut world, instance, "Crate Pair/Crate Base")`
or `entities_under_scene_path_in_instance(&mut world, instance, "Encounter A")`
after a scene loads.
Spawn helpers also populate `SceneInstanceRegistry`; `scene_instance_index`
returns the cached authored path/tag index for a loaded instance when gameplay
or editor code needs repeated lookups without scanning the whole world.
When a loaded copy is no longer needed, call `despawn_scene_instance(&mut world,
instance)`. It removes entities in that instance and detaches any external
parents or children that were linked to the scene at runtime.
Prefab override paths use slash-separated entity names, such as
`"Crate Base/Crate Top"`. Unnamed prefab entities can be addressed by sibling
index segments such as `"#0/#1"`.

Validation reports duplicate or empty dependency paths, duplicate material
names, empty material names, duplicate prefab IDs, missing prefab references,
recursive prefab graphs, invalid prefab override paths, duplicate/empty entity
tags, and empty sprite IDs with descriptor paths such as
`entities[0].children[1].id`. Use `SceneDescriptor::validate()` in editor tools
and `try_spawn_scene_descriptor` / `try_spawn_scene_prefab` when code wants
structured `SceneValidationError` diagnostics before mutating the world.

## Exporting Edited Scenes

Editor and tooling code can serialize live scene entities back into Oxide's
native descriptor format:

```rust
let scene = scene_descriptor_from_world(&mut world)?;
save_scene_descriptor("assets/scenes/edited.oxscene", &scene)?;
```

`scene_descriptor_from_world` exports transform-bearing root entities and their
children. Use `scene_descriptor_from_roots(&world, roots)` when saving only a
selection, prefab candidate, or imported subtree. The exporter preserves
hierarchy order plus supported Oxide scene components: `Name`, `Tags`,
`TransformComponent`, `Visibility`, `RenderLayers`, `RenderMesh`,
`SpriteBillboard`, cameras, and lights. It returns `SceneExportError` instead of
silently dropping unsupported material shaders or malformed hierarchies.

Selections can be promoted directly into prefab descriptors:

```rust
let prefab = scene_prefab_from_roots(&world, "crate_pair", selected_roots)?;
scene.prefabs.push(prefab);
save_scene_descriptor("assets/scenes/edited.oxscene", &scene)?;
```

The exported prefab stores normal `SceneEntityDescriptor` roots, so it validates,
spawns, and accepts prefab overrides through the same path as hand-authored
`.oxscene` prefabs.

## Scene Materials

Code-first scenes can register named material intents once and reference them
from many renderable entities:

```rust
let mut materials = SceneMaterialLibrary::new();
materials.register("enemy_unlit", RenderMaterial::default());
world.insert_resource(materials);

world.spawn((
    TransformComponent::default(),
    RenderMesh::new(
        MeshPrimitive::Cube,
        RenderMaterial::Named("enemy_unlit".to_string()),
    ),
));
```

When `DefaultPlugins` loads a `.oxmat` descriptor through
`request_material_descriptor_load`, the descriptor is also registered into
`SceneMaterialLibrary` under `MaterialDescriptor::name`. This gives small
scenes a data-driven material path without forcing gameplay components to hold
renderer pipelines. Entities that carry `MaterialFilter` can render directly
from the loaded descriptor handle, which keeps spawned/imported entities aligned
with descriptor reloads. Material descriptors can set `base_color`; the
automatic scene renderer multiplies that material color with each entity's
`color` tint. Lit descriptors can also set `metallic_factor`,
`roughness_factor`, and `emissive_color`; the automatic scene renderer carries
those factors into its lightweight lit shader.

Native scene descriptors can declare reusable material entries in the top-level
`materials` array. Spawn registers those entries into `SceneMaterialLibrary`
before entities and prefabs are expanded, so mesh material references in the
same `.oxscene` can resolve without Rust setup:

```json
{
  "materials": [
    {
      "name": "enemy_unlit",
      "shader": "unlit",
      "alpha_mode": "blend",
      "color": [1.0, 0.4, 0.35, 1.0],
      "metallic_factor": 0.0,
      "roughness_factor": 0.6,
      "emissive_color": [0.05, 0.0, 0.0],
      "albedo_texture": "#enemy_albedo"
    }
  ],
  "entities": []
}
```

Native scene descriptors can reference scene-declared, code-registered, or
loaded `.oxmat` materials through the material `ref` field:

```json
{
  "type": "mesh",
  "primitive": "cube",
  "material": {
    "ref": "enemy_unlit",
    "color": [1.0, 0.4, 0.35, 1.0]
  }
}
```

For top-level scene material declarations, `color` becomes the reusable
material base color, `alpha_mode` controls opaque, alpha-masked, or
alpha-blended scene geometry, `metallic_factor`/`roughness_factor` tune the
lit response, `emissive_color` adds unlit contribution, and
`albedo_texture`/`normal_texture`/`metallic_texture`/`roughness_texture` store
material texture labels or path-like references. The renderer multiplies the red
channels from metallic and roughness textures with their scalar factors and
derives tangent-space normal mapping from mesh UV derivatives. For mesh entities
and prefab overrides,
`color` remains the per-entity tint even when material shader data is resolved
from the library. Alpha-masked geometry uses depth writes with a fixed 0.5
cutoff, while alpha-blended scene geometry is drawn after opaque geometry and
sorted back-to-front by camera distance per material batch.
Before batching, the scene renderer skips renderables outside the active
camera's frustum when bounds are known. glTF mesh nodes receive computed
`RenderBounds` from imported vertex positions. Custom mesh-handle entities
should add `RenderBounds` when their asset pipeline does not provide an authored
extent; otherwise they remain visible and layer-filtered but conservatively
uncullable.
`SceneRenderer::stats()` reports the last prepared frame's camera view count,
visible/layer-matching renderable candidates, cull count, submitted instance
counts, and draw calls for overlays or editor diagnostics. Engine integration
records the same counters into `Diagnostics` using the `SCENE_*` diagnostic
labels exported by the prelude.
The same visibility, layer, and bounds metadata is used by gameplay picking via
`pick_scene` and `pick_scene_from_viewport`, so authored culling bounds also
make imported mesh-handle entities pickable.
Texture labels such as `#image_0` or `#crate_albedo` resolve against
`TextureImageAssets`; file paths such as `textures/crate.png` are loaded by the
native scene asset pipeline and tracked as scene dependencies. Loaded `.oxmat`
descriptors and import tooling can also publish those images before the scene
renderer prepares the frame.

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
