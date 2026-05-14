//! Automatic renderer for ECS scene primitives.

use std::collections::{BTreeMap, HashMap};

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use oxide_camera::{CameraBuffer, CameraComponent, CameraUniform};
use oxide_ecs::entity::Entity;
use oxide_ecs::world::World;
use oxide_light::LightBuffer;
use oxide_renderer::depth::DepthTexture;
use oxide_renderer::descriptor::MaterialType;
use oxide_renderer::mesh::{Mesh3D, Vertex3D};
use oxide_renderer::pipeline::create_shader;
use oxide_renderer::shader::BuiltinShader;
use oxide_renderer::wgpu;
use oxide_transform::{GlobalTransform, TransformComponent};

use crate::{MeshPrimitive, RenderMaterial, RenderMesh};

const SCENE_RENDERER_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    position: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct DirectionalLight {
    direction: vec4<f32>,
    color_intensity: vec4<f32>,
};

struct LightUniform {
    ambient_color_intensity: vec4<f32>,
    directional_count: u32,
    point_count: u32,
    _padding: vec2<u32>,
    directional_lights: array<DirectionalLight, 4>,
};

struct PointLight {
    position_radius: vec4<f32>,
    color_intensity: vec4<f32>,
    _padding: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> lights: LightUniform;

@group(1) @binding(1)
var<storage, read> point_lights: array<PointLight>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(6) model_3: vec4<f32>,
    @location(7) tint: vec4<f32>,
    @location(8) material: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) material_mode: f32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let model = mat4x4<f32>(input.model_0, input.model_1, input.model_2, input.model_3);
    let world_position = model * vec4<f32>(input.position, 1.0);

    var output: VertexOutput;
    output.clip_position = camera.view_proj * world_position;
    output.world_position = world_position.xyz;
    output.normal = normalize((model * vec4<f32>(input.normal, 0.0)).xyz);
    output.tint = input.tint;
    output.material_mode = input.material.x;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (input.material_mode < 0.5) {
        return input.tint;
    }

    let normal = normalize(input.normal);
    var lighting = lights.ambient_color_intensity.rgb * lights.ambient_color_intensity.a;

    for (var i: u32 = 0u; i < 4u; i = i + 1u) {
        if (i >= lights.directional_count) {
            break;
        }

        let light = lights.directional_lights[i];
        let light_dir = normalize(-light.direction.xyz);
        let diffuse = max(dot(normal, light_dir), 0.0);
        lighting = lighting + light.color_intensity.rgb * light.color_intensity.a * diffuse;
    }

    for (var i: u32 = 0u; i < lights.point_count; i = i + 1u) {
        let light = point_lights[i];
        let to_light = light.position_radius.xyz - input.world_position;
        let distance = length(to_light);
        let radius = max(light.position_radius.w, 0.001);
        if (distance < radius) {
            let light_dir = to_light / max(distance, 0.001);
            let diffuse = max(dot(normal, light_dir), 0.0);
            let attenuation = max(1.0 - distance / radius, 0.0);
            lighting = lighting + light.color_intensity.rgb
                * light.color_intensity.a
                * diffuse
                * attenuation
                * attenuation;
        }
    }

    let color = input.tint.rgb * max(lighting, vec3<f32>(0.08));
    return vec4<f32>(color, input.tint.a);
}
"#;

const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    3 => Float32x4,
    4 => Float32x4,
    5 => Float32x4,
    6 => Float32x4,
    7 => Float32x4,
    8 => Float32x4
];

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SceneInstanceRaw {
    model: [[f32; 4]; 4],
    tint: [f32; 4],
    material: [f32; 4],
}

