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
- Plugins are unique by default. Override `Plugin::name` for stable diagnostics
  and `Plugin::is_unique` only when a plugin is intentionally repeatable.
- Use `AppStage::Startup` for one-shot setup systems that should use normal
  ECS params such as `Commands`, `ResMut<T>`, or `Res<Window>`.
- Use `ResMut<AppExit>` when a system needs to request a clean application
  shutdown after save, menu, or fatal-error handling.
- Use `AppStage::FixedUpdate` and `FixedTime` for deterministic gameplay
  systems that should advance at a stable tick rate independent of rendering.
- Use `TransformTween` for lightweight transform animation with one-shot,
  looping, or ping-pong playback. `DefaultPlugins` installs `AnimationPlugin`.
- Use `Visibility::Hidden` to hide a renderable entity or hierarchy subtree
  without despawning it. `InheritedVisibility` is propagated automatically.
- Use `RenderLayers` to filter meshes, terrain, and sprites per camera for
  first-person weapons, debug-only helpers, editor overlays, or alternate views.
- Use `CameraRenderView` on camera entities when a game or tool has multiple
  cameras. The scene renderer draws all active views in ascending `order`;
  inactive views are ignored, `viewport` draws into a normalized target
  rectangle, and the first view's `clear_color` overrides the scene renderer's
  default frame clear color.
- Use `ActionBindings<T>`, `ActionInput<T>`, and
  `sync_action_input_system::<T>` for semantic keyboard/mouse controls such as
  jump, fire, interact, or pause.
- Use `AxisBindings<T>`, `AxisInput<T>`, and `sync_axis_input_system::<T>` for
  semantic movement axes such as move X/Y, look X/Y, or throttle.
- Use `SceneAuthoringPlugins` when you want the built-in scene renderer,
  camera-locked game UI, native UI text rendering, custom font registration,
  scene editor resource, runtime UI model, and debug overlay.
- Use `SceneDescriptor` for small data-driven scenes, child hierarchies,
  registered sprite billboards, and reusable prefabs that can be instantiated
  from `.oxscene` data or Rust with per-instance child overrides. Use authored
  scene paths plus `scene_instance_id`, `entity_by_scene_path_in_instance`, and
  `entities_under_scene_path_in_instance` for stable child lookup across
  repeated scene copies. Use `despawn_scene_instance` to unload one copy. Use
  entity `tags` plus `entities_with_tag_in_instance`,
  `first_entity_with_tag_in_instance`, and `entity_has_tag` when gameplay
  systems need stable labels independent of display names.
- Use `reload_oxscene_path` or `reload_changed_oxscenes` when development tools
  should refresh native scene descriptors while preserving handles. Declare
  `.oxscene` `dependencies` when sidecar material, sprite, or import files
  should invalidate the scene during hot reload.
- Use `reload_changed_native_assets` or `poll_native_asset_reloads` when a
  watcher should route changed paths through Oxide's built-in `.oxscene` and
  `.oxmat` reload systems together.
- Use `RenderMesh` to describe render intent without storing GPU buffers in
  gameplay components.
- Use `SceneMaterialLibrary` with `RenderMaterial::Named` when many entities
  should share a reusable material intent. Loaded `.oxmat` descriptors are
  registered into the library by material name through `DefaultPlugins`,
  including descriptor `base_color`, and `.oxscene` files can declare reusable
  `materials` or use mesh material `"ref": "material_name"` entries to
  reference them.
- Use `SpriteAssets` and `SpriteBillboard` for native custom sprites such as
  2D enemies, pickups, muzzle flashes, first-person weapons, and overlay props.
- Use `"type": "sprite"` entities in `.oxscene` files when a scene should place
  registered sprites without hard-coding every billboard in Rust.
- Use `SpriteImage::from_png_bytes` when project sprites are authored as PNGs
  and registered into the engine at startup.
