//! Interactive Scene - 3D demo with FPS camera controls and dynamic lighting

use std::path::Path;
use std::sync::Arc;

use oxide_engine::prelude::*;

struct InteractiveScene {
    world: World,
    pipeline: wgpu::RenderPipeline,
    material_bind_group: Option<wgpu::BindGroup>,
    camera_buffer: CameraBuffer,
    light_buffer: LightBuffer,
    cube_mesh: Mesh3D,
    sphere_mesh: Mesh3D,
    depth_texture: DepthTexture,
}

impl App for InteractiveScene {
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

        // Spawn camera entity
        world.spawn((
            CameraComponent::new(),
            CameraController::new()
                .with_speed(3.0)
                .with_sensitivity(0.003),
        ));

        // Spawn ambient light (global illumination)
        world.spawn(AmbientLight::new(Vec3::new(0.4, 0.4, 0.5), 0.25));

        // Spawn directional lights (like sun/moon)
        world.spawn(DirectionalLight::new(
            Vec3::new(0.9, 1.2, 0.8),
            Vec3::new(1.0, 0.95, 0.9),
            0.65,
        ));
        world.spawn(DirectionalLight::new(
            Vec3::new(-0.6, 0.4, -1.0),
            Vec3::new(0.4, 0.5, 0.7),
            0.35,
        ));

        // Spawn a point light (like a torch)
        world.spawn(PointLight::new(
            Vec3::new(0.0, 2.0, -1.0),
            Vec3::new(1.0, 0.8, 0.5),
            1.0,
            8.0,
        ));

        // Setup Hot Reload Watcher for dev profile
        #[cfg(debug_assertions)]
        if let Ok(watcher) = AssetWatcher::new("examples/hello_window/assets") {
            tracing::info!("Asset watcher initialized for hot-reloading");
            world.insert_non_send_resource(watcher);
        }

        let renderer_res = world.resource::<RendererResource>();
        let device = &renderer_res.renderer.device;

        // Create camera buffer and light buffer
        let camera_buffer = CameraBuffer::new(device);
        let light_buffer = LightBuffer::new(device);

        let queue = &renderer_res.renderer.queue;
        let descriptor_path = Path::new("examples/hello_window/assets/materials/scene_lit.json");

        let material = match load_material_descriptor(descriptor_path) {
            Ok(descriptor) => match MaterialPipeline::from_descriptor(
                device,
                queue,
                renderer_res.renderer.format(),
                &camera_buffer.bind_group_layout,
                &light_buffer.bind_group_layout,
                &descriptor,
            ) {
                Ok(material) => material,
                Err(err) => {
                    tracing::warn!(
                        "Failed to build descriptor material '{}', using built-in fallback: {}",
                        descriptor.name,
                        err
                    );
                    MaterialPipeline::from_builtin(
                        device,
                        queue,
                        renderer_res.renderer.format(),
                        &camera_buffer.bind_group_layout,
                        &light_buffer.bind_group_layout,
                        BuiltinShader::Fallback,
                        MaterialType::Lit,
                        "hello_window_fallback",
                    )
                }
            },
            Err(err) => {
                tracing::warn!(
                    "Failed to load material descriptor '{}', using built-in fallback: {}",
                    descriptor_path.display(),
                    err
                );

                MaterialPipeline::from_builtin(
                    device,
                    queue,
                    renderer_res.renderer.format(),
                    &camera_buffer.bind_group_layout,
                    &light_buffer.bind_group_layout,
                    BuiltinShader::Fallback,
                    MaterialType::Lit,
                    "hello_window_fallback",
                )
            }
        };
        let pipeline = material.pipeline;
        let material_bind_group = material.bind_group;

        // Create meshes
        let cube_mesh = Mesh3D::new_cube(device);
        let sphere_mesh = Mesh3D::new_sphere(device, 16, 16);

        // Create depth texture
        let depth_texture = DepthTexture::new(
            device,
            window.size().width,
            window.size().height,
            Some("Depth Texture"),
        );

        tracing::info!(
            "Interactive scene initialized: {}x{}",
            window.size().width,
            window.size().height
        );
        tracing::info!("Controls: WASD to move, Mouse to look, Space/Shift for up/down");

