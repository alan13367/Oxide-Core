use oxide_engine::prelude::*;

struct MaterialFilterExample {
    world: World,
}

impl App for MaterialFilterExample {
    fn configure(world: &mut World) {
        world.init_resource::<Time>();
        world.init_resource::<KeyboardInput>();
        world.init_resource::<MouseInput>();
    }

    fn init(window: &Window, renderer: Renderer) -> Self {
        let mut world = World::new();
        Self::configure(&mut world);
        world.insert_resource(RendererResource::new(renderer));
        world.insert_resource(WindowResource::new(
            window.size().width,
            window.size().height,
        ));

        let material = Handle::<MaterialDescriptor>::new(1);
        let mut materials = MaterialDescriptorAssets::default();
        materials.assets.insert(
            material,
            MaterialDescriptor {
                name: "descriptor_blue".to_string(),
                material_type: MaterialType::Lit,
                shader: ShaderDescriptor::Builtin {
                    shader: "lit".to_string(),
                },
                fallback_shader: Some("lit".to_string()),
                base_color: [0.15, 0.45, 1.0, 1.0],
                albedo_texture: None,
                normal_texture: None,
                roughness_texture: None,
            },
        );
        world.insert_resource(materials);

        world.spawn(CameraComponent::new());
        world.spawn((
            TransformComponent::default(),
            RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
            MaterialFilter::new(material),
        ));

        Self { world }
    }

    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    fn update(&mut self) {}

    fn on_event(&mut self, _event: EngineEvent) {}
}

fn main() {
    tracing_subscriber::fmt::init();
    app::<MaterialFilterExample>()
        .add_plugins(DefaultPlugins)
        .add_plugin(SceneRendererPlugin)
        .run();
}