- Use `Terrain`, `TerrainDescriptor`, and `SceneWorldDescriptor` for
  configurable heightfield terrain plus reusable world blockout data.
- Use `SceneRendererPlugin` directly if you only want automatic rendering.
  The app runner prepares and queues it automatically when a `SceneRenderer`
  resource exists.
- Use ordered render passes when a plugin needs to draw an overlay,
  post-process effect, capture pass, or debug visualization around the built-in
  scene/text/app/egui queue points without owning `App::queue`.
- Use `Events<T>` plus `EventWriter<T>`, `EventReader<T>`, `EventCursor<T>`,
  or `EventDrain<T>` for gameplay messages, and `Timer` for component/resource timers.
- Use `World::change_tick()` with component/resource revision helpers when
  renderer, editor, AI, or save-game caches need cheap invalidation.
- Use `World::register_component_type::<T>()` and
  `World::register_resource_type::<T>()` when editor, scene, or tooling code
  needs stable type names without full reflection.
- Use `ComponentChanges<T>` when one system should own an added/changed cursor
  for a component type without a separate revision resource.
- Use `ResourceCursor<T>` when a system should react to a resource only after
  that resource changes.
- Use `RemovedComponents<T>` when a cache should remove entries for despawned
  entities or components removed from still-alive entities.
- Use `AudioPlugin` for sound playback. It inserts an `Audio` resource from
  `oxide_audio`, which can play generated tones, spatial clips, or `AudioClip`
  WAV assets.
- Use `RuntimeUiPlugin` for HUD/menu data and `DevOverlayPlugin` for debug
  overlay state.
- Use `Diagnostics` for lightweight runtime scalar metrics. `DefaultPlugins`
  records `DELTA_SECONDS`, `FRAME_TIME_MS`, and `FPS`; games and tools can
  record additional labels without adding a profiling dependency.
- Use `oxide_physics` with the `engine-plugin` feature for rigid bodies,
  colliders, queries, and collision events.

See `examples/minimal_game` for the smallest end-to-end template. It spawns a
starter scene and does not own a render pipeline, camera buffer, light buffer,
depth texture, or primitive GPU mesh itself. See `examples/zombie_shooter` for
a larger code-first gameplay slice with native zombie/weapon sprites,
descriptor-authored terrain/world geometry, jumping, hitscan shooting, zombie
AI, start/pause/game-over screens, health/ammo HUD widgets, native text labels,
menu cursor release/capture, audio feedback, and wave spawning.

## Render Pass Extension Points

Oxide keeps rendering explicit, but the app runner exposes stable render pass
anchors for plugin-owned frame callbacks:

- `RENDER_PASS_SCENE`
- `RENDER_PASS_GAME_TEXT`
- `RENDER_PASS_APP_QUEUE`
- `RENDER_PASS_EGUI`

Register a pass with `add_render_pass`, `add_render_pass_before`, or
`add_render_pass_after`. The callback receives the world and active
`RenderFrame`; create long-lived GPU resources during startup or
`AppStage::Prepare`, then encode only the frame work here.

```rust
fn queue_debug_overlay(world: &mut World, frame: &mut RenderFrame) {
    // Read prepared non-send GPU resources and encode draw commands.
}

app::<MyGame>()
    .add_plugins(DefaultPlugins)
    .add_plugins(SceneAuthoringPlugins)
    .add_render_pass_after("game.debug_overlay", RENDER_PASS_APP_QUEUE, queue_debug_overlay)
    .run();
```

Render pass sets mirror system sets for larger plugins:

```rust
app::<MyGame>()
    .add_render_pass_to_set("game.capture.depth", "game.capture", queue_depth_capture)
    .configure_render_pass_set_before("game.capture", RENDER_PASS_EGUI)
    .run();
```

Tooling can inspect the resolved frame pipeline with
`RenderPassSchedule::ordered_pass_infos`. Temporarily disable custom passes
with `disable_render_pass` and restore them with `enable_render_pass`; disabled
passes remain visible in `RenderPassSchedule::pass_infos` but are skipped by the
runner.