impl SceneInstanceRaw {
    fn new(model: Mat4, tint: [f32; 4], material: MaterialBatchKey) -> Self {
        Self {
            model: model.to_cols_array_2d(),
            tint,
            material: [material.mode.shader_value(), 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct MaterialBatchKey {
    mode: MaterialMode,
    identity: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MaterialMode {
    Unlit,
    Lit,
}

impl MaterialMode {
    fn shader_value(self) -> f32 {
        match self {
            Self::Unlit => 0.0,
            Self::Lit => 1.0,
        }
    }
}

impl MaterialBatchKey {
    fn from_material(material: &RenderMaterial) -> Self {
        match material {
            RenderMaterial::Builtin {
                shader,
                material_type,
                name,
            } => Self {
                mode: material_mode(*shader, *material_type),
                identity: format!("builtin:{shader:?}:{material_type:?}:{name}"),
            },
            RenderMaterial::Named(name) => Self {
                mode: MaterialMode::Lit,
                identity: format!("named:{name}"),
            },
        }
    }
}

fn material_mode(shader: BuiltinShader, material_type: MaterialType) -> MaterialMode {
    match (shader, material_type) {
        (BuiltinShader::Lit, _) | (_, MaterialType::Lit) => MaterialMode::Lit,
        _ => MaterialMode::Unlit,
    }
}

#[derive(Debug)]
struct InstanceBatch {
    buffer: wgpu::Buffer,
    count: u32,
}

#[derive(Debug)]
struct SphereInstanceBatch {
    segments: u32,
    rings: u32,
    instances: InstanceBatch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneRendererStats {
    pub cube_instances: u32,
    pub sphere_instances: u32,
    pub draw_calls: u32,
}

/// GPU renderer for the high-level `RenderMesh` scene component.
///
/// Install it with `SceneRendererPlugin`. Once present in the world, the app
/// runner prepares and queues it automatically before `App::queue`, so gameplay
/// examples can focus on components and systems instead of pipelines.
pub struct SceneRenderer {
    camera_buffer: CameraBuffer,
    light_buffer: LightBuffer,
    pipeline: wgpu::RenderPipeline,
    depth_texture: DepthTexture,
    cube_mesh: Mesh3D,
    sphere_meshes: HashMap<(u32, u32), Mesh3D>,
    cube_instances: Vec<InstanceBatch>,
    sphere_instances: Vec<SphereInstanceBatch>,
    clear_color: wgpu::Color,
    stats: SceneRendererStats,
}

impl SceneRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let camera_buffer = CameraBuffer::new(device);
        let light_buffer = LightBuffer::new(device);
        let shader = create_shader(device, SCENE_RENDERER_SHADER, Some("Scene Renderer Shader"));
        let pipeline = create_scene_pipeline(
            device,
            &shader,
            format,
            &camera_buffer.bind_group_layout,
            &light_buffer.bind_group_layout,
        );

        Self {
            camera_buffer,
            light_buffer,
            pipeline,
            depth_texture: DepthTexture::new(device, width, height, Some("Scene Renderer Depth")),
            cube_mesh: Mesh3D::new_cube(device),
            sphere_meshes: HashMap::new(),
            cube_instances: Vec::new(),
            sphere_instances: Vec::new(),
            clear_color: wgpu::Color {
                r: 0.07,
                g: 0.09,
                b: 0.12,
                a: 1.0,
            },
            stats: SceneRendererStats::default(),
        }
    }

    pub fn set_clear_color(&mut self, clear_color: wgpu::Color) {
        self.clear_color = clear_color;
    }

    pub fn stats(&self) -> SceneRendererStats {
        self.stats
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &mut World,
        aspect_ratio: f32,
    ) {
        self.update_camera(queue, world, aspect_ratio);
        self.light_buffer.update(device, queue, world);
        self.prepare_instances(device, world);
    }

    pub fn queue(&mut self, view: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Scene Renderer Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.clear_color),
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
        render_pass.set_bind_group(1, &self.light_buffer.bind_group, &[]);

        for instances in &self.cube_instances {
            draw_mesh_batch(&mut render_pass, &self.cube_mesh, instances);
        }

        for batch in &self.sphere_instances {
            if let Some(mesh) = self.sphere_meshes.get(&(batch.segments, batch.rings)) {
                draw_mesh_batch(&mut render_pass, mesh, &batch.instances);
            }
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.depth_texture.resize(device, width, height);
    }

    fn update_camera(&self, queue: &wgpu::Queue, world: &mut World, aspect_ratio: f32) {
        let camera = {
            let mut query = world.query::<&CameraComponent>();
            query.iter(world).next().copied()
        };

        let mut uniform = CameraUniform::new();
        if let Some(camera) = camera {
            uniform.update(
                camera.0.view_projection_matrix(aspect_ratio),
                camera.0.position,
            );
        }
        self.camera_buffer.update(queue, &uniform);
    }

    fn prepare_instances(&mut self, device: &wgpu::Device, world: &mut World) {
        let renderables = collect_renderables(world);
        let mut cube_instances = BTreeMap::<MaterialBatchKey, Vec<SceneInstanceRaw>>::new();
        let mut sphere_instances =
            BTreeMap::<(u32, u32, MaterialBatchKey), Vec<SceneInstanceRaw>>::new();

        for (entity, render_mesh) in renderables {
            let model = entity_model_matrix(world, entity);
            let material = MaterialBatchKey::from_material(&render_mesh.material);
            let instance = SceneInstanceRaw::new(model, render_mesh.tint, material.clone());
            match render_mesh.primitive {
                MeshPrimitive::Cube => {
                    cube_instances.entry(material).or_default().push(instance);
                }
                MeshPrimitive::Sphere { segments, rings } => {
                    let segments = segments.clamp(3, 128);
                    let rings = rings.clamp(2, 128);
                    sphere_instances
                        .entry((segments, rings, material))
                        .or_default()
                        .push(instance);
                }
            }
        }

        self.cube_instances = cube_instances
            .values()
            .filter_map(|instances| {
                create_instance_batch(device, "Scene Cube Instances", instances)
            })
            .collect();
        self.sphere_instances.clear();

        let cube_count = cube_instances
            .values()
            .map(|instances| instances.len() as u32)
            .sum();
        let mut sphere_count = 0u32;
        for ((segments, rings, _material), instances) in sphere_instances {
            self.sphere_meshes
                .entry((segments, rings))
                .or_insert_with(|| Mesh3D::new_sphere(device, segments, rings));

            sphere_count += instances.len() as u32;
            if let Some(batch) = create_instance_batch(device, "Scene Sphere Instances", &instances)
            {
                self.sphere_instances.push(SphereInstanceBatch {
                    segments,
                    rings,
                    instances: batch,
                });
            }
        }

        self.stats = SceneRendererStats {
            cube_instances: cube_count,
            sphere_instances: sphere_count,
            draw_calls: self.cube_instances.len() as u32 + self.sphere_instances.len() as u32,
        };
    }
}

fn create_scene_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    light_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Scene Renderer Pipeline Layout"),
        bind_group_layouts: &[camera_layout, light_layout],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Scene Renderer Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex3D::desc(), SceneInstanceRaw::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

impl SceneInstanceRaw {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SceneInstanceRaw>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        }
    }
}

fn create_instance_batch(
    device: &wgpu::Device,
    label: &str,
    instances: &[SceneInstanceRaw],
) -> Option<InstanceBatch> {
    if instances.is_empty() {
        return None;
    }

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: std::mem::size_of_val(instances) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(bytemuck::cast_slice(instances));
    buffer.unmap();

    Some(InstanceBatch {
        buffer,
        count: instances.len() as u32,
    })
}

fn collect_renderables(world: &mut World) -> Vec<(Entity, RenderMesh)> {
    let mut query = world.query::<(Entity, &RenderMesh)>();
    query
        .iter(world)
        .map(|(entity, render_mesh)| (entity, render_mesh.clone()))
        .collect()
}

fn entity_model_matrix(world: &World, entity: Entity) -> Mat4 {
    world
        .get::<GlobalTransform>(entity)
        .map(|global| global.matrix)
        .or_else(|| {
            world
                .get::<TransformComponent>(entity)
                .map(|local| local.to_matrix())
        })
        .unwrap_or(Mat4::IDENTITY)
}

fn draw_mesh_batch<'pass>(
    render_pass: &mut wgpu::RenderPass<'pass>,
    mesh: &'pass Mesh3D,
    instances: &'pass InstanceBatch,
) {
    render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
    render_pass.set_vertex_buffer(1, instances.buffer.slice(..));
    render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    render_pass.draw_indexed(0..mesh.index_count, 0, 0..instances.count);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_unlit_material_maps_to_unlit_instance_mode() {
        let material = RenderMaterial::Builtin {
            shader: BuiltinShader::Unlit,
            material_type: MaterialType::Unlit,
            name: "ui".to_string(),
        };

        let key = MaterialBatchKey::from_material(&material);

        assert_eq!(key.mode, MaterialMode::Unlit);
        assert_eq!(key.mode.shader_value(), 0.0);
    }

    #[test]
    fn builtin_lit_material_maps_to_lit_instance_mode() {
        let material = RenderMaterial::Builtin {
            shader: BuiltinShader::Lit,
            material_type: MaterialType::Lit,
            name: "scene_lit".to_string(),
        };

        let key = MaterialBatchKey::from_material(&material);

        assert_eq!(key.mode, MaterialMode::Lit);
        assert_eq!(key.mode.shader_value(), 1.0);
    }

    #[test]
    fn material_batch_key_preserves_named_identity() {
        let first = MaterialBatchKey::from_material(&RenderMaterial::Named("metal".to_string()));
        let second = MaterialBatchKey::from_material(&RenderMaterial::Named("glass".to_string()));

        assert_ne!(first, second);
    }
}
