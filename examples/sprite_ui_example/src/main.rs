use oxide_engine::prelude::*;
use std::sync::Arc;

struct SpriteUiApp {
    world: World,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: CameraBuffer,
    cube_mesh: Mesh3D,
    depth_texture: DepthTexture,
}

impl App for SpriteUiApp {
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

        world.spawn((
            CameraComponent::new(),
            CameraController::new()
                .with_speed(3.0)
                .with_sensitivity(0.003),
        ));

        let renderer_res = world.resource::<RendererResource>();
        let device = Arc::clone(&renderer_res.renderer.device);
        let format = renderer_res.renderer.format();
        let _ = renderer_res;
        let camera_buffer = CameraBuffer::new(&device);

        let material = MaterialPipeline::from_builtin(
            &device,
            format,
            &camera_buffer.bind_group_layout,
            BuiltinShader::SpriteUi,
            MaterialType::Basic,
            "sprite_ui_material",
        );

        let depth_texture = DepthTexture::new(
            &device,
            window.size().width,
            window.size().height,
            Some("Depth Texture"),
        );

        Self {
            world,
            pipeline: material.pipeline,
            camera_buffer,
            cube_mesh: Mesh3D::new_cube(&device),
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
        let mut query = self
            .world
            .query::<(&mut CameraComponent, &mut CameraController)>();

        if let Some((mut camera, controller)) = query.iter_mut(&mut self.world).next() {
            controller.update_camera(&mut camera.0);
        }
    }

    fn render(&mut self) {
        let (width, height) = {
            let res = self.world.resource::<WindowResource>();
            (res.width, res.height)
        };
        let (device, queue) = {
            let renderer = self.world.resource::<RendererResource>();
            (
                Arc::clone(&renderer.renderer.device),
                Arc::clone(&renderer.renderer.queue),
            )
        };
        let surface_texture = match self
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

        let aspect_ratio = if height > 0 {
            width as f32 / height as f32
        } else {
            1.0
        };
        let mut query = self.world.query::<&CameraComponent>();
        if let Some(camera) = query.iter(&self.world).next() {
            let mut uniform = CameraUniform::new();
            uniform.update(
                camera.0.view_projection_matrix(aspect_ratio),
                camera.0.position,
            );
            self.camera_buffer.update(&queue, &uniform);
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
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.camera_buffer.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.cube_mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(
                self.cube_mesh.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );
            render_pass.draw_indexed(0..self.cube_mesh.index_count, 0, 0..1);
        }

        queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
    }

    fn on_event(&mut self, event: EngineEvent) {
        if let EngineEvent::Resized { width, height } = event {
            if width > 0 && height > 0 {
                self.world
                    .resource_mut::<WindowResource>()
                    .update(width, height);
                let renderer_device = {
                    let mut renderer = self.world.resource_mut::<RendererResource>();
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
    run_app::<SpriteUiApp>();
}