```rust
app::<MyGame>()
    .add_plugins(DefaultPlugins)
    .add_plugins(SceneAuthoringPlugins)
    .add_plugin(AudioPlugin)
    .add_system(AppStage::Startup, setup_level)
    .add_labeled_system_to_set(
        AppStage::PreUpdate,
        "game.input.actions",
        "game.input",
        sync_action_input_system::<GameAction>,
    )
    .add_system_after(AppStage::PreUpdate, "game.input", camera_controller_system)
    .add_system(AppStage::FixedUpdate, fixed_simulation_system)
    .run();
```

`AppStage::Startup` runs once after the app and window exist, after
window-aware engine startup hooks, and before the first update frame. It uses
normal system params:

```rust
fn setup_level(mut commands: Commands, window: Res<Window>) {
    commands.spawn((
        Name("Player".to_string()),
        TransformComponent::default(),
    ));

    let size = window.size();
    tracing::info!("window size: {}x{}", size.width, size.height);
}
```

`AppExit` is inserted by the runner before startup systems. Any startup or frame
system can request shutdown:

```rust
fn quit_from_menu(mut exit: ResMut<AppExit>, input: Res<ActionInput<MenuAction>>) {
    if input.just_pressed(MenuAction::Quit) {
        exit.request();
    }
}
```

`FixedUpdate` runs after `PreUpdate` and before normal `Update` work. Read
`FixedTime::timestep_secs()` inside fixed systems when integrating simulation
state.

`TransformTween` animates local `TransformComponent` data before transform
propagation:

```rust
commands.spawn((
    TransformComponent::from_position(Vec3::ZERO),
    TransformTween::ping_pong(
        Transform::from_position(Vec3::ZERO),
        Transform::from_position(Vec3::new(0.0, 1.0, 0.0)),
        Duration::from_secs(2),
    )
    .with_easing(TweenEasing::SmoothStep),
));
```

`Visibility` controls scene renderer collection for meshes, sprites, and
terrain. Visibility propagates through `Parent`/`Children`, so hiding a parent
hides its descendants:

```rust
commands.spawn((
    TransformComponent::default(),
    RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
    Visibility::Hidden,
));
```

`RenderLayers` controls which cameras see a renderable entity. Entities and
cameras default to layer `0`; a mesh, terrain, or sprite only renders in camera
views whose layer masks intersect its layer mask:

```rust
commands.spawn((
    CameraComponent::default(),
    CameraRenderView::new()
        .with_order(-1)
        .with_viewport(CameraViewport::new(0.75, 0.0, 0.25, 0.25))
        .with_clear_color([0.02, 0.03, 0.04, 1.0]),
    TransformComponent::default(),
    RenderLayers::layer(1),
));

commands.spawn((
    TransformComponent::default(),
    RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
    RenderLayers::layer(1),
));
```

Use `add_labeled_system`, `add_system_to_set`, `add_system_before`, and
`add_system_after` when plugins or gameplay systems need stable ordering inside
a stage. Before/after targets can reference either a system label or a set
name. Built-in labels include `OXSCENE_SPAWN_SYSTEM`, `GLTF_SCENE_SPAWN_SYSTEM`,
and `TRANSFORM_PROPAGATE_SYSTEM`.

Use `State<T>` for coarse game modes and `.run_if(...)` for mode-specific
systems. `in_state(...)` gates normal steady-state work, while
`state_entered(...)` and `state_exited(...)` fire once per system for each
transition revision:

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
enum GameMode {
    Menu,
    Playing,
}

world.insert_resource(State::new(GameMode::Menu));

