//! Interactive Scene - 3D demo with FPS camera controls

use std::sync::Arc;

use oxide_engine::prelude::*;

struct InteractiveScene {
    world: World,
    window: Window,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: CameraBuffer,
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
        world.insert_resource(WindowResource::new(window.size().width, window.size().height));

        // Spawn camera entity
        world.spawn((
            CameraComponent::new(),
            CameraController::new().with_speed(3.0).with_sensitivity(0.003),
        ));

        let renderer_res = world.resource::<RendererResource>();
        let device = &renderer_res.renderer.device;

        // Create shader and pipeline
        let shader = create_shader(device, LIT_SHADER, Some("Lit Shader"));
        let camera_buffer = CameraBuffer::new(device);
        let pipeline = create_lit_pipeline(device, &shader, renderer_res.renderer.format(), &camera_buffer.bind_group_layout);

        // Create meshes
        let cube_mesh = Mesh3D::new_cube(device);
        let sphere_mesh = Mesh3D::new_sphere(device, 16, 16);

        // Create depth texture
        let depth_texture = DepthTexture::new(device, window.size().width, window.size().height, Some("Depth Texture"));

        tracing::info!(
            "Interactive scene initialized: {}x{}",
            window.size().width,
            window.size().height
        );
        tracing::info!("Controls: WASD to move, Mouse to look, Space/Shift for up/down");

        Self {
            world,
            window: window.clone(),
            pipeline,
            camera_buffer,
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

        let mut query = self.world.query::<(&mut CameraController, &mut CameraComponent)>();
        
        for (mut controller, mut camera_comp) in query.iter_mut(&mut self.world) {
            let camera = &mut camera_comp.0;
            
            controller.yaw -= dx * controller.sensitivity;
            controller.pitch -= dy * controller.sensitivity;
            controller.pitch = controller.pitch.clamp(-std::f32::consts::FRAC_PI_2 + 0.01, std::f32::consts::FRAC_PI_2 - 0.01);

            let rotation = Quat::from_rotation_y(controller.yaw) * Quat::from_rotation_x(controller.pitch);
            let forward = rotation * Vec3::NEG_Z;
            let right = rotation * Vec3::X;

            let mut velocity = Vec3::ZERO;
            if w { velocity += forward; }
            if s { velocity -= forward; }
            if a { velocity -= right; }
            if d { velocity += right; }
            if space { velocity += Vec3::Y; }
            if shift { velocity -= Vec3::Y; }

            if velocity != Vec3::ZERO {
                camera.position += velocity.normalize() * controller.speed * 0.016;
            }

            controller.update_camera(camera);
        }
    }

    fn render(&mut self) {
        // Get window info first
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

        let surface_texture: wgpu::SurfaceTexture = match self
            .world
            .resource::<RendererResource>()
            .renderer
            .begin_frame()
        {
            Ok(t) => t,
            Err(_) => return,
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Update camera uniform
        let aspect_ratio = if height > 0 { width as f32 / height as f32 } else { 1.0 };
        
        {
            let mut query = self.world.query::<(&CameraComponent, &CameraController)>();
            
            if let Some((camera_comp, _)) = query.iter(&self.world).next() {
                let camera = &camera_comp.0;
                let mut uniform = CameraUniform::new();
                uniform.update(camera.view_projection_matrix(aspect_ratio), camera.position);
                self.camera_buffer.update(&queue, &uniform);
            }
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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

            // Draw cubes
            render_pass.set_vertex_buffer(0, self.cube_mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.cube_mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.cube_mesh.index_count, 0, 0..6);

            // Draw spheres
            render_pass.set_vertex_buffer(0, self.sphere_mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.sphere_mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.sphere_mesh.index_count, 0, 6..10);
        }

        queue.submit(vec![encoder.finish()]);
        surface_texture.present();
    }

    fn on_event(&mut self, event: EngineEvent) {
        match event {
            EngineEvent::Resized { width, height } => {
                if width > 0 && height > 0 {
                    tracing::info!("Window resized: {}x{}", width, height);
                    let mut window_res = self.world.resource_mut::<WindowResource>();
                    window_res.update(width, height);

                    let renderer_device = {
                        let mut renderer = self.world.resource_mut::<RendererResource>();
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
    run_app::<InteractiveScene>();
}