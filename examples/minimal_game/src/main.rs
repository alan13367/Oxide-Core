use std::time::Duration;

use oxide_engine::prelude::*;
use oxide_physics::prelude::*;

#[derive(Clone, Debug)]
enum GameEvent {
    Pulse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GameAction {
    SpawnCube,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum GameAxis {
    MoveX,
}

#[derive(Default)]
struct PendingSceneRoots {
    entities: Vec<Entity>,
}

impl Resource for PendingSceneRoots {}

#[derive(Default)]
struct FixedTickCounter(u64);

impl Resource for FixedTickCounter {}

#[derive(Default)]
struct PulseCounter(u64);

impl Resource for PulseCounter {}

struct MinimalGame {
    world: World,
    pulse_timer: Timer,
    scene_handle: Handle<SceneDescriptor>,
    scene_loaded: bool,
    marker_animation: Option<Entity>,
}

impl App for MinimalGame {
    fn configure(world: &mut World) {
        world.init_resource::<Time>();
        world.init_resource::<KeyboardInput>();
        world.init_resource::<MouseInput>();
        world.init_resource::<Events<GameEvent>>();
        world.init_resource::<PendingSceneRoots>();
        world.insert_resource(FixedTickCounter::default());
        world.insert_resource(PulseCounter::default());
        world.insert_resource(ActionInput::<GameAction>::default());
        world.insert_resource(AxisInput::<GameAxis>::default());

        let mut action_bindings = ActionBindings::default();
        action_bindings.bind_key(GameAction::SpawnCube, KeyCode::Space);
        world.insert_resource(action_bindings);

        let mut axis_bindings = AxisBindings::default();
        axis_bindings.bind_key_pair(GameAxis::MoveX, KeyCode::KeyA, KeyCode::KeyD);
        world.insert_resource(axis_bindings);
    }

    fn init(window: &Window, renderer: Renderer) -> Self {
        let mut world = World::new();
        Self::configure(&mut world);

        world.insert_resource(RendererResource::new(renderer));
        world.insert_resource(WindowResource::new(
            window.size().width,
            window.size().height,
        ));
        if let Ok(marker) = SpriteImage::solid(16, 16, [80, 190, 255, 255]) {
            register_sprite(&mut world, "debug.marker", marker);
        }

        let scene_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/scenes/starter.oxscene");
        let scene_handle = request_oxscene_spawn(&mut world, scene_path);

        Self {
            world,
            pulse_timer: Timer::repeating(Duration::from_secs(1)),
            scene_handle,
            scene_loaded: false,
            marker_animation: None,
        }
    }

    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    fn update(&mut self) {
        if !self.scene_loaded {
            if let Some(roots) = take_spawned_oxscene_roots(&mut self.world, self.scene_handle) {
                let scene_instance = roots
                    .first()
                    .and_then(|root| scene_instance_id(&self.world, *root));
                if let Some(scene_instance) = scene_instance {
                    if let Some(crate_pair) = entity_by_scene_path_in_instance(
                        &mut self.world,
                        scene_instance,
                        "Crate Pair",
                    ) {
                        add_y_pulse(&mut self.world, crate_pair, 0.25, Duration::from_secs(2));
                    }
                    if let Some(marker) =
                        first_entity_with_tag_in_instance(&mut self.world, scene_instance, "marker")
                    {
                        add_blended_clip_y_pulse(
                            &mut self.world,
                            marker,
                            0.15,
                            Duration::from_millis(900),
                        );
                        self.marker_animation = Some(marker);
                    }
                }
                self.world
                    .resource_mut::<PendingSceneRoots>()
                    .entities
                    .extend(roots);
                self.scene_loaded = true;
            }
        }

        let delta = self.world.resource::<Time>().delta;
        if self.pulse_timer.tick(delta).just_finished() {
            self.world
                .resource_mut::<Events<GameEvent>>()
                .send(GameEvent::Pulse);
        }

        let spawn_requested = self
            .world
            .resource::<ActionInput<GameAction>>()
            .just_pressed(&GameAction::SpawnCube)
            || self
                .world
                .get_resource::<RuntimeUi>()
                .map(|ui| ui.clicked("spawn"))
                .unwrap_or(false);
        if spawn_requested {
            let _ = with_scene_editor(&mut self.world, |editor, world| editor.spawn_cube(world));
        }

        let mesh_count = {
            let mut query = self.world.query::<&RenderMesh>();
            query.iter(&self.world).count()
        };
        let pending_events = self.world.resource::<Events<GameEvent>>().len();
        let entity_count = self.world.entity_count();
        let fixed_ticks = self.world.resource::<FixedTickCounter>().0;
        let pulse_count = self.world.resource::<PulseCounter>().0;
        let move_x = self
            .world
            .resource::<AxisInput<GameAxis>>()
            .value(&GameAxis::MoveX);
        if let Some(marker) = self.marker_animation {
            if let Some(machine) = self.world.get_mut::<AnimationStateMachine>(marker) {
                let state = if move_x.abs() > 0.01 {
                    "strong"
                } else {
                    "subtle"
                };
                machine.set_state(state, Duration::from_millis(250));
            }
        }

        if let Some(ui) = self.world.get_resource_mut::<RuntimeUi>() {
            ui.clear();
            ui.label("title", "Oxide Minimal Game");
            ui.label(
                "scene",
                if self.scene_loaded {
                    "Scene: assets/scenes/starter.oxscene"
                } else {
                    "Scene: loading..."
                },
            );
            ui.label("entities", format!("Entities: {entity_count}"));
            ui.label("meshes", format!("Render meshes: {mesh_count}"));
            ui.label("events", format!("Queued events: {pending_events}"));
            ui.label("fixed", format!("Fixed ticks: {fixed_ticks}"));
            ui.label("pulses", format!("Pulses handled: {pulse_count}"));
            ui.label("axis", format!("Move axis: {move_x:.1}"));
            ui.label(
                "controls",
                "A/D axis, Space or button: spawn cube; tween + blended clip animation",
            );
            ui.separator();
            ui.button("spawn", "Spawn Cube");
        }
    }

    fn on_event(&mut self, _event: EngineEvent) {}
}

fn add_y_pulse(world: &mut World, entity: Entity, height: f32, duration: Duration) {
    if let Some(from) = world
        .get::<TransformComponent>(entity)
        .map(|transform| transform.transform)
    {
        let mut to = from;
        to.position.y += height;
        world.entity_mut(entity).insert(
            TransformTween::ping_pong(from, to, duration).with_easing(TweenEasing::SmoothStep),
        );
    }
}

fn add_blended_clip_y_pulse(world: &mut World, entity: Entity, height: f32, duration: Duration) {
    if !world.contains_resource::<TransformAnimationClipAssets>() {
        world.insert_resource(TransformAnimationClipAssets::default());
    }
    if let Some(from) = world
        .get::<TransformComponent>(entity)
        .map(|transform| transform.transform)
    {
        let mut to = from;
        to.position.y += height;
        let target = TransformAnimationTarget(entity.index() as u64);
        let subtle_handle = TransformAnimationClipHandle::new(10_000 + u64::from(entity.index()));
        let strong_handle = TransformAnimationClipHandle::new(20_000 + u64::from(entity.index()));
        let subtle_clip = TransformAnimationClip::new("marker_bob_subtle", duration).with_channel(
            TransformAnimationChannel::Translation {
                target,
                interpolation: TransformAnimationInterpolation::Linear,
                keyframes: vec![
                    Vec3Keyframe {
                        time: Duration::ZERO,
                        value: from.position,
                    },
                    Vec3Keyframe {
                        time: duration,
                        value: to.position,
                    },
                ],
            },
        );
        let mut strong = from;
        strong.position.y += height * 1.8;
        let strong_clip = TransformAnimationClip::new("marker_bob_strong", duration).with_channel(
            TransformAnimationChannel::Translation {
                target,
                interpolation: TransformAnimationInterpolation::Linear,
                keyframes: vec![
                    Vec3Keyframe {
                        time: Duration::ZERO,
                        value: from.position,
                    },
                    Vec3Keyframe {
                        time: duration,
                        value: strong.position,
                    },
                ],
            },
        );
        world
            .resource_mut::<TransformAnimationClipAssets>()
            .assets
            .insert(subtle_handle, subtle_clip);
        world
            .resource_mut::<TransformAnimationClipAssets>()
            .assets
            .insert(strong_handle, strong_clip);
        world.entity_mut(entity).insert(target).insert(
            AnimationStateMachine::new("subtle")
                .with_state(AnimationState::new(
                    "subtle",
                    vec![AnimationBlendLayer::new(subtle_handle, target, 1.0)
                        .with_repeat(TweenRepeat::PingPong)],
                ))
                .with_state(AnimationState::new(
                    "strong",
                    vec![
                        AnimationBlendLayer::new(subtle_handle, target, 0.35)
                            .with_repeat(TweenRepeat::PingPong),
                        AnimationBlendLayer::new(strong_handle, target, 0.65)
                            .with_repeat(TweenRepeat::PingPong),
                    ],
                )),
        );
    }
}

fn fixed_tick_system(mut ticks: ResMut<FixedTickCounter>) {
    ticks.0 += 1;
}

fn handle_pulse_events(mut events: EventDrain<GameEvent>, mut pulses: ResMut<PulseCounter>) {
    for event in events.drain() {
        match event {
            GameEvent::Pulse => pulses.0 += 1,
        }
    }
}

fn publish_scene_spawn_result(mut pending: ResMut<PendingSceneRoots>, mut commands: Commands) {
    if pending.entities.is_empty() {
        return;
    }

    commands.insert_resource(SceneSpawnResult {
        entities: std::mem::take(&mut pending.entities),
    });
}

fn main() {
    tracing_subscriber::fmt::init();
    app::<MinimalGame>()
        .add_plugins(DefaultPlugins)
        .add_plugins(SceneAuthoringPlugins)
        .add_labeled_system_to_set(
            AppStage::PreUpdate,
            "minimal.input.actions",
            "minimal.input",
            sync_action_input_system::<GameAction>,
        )
        .add_labeled_system_to_set(
            AppStage::PreUpdate,
            "minimal.input.axes",
            "minimal.input",
            sync_axis_input_system::<GameAxis>,
        )
        .add_system_after(
            AppStage::PreUpdate,
            "minimal.input",
            camera_controller_system,
        )
        .add_system(AppStage::Update, publish_scene_spawn_result)
        .add_system(AppStage::FixedUpdate, fixed_tick_system)
        .add_system(AppStage::Update, handle_pulse_events)
        .add_plugin(PhysicsPlugin)
        .run();
}