app::<MyGame>()
    .add_system(AppStage::Update, menu_system.run_if(in_state(GameMode::Menu)))
    .add_system(AppStage::Update, spawn_level.run_if(state_entered(GameMode::Playing)))
    .add_system(AppStage::Update, stop_music.run_if(state_exited(GameMode::Menu)))
    .run();
```

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum GameAction {
    Fire,
}

world.insert_resource(ActionInput::<GameAction>::default());
let mut bindings = ActionBindings::default();
bindings
    .bind_key(GameAction::Fire, KeyCode::ControlLeft)
    .bind_mouse(GameAction::Fire, MouseButton::Left);
world.insert_resource(bindings);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum GameAxis {
    MoveX,
}

world.insert_resource(AxisInput::<GameAxis>::default());
let mut axes = AxisBindings::default();
axes.bind_key_pair(GameAxis::MoveX, KeyCode::KeyA, KeyCode::KeyD);
world.insert_resource(axes);
```

Scene descriptors can keep reusable object templates next to the level data:

```rust
let instance = spawn_scene_prefab_instance(
    &mut world,
    &scene_descriptor,
    "crate_pair",
    SceneTransform::from_position([2.0, 0.0, -1.5]),
).unwrap();
```

`SpawnedSceneInstance` includes the instance ID plus root entities. Prefab
roots are normal entities with `TransformComponent`, `GlobalTransform`, and
`Children`, so gameplay systems and the editor can move or inspect them like any
other hierarchy root. Use `instance.root()` for single-root prefab workflows and
`instance.id` with scoped scene queries or `despawn_scene_instance` when managing
the spawned copy later.

`.oxscene` prefab entities can provide `overrides` for named prefab children
when an instance needs a different child transform, visibility, render layer, or
mesh material:

```json
{
  "type": "prefab",
  "id": "crate_pair",
  "overrides": [
    {
      "path": "Crate Base/Crate Top",
      "transform": { "position": [0.0, 1.35, 0.0] },
      "material": { "ref": "crate_unlit", "color": [1.0, 0.45, 0.35, 1.0] }
    }
  ]
}
```

Frame metrics are available through the shared diagnostics resource:

```rust
fn update_debug_numbers(diagnostics: Res<Diagnostics>, mut ui: ResMut<RuntimeUi>) {
    if let Some(fps) = diagnostics.latest(FPS) {
        ui.label("fps", format!("FPS: {:.0}", fps));
    }
}

fn record_spawn_pressure(mut diagnostics: ResMut<Diagnostics>, mut query: Query<&Enemy>) {
    diagnostics.record("game.enemy_count", query.iter().count() as f64);
}
```

`SceneAuthoringPlugins` also uses these values for the egui debug overlay.

Editor and import tooling should call `SceneDescriptor::validate()` before
publishing authored data. Native `.oxscene` loads also validate automatically,
and `try_spawn_scene_descriptor` / `try_spawn_scene_prefab` return structured
diagnostics for missing prefab IDs, duplicate IDs, recursive prefab graphs, and
invalid prefab overrides without partially spawning invalid content.

Editor save flows can export live Oxide scene entities back into descriptors:

```rust
let scene = scene_descriptor_from_world(&mut world)?;
save_scene_descriptor("assets/scenes/edited.oxscene", &scene)?;
```

Use `scene_descriptor_from_roots(&world, roots)` when a tool should save a
selection or authored subtree instead of every root entity. The exporter
preserves hierarchy, transforms, names, tags, visibility, render layers, meshes,
sprites, cameras, and lights, and reports `SceneExportError` when a live entity
uses data that cannot be represented by the current `.oxscene` schema.

Selections can also become reusable prefabs:

```rust
let prefab = scene_prefab_from_roots(&world, "crate_pair", selected_roots)?;
scene_descriptor.prefabs.push(prefab);
```

The prefab can then be instantiated through normal `.oxscene` prefab entities or
`spawn_scene_prefab_instance`.

Authored tags are intended for game logic and tools, not just editor display:

