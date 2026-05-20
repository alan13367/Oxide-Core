use std::time::Duration;

use oxide_engine::prelude::*;
use oxide_physics::prelude::*;

#[derive(Clone, Debug)]
enum GameEvent {
    Pulse,
}

struct MinimalGame {
    world: World,
    pulse_timer: Timer,
    scene_handle: Handle<SceneDescriptor>,
    scene_loaded: bool,
}

impl App for MinimalGame {
    fn configure(world: &mut World) {
        world.init_resource::<Time>();
        world.init_resource::<KeyboardInput>();
        world.init_resource::<MouseInput>();
        world.init_resource::<Events<GameEvent>>();
    }

    fn init(window: &Window, renderer: Renderer) -> Self {
        let mut world = World::new();
        Self::configure(&mut world);

        world.insert_resource(RendererResource::new(renderer));
        world.insert_resource(WindowResource::new(
            window.size().width,
            window.size().height,
        ));

        let scene_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/scenes/starter.oxscene");
        let scene_handle = request_oxscene_spawn(&mut world, scene_path);

        Self {
            world,
            pulse_timer: Timer::repeating(Duration::from_secs(1)),
            scene_handle,
            scene_loaded: false,
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
                self.world
                    .insert_resource(SceneSpawnResult { entities: roots });
                self.scene_loaded = true;
            }
        }

        let delta = self.world.resource::<Time>().delta;
        if self.pulse_timer.tick(delta).just_finished() {
            self.world
                .resource_mut::<Events<GameEvent>>()
                .send(GameEvent::Pulse);
        }

        let spawn_requested = self.world.contains_resource::<RuntimeUi>()
            && self.world.resource::<RuntimeUi>().clicked("spawn");
        if spawn_requested {
            let _ = with_scene_editor(&mut self.world, |editor, world| editor.spawn_cube(world));
        }

        let mesh_count = {
            let mut query = self.world.query::<&RenderMesh>();
            query.iter(&self.world).count()
        };
        let pending_events = self.world.resource::<Events<GameEvent>>().len();
        let entity_count = self.world.entity_count();

        if self.world.contains_resource::<RuntimeUi>() {
            let ui = self.world.resource_mut::<RuntimeUi>();
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
            ui.separator();
            ui.button("spawn", "Spawn Cube");
        }
    }

    fn on_event(&mut self, _event: EngineEvent) {}
}

fn main() {
    tracing_subscriber::fmt::init();
    app::<MinimalGame>()
        .add_plugins(DefaultPlugins)
        .add_plugins(SceneAuthoringPlugins)
        .add_system(AppStage::PreUpdate, camera_controller_system)
        .add_plugin(PhysicsPlugin)
        .run();
}