        Self {
            world,
            pipeline,
            material_bind_group,
            camera_buffer,
            light_buffer,
            cube_mesh,
            sphere_mesh,
            depth_texture,
        }
    }

    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    fn update(&mut self) {
        #[cfg(debug_assertions)]
        if let Some(watcher) = self.world.get_non_send_resource_mut::<AssetWatcher>() {
            let changed = watcher.poll_changed_files();
            if !changed.is_empty() {
                tracing::info!("Assets changed, attempting hot reload: {:?}", changed);
                // Simple implementation: reload the main material pipeline
                let descriptor_path =
                    Path::new("examples/hello_window/assets/materials/scene_lit.json");
                let renderer_res = self.world.resource::<RendererResource>();
                let device = &renderer_res.renderer.device;
                let queue = &renderer_res.renderer.queue;

                if let Ok(descriptor) = load_material_descriptor(descriptor_path) {
                    if let Ok(material) = MaterialPipeline::from_descriptor(
                        device,
                        queue,
                        renderer_res.renderer.format(),
                        &self.camera_buffer.bind_group_layout,
                        &self.light_buffer.bind_group_layout,
                        &descriptor,
                    ) {
                        tracing::info!("Successfully hot-reloaded material '{}'", descriptor.name);
                        self.pipeline = material.pipeline;
                        self.material_bind_group = material.bind_group;
                    }
                }
            }
        }

        // Store key states to avoid borrow checker issues
        let (w, s, a, d, space, shift) = {
            let keyboard = self.world.resource::<KeyboardInput>();
            (
                keyboard.pressed(KeyCode::KeyW),
                keyboard.pressed(KeyCode::KeyS),
                keyboard.pressed(KeyCode::KeyA),
                keyboard.pressed(KeyCode::KeyD),
                keyboard.pressed(KeyCode::Space),
                keyboard.pressed(KeyCode::ShiftLeft),
            )
        };

        let (dx, dy) = {
            let mouse = self.world.resource::<MouseInput>();
            mouse.delta()
        };

        let mut query = self
            .world
            .query::<(&mut CameraController, &mut CameraComponent)>();

        for (controller, camera_comp) in query.iter_mut(&mut self.world) {
            let camera = &mut camera_comp.0;

            controller.yaw -= dx * controller.sensitivity;
            controller.pitch -= dy * controller.sensitivity;
            controller.pitch = controller.pitch.clamp(
                -std::f32::consts::FRAC_PI_2 + 0.01,
                std::f32::consts::FRAC_PI_2 - 0.01,
            );

            let rotation =
                Quat::from_rotation_y(controller.yaw) * Quat::from_rotation_x(controller.pitch);
            let forward = rotation * Vec3::NEG_Z;
            let right = rotation * Vec3::X;

            let mut velocity = Vec3::ZERO;
            if w {
                velocity += forward;
            }
            if s {
                velocity -= forward;
            }
            if a {
                velocity -= right;
            }
            if d {
                velocity += right;
            }
            if space {
                velocity += Vec3::Y;
            }
            if shift {
                velocity -= Vec3::Y;
            }

            if velocity != Vec3::ZERO {
                camera.position += velocity.normalize() * controller.speed * 0.016;
            }

            controller.update_camera(camera);
        }
    }

    fn prepare(&mut self) {
        let (width, height) = {
            let window_res = self.world.resource::<WindowResource>();
            (window_res.width, window_res.height)
        };

        let (device, queue) = {
            let renderer = self.world.resource::<RendererResource>();
            (
                Arc::clone(&renderer.renderer.device),
                Arc::clone(&renderer.renderer.queue),
            )
        };

        // Update camera uniform
        let aspect_ratio = if height > 0 {
            width as f32 / height as f32
        } else {
            1.0
        };

        {
            let mut query = self.world.query::<(&CameraComponent, &CameraController)>();

            if let Some((camera_comp, _)) = query.iter(&self.world).next() {
                let camera = &camera_comp.0;
                let mut uniform = CameraUniform::new();
                uniform.update(camera.view_projection_matrix(aspect_ratio), camera.position);
                self.camera_buffer.update(&queue, &uniform);
            }
        }

        self.light_buffer.update(&device, &queue, &mut self.world);
    }

    fn queue(&mut self, frame: &mut RenderFrame) {
        {
            let mut render_pass = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Main Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame.view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.07,
                                g: 0.09,
                                b: 0.14,
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
            render_pass.set_bind_group(2, &self.light_buffer.bind_group, &[]);

            // Draw cubes
            render_pass.set_vertex_buffer(0, self.cube_mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(
                self.cube_mesh.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );
            render_pass.draw_indexed(0..self.cube_mesh.index_count, 0, 0..6);

            // Draw spheres
            render_pass.set_vertex_buffer(0, self.sphere_mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(
                self.sphere_mesh.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );
            render_pass.draw_indexed(0..self.sphere_mesh.index_count, 0, 6..10);
        }
    }

    fn on_event(&mut self, event: EngineEvent) {
        match event {
            EngineEvent::Resized { width, height } => {
                if width > 0 && height > 0 {
                    tracing::info!("Window resized: {}x{}", width, height);
                    let window_res = self.world.resource_mut::<WindowResource>();
                    window_res.update(width, height);

                    let renderer_device = {
                        let renderer = self.world.resource_mut::<RendererResource>();
                        renderer.renderer.resize(width, height);
                        Arc::clone(&renderer.renderer.device)
                    };

                    self.depth_texture.resize(&renderer_device, width, height);
                }
            }
            EngineEvent::CloseRequested => {
                tracing::info!("Close requested");
            }
            _ => {}
        }
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting Oxide Core - Interactive Scene Demo");
    tracing::info!("Controls:");
    tracing::info!("  WASD - Move camera");
    tracing::info!("  Mouse - Look around");
    tracing::info!("  Space - Move up");
    tracing::info!("  Shift - Move down");
    app::<InteractiveScene>().add_plugins(DefaultPlugins).run();
}