```rust
let instance = scene_instance_id(&world, loaded_root).unwrap();

if let Some(chest_lid) =
    entity_by_scene_path_in_instance(&mut world, instance, "Treasure Chest/Lid")
{
    // Attach an animation or interaction component to a named scene child.
}

if let Some(spawn) = first_entity_with_tag_in_instance(&mut world, instance, "player_spawn") {
    // Read the spawn transform or attach a player-controlled entity here.
}

for enemy in entities_with_tag_in_instance(&mut world, instance, "enemy") {
    // Attach AI, health, or runtime state after loading the scene.
}

let removed_entities = despawn_scene_instance(&mut world, instance);
```

During development, native scene descriptors can be reloaded in place:

```rust
let reloads = poll_render_asset_reloads(&mut world);
if !reloads.is_empty() {
    tracing::info!("Reloading assets: {:?}", reloads.native.changed_paths);
}
```

Native reloads update `SceneDescriptorAssets` and `MaterialDescriptorAssets`;
they do not duplicate already spawned scene instances. Queue a returned scene
handle explicitly if the game wants to create a new instance from the refreshed
descriptor.

glTF scenes use handle-scoped replacement instead: entities spawned by
`request_gltf_scene_spawn(...)` are tagged with `GltfSceneInstance`, and
`poll_render_asset_reloads(...)`, `reload_gltf_scene_path(...)`, or
`reload_changed_gltf_scenes(...)` queues the same handle so the spawn system
removes the previous imported hierarchy before spawning the refreshed one.

```rust
if let Some(audio) = world.get_resource::<Audio>() {
    audio.play_tone(AudioTone::sine(660.0, 0.08, 0.18));
}
```

Spatial playback uses the audio listener position/right vector for distance
attenuation and stereo panning:

```rust
if let Some(audio) = world.get_resource::<Audio>() {
    audio.set_listener_position([player.x, player.y, player.z]);
    audio.set_listener_right([camera_right.x, camera_right.y, camera_right.z]);
    audio.play_spatial_tone(AudioTone::sine(220.0, 0.08, 0.2), [impact.x, impact.y, impact.z]);
    audio.play_spatial_clip(
        explosion_clip,
        [impact.x, impact.y, impact.z],
        PlaySoundSettings::default().with_volume(0.7),
    );
}
```

Inside systems, optional plugin resources can be declared directly:

```rust
fn optional_audio_feedback(audio: Option<Res<Audio>>) {
    if let Some(audio) = audio {
        audio.play_tone(AudioTone::sine(440.0, 0.05, 0.12));
    }
}
```

Use `Local<T>` for tiny persistent state owned by one system, such as debounce
flags, debug counters, or cached previous values:

```rust
fn count_frames(mut frames: Local<u64>, mut ui: ResMut<RuntimeUi>) {
    *frames += 1;
    ui.label("frames", format!("Frames: {}", *frames));
}
```

Use ECS revisions when a cache should only refresh changed world data:

```rust
let last_sync = world.change_tick();
let player = world.spawn(TransformComponent::default()).id();

let mut query = world.query::<(Entity, &TransformComponent)>();
for (entity, transform) in query.iter_added_since(&world, last_sync) {
    // create cached transform-dependent data
}

let mut query = world.query::<(Entity, &TransformComponent)>();
for (entity, transform) in query.iter_changed_since(&world, last_sync) {
    // update cached transform-dependent data
}

let mut changed = world.query_filtered::<(Entity, &TransformComponent), Changed<TransformComponent>>();
for (entity, transform) in changed.iter_since(&world, last_sync) {
    // filtered-query syntax for the same changed component data
}
```

```rust
fn sync_render_settings(mut settings: ResourceCursor<RenderSettings>) {
    if let Some(settings) = settings.read_if_changed() {
        // update settings-dependent cached data
    }
}
```

