use oxide_engine::prelude::*;
use oxide_physics::prelude::*;
use std::sync::Arc;

struct CpuDrivenMesh {
    base_vertices: Vec<Vertex3D>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl CpuDrivenMesh {
    fn new_cube(device: &wgpu::Device, label: &str) -> Self {
        let base_vertices = cube_vertices();
        let indices = cube_indices();

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label} Vertex Buffer")),
            size: std::mem::size_of_val(base_vertices.as_slice()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        vertex_buffer
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(base_vertices.as_slice()));
        vertex_buffer.unmap();

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label} Index Buffer")),
            size: std::mem::size_of_val(indices.as_slice()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::INDEX,
            mapped_at_creation: true,
        });
        index_buffer
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(indices.as_slice()));
        index_buffer.unmap();

        Self {
            base_vertices,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
        }
    }

    fn write_transform(&self, queue: &wgpu::Queue, translation: Vec3, rotation: Quat, scale: Vec3) {
        let mut transformed = self.base_vertices.clone();
        for vertex in &mut transformed {
            let local = Vec3::new(vertex.position[0], vertex.position[1], vertex.position[2]);
            let world = translation + rotation * (local * scale);
            vertex.position = [world.x, world.y, world.z];

            let normal = Vec3::new(vertex.normal[0], vertex.normal[1], vertex.normal[2]);
            let world_normal = (rotation * normal).normalize_or_zero();
            vertex.normal = [world_normal.x, world_normal.y, world_normal.z];
        }

        queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(transformed.as_slice()),
        );
    }
}

struct PhysicsApp {
    world: World,
    pipeline: wgpu::RenderPipeline,
    material_bind_group: Option<wgpu::BindGroup>,
    camera_buffer: CameraBuffer,
    _light_buffer: LightBuffer,
    dynamic_mesh: CpuDrivenMesh,
    ground_mesh: CpuDrivenMesh,
    depth_texture: DepthTexture,
    log_timer: f32,
}

impl App for PhysicsApp {
    fn configure(world: &mut World) {
        world.init_resource::<Time>();
        world.init_resource::<KeyboardInput>();
        world.init_resource::<MouseInput>();
    }

