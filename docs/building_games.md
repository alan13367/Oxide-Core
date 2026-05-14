# Building Games With Oxide

Oxide is currently a code-first Rust engine. A game starts as a normal Rust
crate that depends on `oxide_engine`, implements `App`, registers plugins, and
adds gameplay systems.

`oxide_engine` is the ergonomic facade. The actual runtime domains live in
focused crates such as `oxide_scene`, `oxide_ui`, `oxide_camera`,
`oxide_light`, `oxide_audio`, and `oxide_physics`, then get re-exported through
the engine prelude for normal game code.

## Current Game Authoring Path

- Use `DefaultPlugins` for input, transforms, renderer resources, and asset
  setup.
- Use `SceneAuthoringPlugins` when you want the built-in scene renderer,
  camera-locked game UI, native UI text rendering, custom font registration,
  scene editor resource, runtime UI model, and debug overlay.
- Use `SceneDescriptor` for small data-driven scenes and prefabs.
- Use `RenderMesh` to describe render intent without storing GPU buffers in
  gameplay components.
- Use `SpriteAssets` and `SpriteBillboard` for native custom sprites such as
  2D enemies, pickups, muzzle flashes, first-person weapons, and overlay props.
- Use `Terrain`, `TerrainDescriptor`, and `SceneWorldDescriptor` for
  configurable heightfield terrain plus reusable world blockout data.
- Use `SceneRendererPlugin` directly if you only want automatic rendering.
  The app runner prepares and queues it automatically when a `SceneRenderer`
  resource exists.
- Use `Events<T>` for gameplay messages and `Timer` for component/resource
  timers.
- Use `AudioPlugin` for sound playback. It inserts an `Audio` resource from
  `oxide_audio`, which can play generated tones or `AudioClip` WAV assets.
- Use `RuntimeUiPlugin` for HUD/menu data and `DevOverlayPlugin` for debug
  overlay state.
- Use `oxide_physics` with the `engine-plugin` feature for rigid bodies,
  colliders, queries, and collision events.

See `examples/minimal_game` for the smallest end-to-end template. It spawns a
starter scene and does not own a render pipeline, camera buffer, light buffer,
depth texture, or primitive GPU mesh itself. See `examples/zombie_shooter` for
a larger code-first gameplay slice with native zombie/weapon sprites,
descriptor-authored terrain/world geometry, jumping, hitscan shooting, zombie
AI, start/pause/game-over screens, health/ammo HUD widgets, native text labels,
menu cursor release/capture, audio feedback, and wave spawning.

```rust
app::<MyGame>()
    .add_plugins(DefaultPlugins)
    .add_plugins(SceneAuthoringPlugins)
    .add_plugin(AudioPlugin)
    .add_system(AppStage::PreUpdate, camera_controller_system)
    .run();
```

```rust
if let Some(audio) = world
    .contains_resource::<Audio>()
    .then(|| world.resource::<Audio>())
{
    audio.play_tone(AudioTone::sine(660.0, 0.08, 0.18));
}
```

Native sprites are registered once and referenced by stable IDs from gameplay
components:

```rust
register_sprite(&mut world, "zombie.walker", zombie_sprite_image()?);

world.spawn((
    Name("Zombie".to_string()),
    TransformComponent::from_position(Vec3::new(0.0, 0.9, -8.0)),
    GlobalTransform::default(),
    SpriteBillboard::new("zombie.walker", Vec2::new(1.25, 1.85))
        .with_facing(SpriteFacing::YBillboard),
));
```

Worlds can be authored as data and then given physics colliders by the game:

```rust
let spawned = spawn_world_descriptor(&mut world, &my_world_descriptor);
if let Some(terrain) = spawned.terrain {
    world.entity_mut(terrain).insert((
        RigidBodyComponent::static_body(),
        ColliderComponent::cuboid(Vec3::new(32.0, 0.1, 32.0)),
    ));
}
```

## Editor/UI Direction

The editor is implemented as normal engine data, not a separate tool runtime:

- `SceneEditorPlugin` inserts a `SceneEditor` resource.
- `SceneEditor` can select, spawn, duplicate, delete, rename, transform, and
  retint scene entities.
- `GameUiPlugin` inserts a `GameUi` resource and renders panels, buttons, bars,
  counters, and reticles as camera-locked scene primitives.
- `GameTextRenderer` is installed with `GameUiPlugin`; it batches styled text
  widgets into a native overlay pass with a dynamic glyph atlas.
- `GameFonts` stores the built-in bitmap font plus custom TrueType/OpenType
  fonts registered with `load_game_font` or `register_game_font_bytes`.
- `AudioPlugin` installs `oxide_audio::Audio` for generated tones, WAV clips,
  volume control, repeated playback, and stopping active sound instances.
- `show_scene_editor_egui(world, ctx)` draws the hierarchy and inspector.
- `show_scene_authoring_egui(world, ctx)` draws the scene editor plus runtime
  UI and debug overlay models.

The reusable egui data models exist today. A project that owns an egui render
pass can display them immediately through the helpers above.

## Near-Term Missing Pieces

- Native Oxide mesh/material/scene asset formats.
- A reusable egui-wgpu render pass owned by Oxide instead of app-level wiring.
- Viewport picking/gizmos for the scene editor.
- Richer font shaping/localization beyond basic glyph rasterization.
- Streaming/compressed audio importers beyond the current generated tone and
  WAV clip support.
- A command-line project scaffold once the template stabilizes.