```rust
fn sync_transform_cache(mut transforms: ComponentChanges<TransformComponent>) {
    for (entity, transform) in transforms.added() {
        // create cached transform-dependent data
    }
    for (entity, transform) in transforms.read_changed() {
        // update cached transform-dependent data
    }
}
```

```rust
fn cleanup_render_cache(
    mut removed: RemovedComponents<RenderMesh>,
    mut cache: ResMut<MeshCache>,
) {
    for record in removed.read() {
        cache.remove(record.entity);
    }
}
```

After all systems that consume removals have advanced past a revision, use
`World::prune_removed_components_through::<T>(revision)` or
`World::prune_all_removed_components_through(revision)` to bound retained
cleanup history in long-running tools.

```rust
fn collect_pulses(mut pulses: EventDrain<GameEvent>, mut state: ResMut<GameState>) {
    for event in pulses.drain() {
        match event {
            GameEvent::Pulse => state.pulses += 1,
        }
    }
}
```

Use `EventCursor<T>` when multiple systems need non-consuming incremental reads
from the same buffer:

```rust
fn observe_pulses(mut pulses: EventCursor<GameEvent>, mut state: ResMut<GameState>) {
    for event in pulses.read() {
        match event {
            GameEvent::Pulse => state.observed_pulses += 1,
        }
    }
}
```

Deferred commands can reserve entity IDs immediately while keeping component
insertion and event emission deferred until the current stage completes:

```rust
fn spawn_pickup(mut commands: Commands) {
    let pickup = commands
        .spawn(Pickup)
        .insert(TransformComponent::from_position(Vec3::new(0.0, 1.0, -4.0)))
        .id();
    commands.send_event(GameEvent::PickupSpawned(pickup));
}
```

Use `EventWriter<T>` when the event should be visible immediately to later
systems in the same schedule run. Use `Commands::send_event` when event
emission belongs with other deferred world edits.

Import `HierarchyCommandsExt` from the engine prelude to queue hierarchy edits
through the same command buffer:

```rust
fn parent_pickup(mut commands: Commands, player: Res<PlayerEntity>) {
    let pickup = commands.spawn(Pickup).id();
    commands.attach_child(player.0, pickup);
}
```

Native sprites are registered once and referenced by stable IDs from gameplay
components or UI widgets:

```rust
register_sprite(
    &mut world,
    "zombie.walker",
    SpriteImage::from_png_bytes(include_bytes!("../assets/sprites/zombie.png"))?,
);

world.spawn((
    Name("Zombie".to_string()),
    TransformComponent::from_position(Vec3::new(0.0, 0.9, -8.0)),
    GlobalTransform::default(),
    SpriteBillboard::new("zombie.walker", Vec2::new(1.25, 1.85))
        .with_facing(SpriteFacing::YBillboard),
));

world.resource_mut::<GameUi>().sprite(
    "weapon",
    "player.shotgun",
    GameUiAnchor::BottomRight,
    [-0.16, 0.31],
    [0.32, 0.26],
    [1.0, 1.0, 1.0, 1.0],
);
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
- `scene_descriptor_from_world` and `scene_descriptor_from_roots` provide the
  save-side bridge from edited ECS data back into `.oxscene` descriptors.
- `scene_prefab_from_roots` lets editor tools promote selected entities into
  reusable `.oxscene` prefab descriptors.
- `GameUiPlugin` inserts a `GameUi` resource and renders panels, buttons, bars,
  counters, and reticles as camera-locked scene primitives.
- `GameTextRenderer` is installed with `GameUiPlugin`; it batches styled text
  widgets into a native overlay pass with a dynamic glyph atlas.
- `GameFonts` stores the built-in bitmap font plus custom TrueType/OpenType
  fonts registered with `load_game_font` or `register_game_font_bytes`.
- `AudioPlugin` installs `oxide_audio::Audio` for generated tones, WAV clips,
  lightweight spatial playback, volume control, repeated playback, and stopping
  active sound instances.
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