    fn init(window: &Window, renderer: Renderer) -> Self {
        let mut world = World::new();
        Self::configure(&mut world);

        world.insert_resource(Time::default());
        world.insert_resource(KeyboardInput::default());
        world.insert_resource(MouseInput::default());
        world.insert_resource(RendererResource::new(renderer));
        world.insert_resource(WindowResource::new(
            window.size().width,
            window.size().height,
        ));

        let mut camera = CameraComponent::new();
        camera.0.position = Vec3::new(0.0, 2.0, 8.0);
        camera.0.target = Vec3::new(0.0, 0.0, 0.0);
        world.spawn(camera);

        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, 5.0, 0.0)),
            RigidBodyComponent::dynamic(),
            ColliderComponent::cuboid(Vec3::splat(0.5)),
        ));

        world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, -2.0, 0.0)),
            RigidBodyComponent::static_body(),
            ColliderComponent::cuboid(Vec3::new(10.0, 0.5, 10.0)),
        ));

        let renderer_res = world.resource::<RendererResource>();
        let device = Arc::clone(&renderer_res.renderer.device);
        let queue = &renderer_res.renderer.queue;
        let format = renderer_res.renderer.format();
        let _ = renderer_res;

        let camera_buffer = CameraBuffer::new(&device);
        let light_buffer = LightBuffer::new(&device);
        let material = MaterialPipeline::from_builtin(
            &device,
            queue,
            format,
            &camera_buffer.bind_group_layout,
            &light_buffer.bind_group_layout,
            BuiltinShader::Unlit,
            MaterialType::Unlit,
            "physics_example_material",
        );

        let dynamic_mesh = CpuDrivenMesh::new_cube(&device, "Dynamic Cube");
        let ground_mesh = CpuDrivenMesh::new_cube(&device, "Ground Cube");

        dynamic_mesh.write_transform(queue, Vec3::new(0.0, 5.0, 0.0), Quat::IDENTITY, Vec3::ONE);
        ground_mesh.write_transform(
            queue,
            Vec3::new(0.0, -2.0, 0.0),
            Quat::IDENTITY,
            Vec3::new(20.0, 1.0, 20.0),
        );

        let depth_texture = DepthTexture::new(
            &device,
            window.size().width,
            window.size().height,
            Some("Depth Texture"),
        );

        tracing::info!("Physics example initialized.");
        tracing::info!("You should now see a falling cube landing on a large ground block.");
        tracing::info!("Watch terminal logs for dynamic body Y position.");

        Self {
            world,
            pipeline: material.pipeline,
            material_bind_group: material.bind_group,
            camera_buffer,
            _light_buffer: light_buffer,
            dynamic_mesh,
            ground_mesh,
            depth_texture,
            log_timer: 0.0,
        }
    }

    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    fn update(&mut self) {
        self.log_timer += self.world.resource::<Time>().delta_secs();
        if self.log_timer < 0.2 {
            return;
        }
        self.log_timer = 0.0;

        let mut query = self
            .world
            .query::<(&TransformComponent, &RigidBodyComponent)>();
        if let Some((transform, _)) = query
            .iter(&self.world)
            .find(|(_, body)| body.body_type == RigidBodyType::Dynamic)
        {
            tracing::info!("Dynamic body Y: {:.3}", transform.transform.position.y);
        }
    }

    fn prepare(&mut self) {
        let (width, height) = {
            let res = self.world.resource::<WindowResource>();
            (res.width, res.height)
        };
        let queue = {
            let renderer = self.world.resource::<RendererResource>();
            Arc::clone(&renderer.renderer.queue)
        };

        let aspect_ratio = if height > 0 {
            width as f32 / height as f32
        } else {
            1.0
        };

        let camera = {
            let mut query = self.world.query::<&CameraComponent>();
            query.iter(&self.world).next().copied()
        };

        if let Some(camera) = camera {
            let mut uniform = CameraUniform::new();
            uniform.update(
                camera.0.view_projection_matrix(aspect_ratio),
                camera.0.position,
            );
            self.camera_buffer.update(&queue, &uniform);
        }

        let dynamic_transform = {
            let mut query = self
                .world
                .query::<(&TransformComponent, &RigidBodyComponent)>();
            query
                .iter(&self.world)
                .find(|(_, body)| body.body_type == RigidBodyType::Dynamic)
                .map(|(transform, _)| transform.transform)
        };

        if let Some(transform) = dynamic_transform {
            self.dynamic_mesh.write_transform(
                &queue,
                transform.position,
                transform.rotation,
                transform.scale,
            );
        }
    }

    fn queue(&mut self, frame: &mut RenderFrame) {
        let mut render_pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Physics Example Main Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Store,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.camera_buffer.bind_group, &[]);
        if let Some(ref bind_group) = self.material_bind_group {
            render_pass.set_bind_group(1, bind_group, &[]);
        }

        render_pass.set_vertex_buffer(0, self.ground_mesh.vertex_buffer.slice(..));
        render_pass.set_index_buffer(
            self.ground_mesh.index_buffer.slice(..),
            wgpu::IndexFormat::Uint16,
        );
        render_pass.draw_indexed(0..self.ground_mesh.index_count, 0, 0..1);

        render_pass.set_vertex_buffer(0, self.dynamic_mesh.vertex_buffer.slice(..));
        render_pass.set_index_buffer(
            self.dynamic_mesh.index_buffer.slice(..),
            wgpu::IndexFormat::Uint16,
        );
        render_pass.draw_indexed(0..self.dynamic_mesh.index_count, 0, 0..1);
    }

    fn on_event(&mut self, event: EngineEvent) {
        if let EngineEvent::Resized { width, height } = event {
            if width > 0 && height > 0 {
                self.world
                    .resource_mut::<WindowResource>()
                    .update(width, height);
                let renderer_device = {
                    let renderer = self.world.resource_mut::<RendererResource>();
                    renderer.renderer.resize(width, height);
                    Arc::clone(&renderer.renderer.device)
                };
                self.depth_texture.resize(&renderer_device, width, height);
            }
        }
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    app::<PhysicsApp>()
        .add_plugins(DefaultPlugins)
        .add_plugin(PhysicsPlugin)
        .run();
}
