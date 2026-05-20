//! Automatic renderer for ECS scene primitives.

use std::collections::{BTreeMap, HashMap, HashSet};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec2, Vec3};
use oxide_camera::{
    CameraBuffer, CameraComponent, CameraRenderView, CameraUniform, CameraViewport,
};
use oxide_ecs::entity::Entity;
use oxide_ecs::world::World;
use oxide_light::LightBuffer;
use oxide_renderer::depth::DepthTexture;
use oxide_renderer::descriptor::{AlphaMode, MaterialType};
use oxide_renderer::mesh::{Mesh3D, Vertex, Vertex3D};
use oxide_renderer::pipeline::create_shader;
use oxide_renderer::shader::BuiltinShader;
use oxide_renderer::texture::{SamplerDescriptor, Texture};
use oxide_renderer::wgpu;
use oxide_transform::{is_visible, GlobalTransform, TransformComponent};

use crate::{
    MaterialDescriptorAssets, MaterialFilter, MeshCache, MeshFilter, MeshPrimitive, RenderLayers,
    RenderMaterial, RenderMesh, SceneGizmoLines, SceneMaterialLibrary, SpriteAssets,
    SpriteBillboard, SpriteDepthMode, SpriteFacing, SpriteId, Terrain, TextureImageAssets,
};

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

@group(2) @binding(0)
var albedo_texture: texture_2d<f32>;

@group(2) @binding(1)
var normal_texture: texture_2d<f32>;

@group(2) @binding(2)
var roughness_texture: texture_2d<f32>;

@group(2) @binding(3)
var material_sampler: sampler;

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
    @location(9) material_factors: vec4<f32>,
    @location(10) texture_flags: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) material_mode: f32,
    @location(4) uv: vec2<f32>,
    @location(6) alpha_mode: f32,
    @location(7) roughness_factor: f32,
    @location(8) metallic_factor: f32,
    @location(9) emissive_color: vec3<f32>,
    @location(10) texture_flags: vec3<f32>,
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
    output.uv = input.uv;
    output.alpha_mode = input.material.z;
    output.roughness_factor = input.material.w;
    output.metallic_factor = input.material_factors.x;
    output.emissive_color = input.material_factors.yzw;
    output.texture_flags = input.texture_flags.xyz;
    return output;
}

fn normal_from_map(base_normal: vec3<f32>, world_position: vec3<f32>, uv: vec2<f32>, sample: vec3<f32>) -> vec3<f32> {
    let mapped = sample * 2.0 - vec3<f32>(1.0);
    let dp1 = dpdx(world_position);
    let dp2 = dpdy(world_position);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);
    let tangent = normalize(dp1 * duv2.y - dp2 * duv1.y);
    let bitangent = normalize(-dp1 * duv2.x + dp2 * duv1.x);
    return normalize(tangent * mapped.x + bitangent * mapped.y + base_normal * mapped.z);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(albedo_texture, material_sampler, input.uv);
    let texture_color = mix(vec4<f32>(1.0, 1.0, 1.0, 1.0), sampled, input.texture_flags.x);
    let surface_color = input.tint * texture_color;
    if (input.alpha_mode > 0.5 && input.alpha_mode < 1.5 && surface_color.a < 0.5) {
        discard;
    }

    if (input.material_mode < 0.5) {
        return surface_color;
    }

    let base_normal = normalize(input.normal);
    let normal_sample = textureSample(normal_texture, material_sampler, input.uv).xyz;
    let normal = select(base_normal, normal_from_map(base_normal, input.world_position, input.uv, normal_sample), input.texture_flags.y > 0.5);
    let view_dir = normalize(camera.position.xyz - input.world_position);
    let roughness_sample = textureSample(roughness_texture, material_sampler, input.uv).r;
    let roughness = clamp(input.roughness_factor * mix(1.0, roughness_sample, input.texture_flags.z), 0.04, 1.0);
    let metallic = clamp(input.metallic_factor, 0.0, 1.0);
    var lighting = lights.ambient_color_intensity.rgb * lights.ambient_color_intensity.a;
    var specular_lighting = vec3<f32>(0.0);

    for (var i: u32 = 0u; i < 4u; i = i + 1u) {
        if (i >= lights.directional_count) {
            break;
        }

        let light = lights.directional_lights[i];
        let light_dir = normalize(-light.direction.xyz);
        let diffuse = max(dot(normal, light_dir), 0.0);
        lighting = lighting + light.color_intensity.rgb * light.color_intensity.a * diffuse;
        let half_dir = normalize(light_dir + view_dir);
        let specular_power = mix(64.0, 8.0, roughness);
        let specular = pow(max(dot(normal, half_dir), 0.0), specular_power) * (1.0 - roughness);
        specular_lighting = specular_lighting
            + light.color_intensity.rgb * light.color_intensity.a * specular;
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
            let half_dir = normalize(light_dir + view_dir);
            let specular_power = mix(64.0, 8.0, roughness);
            let specular = pow(max(dot(normal, half_dir), 0.0), specular_power) * (1.0 - roughness);
            specular_lighting = specular_lighting
                + light.color_intensity.rgb
                * light.color_intensity.a
                * specular
                * attenuation
                * attenuation;
        }
    }

    let diffuse_color = surface_color.rgb * (1.0 - metallic * 0.65);
    let specular_color = mix(vec3<f32>(0.04), surface_color.rgb, metallic);
    let color = diffuse_color * max(lighting, vec3<f32>(0.08))
        + specular_color * specular_lighting
        + input.emissive_color;
    return vec4<f32>(color, surface_color.a);
}
"#;

const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
    3 => Float32x4,
    4 => Float32x4,
    5 => Float32x4,
    6 => Float32x4,
    7 => Float32x4,
    8 => Float32x4,
    9 => Float32x4,
    10 => Float32x4
];

const SPRITE_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];
const SPRITE_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    2 => Float32x4,
    3 => Float32x4,
    4 => Float32x4,
    5 => Float32x4
];

const SPRITE_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    position: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var sprite_texture: texture_2d<f32>;

@group(1) @binding(1)
var sprite_sampler: sampler;

struct VertexInput {
    @location(0) local_position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) center: vec4<f32>,
    @location(3) right_size: vec4<f32>,
    @location(4) up_size: vec4<f32>,
    @location(5) tint: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    if (input.center.w > 0.5) {
        let clip_position = vec2<f32>(
            input.center.x + input.local_position.x * input.right_size.w,
            input.center.y + input.local_position.y * input.up_size.w
        );
        output.clip_position = vec4<f32>(clip_position, input.center.z, 1.0);
    } else {
        let world_position = input.center.xyz
            + input.right_size.xyz * input.local_position.x * input.right_size.w
            + input.up_size.xyz * input.local_position.y * input.up_size.w;
        output.clip_position = camera.view_proj * vec4<f32>(world_position, 1.0);
    }
    output.uv = input.uv;
    output.tint = input.tint;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(sprite_texture, sprite_sampler, input.uv) * input.tint;
    if (color.a <= 0.01) {
        discard;
    }
    return color;
}
"#;

const GIZMO_LINE_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    position: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = camera.view_proj * vec4<f32>(input.position, 1.0);
    output.color = vec4<f32>(input.color, 1.0);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SceneInstanceRaw {
    model: [[f32; 4]; 4],
    tint: [f32; 4],
    material: [f32; 4],
    material_factors: [f32; 4],
    texture_flags: [f32; 4],
}

impl SceneInstanceRaw {
    fn new(model: Mat4, tint: [f32; 4], material: MaterialBatchKey) -> Self {
        let has_texture = f32::from(material.albedo_texture.is_some());
        Self {
            model: model.to_cols_array_2d(),
            tint,
            material: [
                material.mode.shader_value(),
                has_texture,
                material.alpha_mode.shader_value(),
                f32::from_bits(material.roughness_factor),
            ],
            material_factors: [
                f32::from_bits(material.metallic_factor),
                f32::from_bits(material.emissive_color[0]),
                f32::from_bits(material.emissive_color[1]),
                f32::from_bits(material.emissive_color[2]),
            ],
            texture_flags: [
                f32::from(material.albedo_texture.is_some()),
                f32::from(material.normal_texture.is_some()),
                f32::from(material.roughness_texture.is_some()),
                0.0,
            ],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct MaterialBatchKey {
    mode: MaterialMode,
    alpha_mode: AlphaMode,
    metallic_factor: u32,
    roughness_factor: u32,
    emissive_color: [u32; 3],
    identity: String,
    albedo_texture: Option<String>,
    normal_texture: Option<String>,
    roughness_texture: Option<String>,
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

trait AlphaModeShaderExt {
    fn shader_value(self) -> f32;
    fn is_transparent_phase(&self) -> bool;
}

impl AlphaModeShaderExt for AlphaMode {
    fn shader_value(self) -> f32 {
        match self {
            AlphaMode::Opaque => 0.0,
            AlphaMode::Mask => 1.0,
            AlphaMode::Blend => 2.0,
        }
    }

    fn is_transparent_phase(&self) -> bool {
        matches!(self, AlphaMode::Blend)
    }
}

impl MaterialBatchKey {
    #[cfg(test)]
    fn from_material(material: &RenderMaterial) -> Self {
        Self::from_material_with_library(material, None)
    }

    fn from_material_with_library(
        material: &RenderMaterial,
        library: Option<&SceneMaterialLibrary>,
    ) -> Self {
        if let RenderMaterial::Named(name) = material {
            if let Some(resolved) = library.and_then(|library| library.get(name)) {
                return Self::from_material_with_library(resolved, None);
            }
        }

        match material {
            RenderMaterial::Builtin {
                shader,
                material_type,
                name,
                base_color,
                metallic_factor,
                roughness_factor,
                emissive_color,
                alpha_mode,
                albedo_texture,
                normal_texture,
                roughness_texture,
            } => Self {
                mode: material_mode(*shader, *material_type),
                alpha_mode: *alpha_mode,
                metallic_factor: material_factor_key(*metallic_factor),
                roughness_factor: material_factor_key(*roughness_factor),
                emissive_color: emissive_color.map(material_factor_key),
                identity: format!(
                    "builtin:{shader:?}:{material_type:?}:{name}:{base_color:?}:{metallic_factor:?}:{roughness_factor:?}:{emissive_color:?}:{alpha_mode:?}:{albedo_texture:?}:{normal_texture:?}:{roughness_texture:?}"
                ),
                albedo_texture: albedo_texture.clone(),
                normal_texture: normal_texture.clone(),
                roughness_texture: roughness_texture.clone(),
            },
            RenderMaterial::Named(name) => Self {
                mode: MaterialMode::Lit,
                alpha_mode: AlphaMode::Opaque,
                metallic_factor: material_factor_key(0.0),
                roughness_factor: material_factor_key(0.5),
                emissive_color: [0.0, 0.0, 0.0].map(material_factor_key),
                identity: format!("named:{name}"),
                albedo_texture: None,
                normal_texture: None,
                roughness_texture: None,
            },
        }
    }
}

fn material_factor_key(value: f32) -> u32 {
    if value.is_nan() {
        0.0f32.to_bits()
    } else {
        value.to_bits()
    }
}

fn material_mode(shader: BuiltinShader, material_type: MaterialType) -> MaterialMode {
    match (shader, material_type) {
        (BuiltinShader::Lit, _) | (_, MaterialType::Lit) => MaterialMode::Lit,
        _ => MaterialMode::Unlit,
    }
}

fn multiply_color(left: [f32; 4], right: [f32; 4]) -> [f32; 4] {
    [
        left[0] * right[0],
        left[1] * right[1],
        left[2] * right[2],
        left[3] * right[3],
    ]
}

#[derive(Debug)]
struct InstanceBatch {
    buffer: wgpu::Buffer,
    count: u32,
    alpha_mode: AlphaMode,
    sort_depth: f32,
    material_texture: Option<wgpu::BindGroup>,
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
    pub mesh_handle_instances: u32,
    pub terrain_instances: u32,
    pub sprite_instances: u32,
    pub draw_calls: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SpriteVertex {
    local_position: [f32; 2],
    uv: [f32; 2],
}

impl SpriteVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SPRITE_VERTEX_ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SpriteInstanceRaw {
    center: [f32; 4],
    right_size: [f32; 4],
    up_size: [f32; 4],
    tint: [f32; 4],
}

impl SpriteInstanceRaw {
    fn new(center: Vec3, right: Vec3, up: Vec3, size: Vec2, tint: [f32; 4]) -> Self {
        Self {
            center: [center.x, center.y, center.z, 0.0],
            right_size: [right.x, right.y, right.z, size.x.max(0.001)],
            up_size: [up.x, up.y, up.z, size.y.max(0.001)],
            tint,
        }
    }

    fn overlay(center: Vec3, size: Vec2, tint: [f32; 4]) -> Self {
        Self {
            center: [center.x, center.y, center.z.clamp(0.0, 1.0), 1.0],
            right_size: [1.0, 0.0, 0.0, size.x.max(0.001)],
            up_size: [0.0, 1.0, 0.0, size.y.max(0.001)],
            tint,
        }
    }

    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteInstanceRaw>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &SPRITE_INSTANCE_ATTRIBUTES,
        }
    }
}

#[derive(Debug)]
struct SpriteTexture {
    _texture: Texture,
    bind_group: wgpu::BindGroup,
    revision: u64,
}

#[derive(Debug)]
struct MaterialTexture {
    _texture: Texture,
    revision: u64,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct MaterialTextureSetKey {
    albedo: Option<String>,
    normal: Option<String>,
    roughness: Option<String>,
}

#[derive(Debug)]
struct MaterialTextureSet {
    bind_group: wgpu::BindGroup,
    revisions: [u64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaterialFallback {
    White,
    Normal,
}

impl MaterialTextureSetKey {
    fn from_material(material: &MaterialBatchKey) -> Self {
        Self {
            albedo: material_texture_label(material.albedo_texture.as_deref()),
            normal: material_texture_label(material.normal_texture.as_deref()),
            roughness: material_texture_label(material.roughness_texture.as_deref()),
        }
    }

    fn is_empty(&self) -> bool {
        self.albedo.is_none() && self.normal.is_none() && self.roughness.is_none()
    }
}

fn material_texture_label(label: Option<&str>) -> Option<String> {
    label
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(|label| label.trim_start_matches('#').to_string())
}

#[derive(Debug)]
struct SpriteBatch {
    sprite: SpriteId,
    depth: SpriteDepthMode,
    instances: InstanceBatch,
}

struct TerrainMeshEntry {
    mesh: Mesh3D,
    revision: u64,
}

#[derive(Debug)]
struct TerrainDraw {
    entity: Entity,
    instances: InstanceBatch,
}

#[derive(Debug)]
struct MeshHandleDraw {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instances: InstanceBatch,
}

enum TransparentDraw<'a> {
    Cube(&'a InstanceBatch),
    Sphere {
        instances: &'a InstanceBatch,
        mesh: &'a Mesh3D,
    },
    Mesh(&'a MeshHandleDraw),
    Terrain {
        instances: &'a InstanceBatch,
        mesh: &'a Mesh3D,
    },
}

impl TransparentDraw<'_> {
    fn sort_depth(&self) -> f32 {
        match self {
            Self::Cube(instances)
            | Self::Sphere { instances, .. }
            | Self::Terrain { instances, .. } => instances.sort_depth,
            Self::Mesh(draw) => draw.instances.sort_depth,
        }
    }

    fn kind_order(&self) -> u8 {
        match self {
            Self::Cube(_) => 0,
            Self::Sphere { .. } => 1,
            Self::Mesh(_) => 2,
            Self::Terrain { .. } => 3,
        }
    }
}

struct ViewCameraBinding {
    _buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

struct SceneViewDraw {
    camera: ViewCameraBinding,
    viewport: CameraViewport,
    clear_color: wgpu::Color,
    cube_instances: Vec<InstanceBatch>,
    sphere_instances: Vec<SphereInstanceBatch>,
    mesh_handle_draws: Vec<MeshHandleDraw>,
    terrain_draws: Vec<TerrainDraw>,
    sprite_batches: Vec<SpriteBatch>,
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
    alpha_pipeline: wgpu::RenderPipeline,
    sprite_world_pipeline: wgpu::RenderPipeline,
    sprite_overlay_pipeline: wgpu::RenderPipeline,
    sprite_texture_layout: wgpu::BindGroupLayout,
    material_texture_layout: wgpu::BindGroupLayout,
    fallback_material_texture: MaterialTexture,
    fallback_normal_texture: MaterialTexture,
    fallback_material_bind_group: wgpu::BindGroup,
    depth_texture: DepthTexture,
    cube_mesh: Mesh3D,
    sphere_meshes: HashMap<(u32, u32), Mesh3D>,
    terrain_meshes: HashMap<Entity, TerrainMeshEntry>,
    view_draws: Vec<SceneViewDraw>,
    sprite_vertex_buffer: wgpu::Buffer,
    sprite_index_buffer: wgpu::Buffer,
    sprite_index_count: u32,
    sprite_textures: HashMap<SpriteId, SpriteTexture>,
    material_textures: HashMap<String, MaterialTexture>,
    material_texture_sets: HashMap<MaterialTextureSetKey, MaterialTextureSet>,
    target_width: u32,
    target_height: u32,
    gizmo_pipeline: wgpu::RenderPipeline,
    gizmo_vertex_buffer: wgpu::Buffer,
    gizmo_vertex_count: u32,
    clear_color: wgpu::Color,
    stats: SceneRendererStats,
}

impl SceneRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let camera_buffer = CameraBuffer::new(device);
        let light_buffer = LightBuffer::new(device);
        let shader = create_shader(device, SCENE_RENDERER_SHADER, Some("Scene Renderer Shader"));
        let material_texture_layout =
            create_material_texture_bind_group_layout(device, "Scene Material Texture Layout");
        let pipeline = create_scene_pipeline(
            device,
            &shader,
            ScenePipelineConfig {
                format,
                camera_layout: &camera_buffer.bind_group_layout,
                light_layout: &light_buffer.bind_group_layout,
                material_texture_layout: &material_texture_layout,
                blend: None,
                depth_write_enabled: true,
                label: "Scene Renderer Pipeline",
            },
        );
        let alpha_pipeline = create_scene_pipeline(
            device,
            &shader,
            ScenePipelineConfig {
                format,
                camera_layout: &camera_buffer.bind_group_layout,
                light_layout: &light_buffer.bind_group_layout,
                material_texture_layout: &material_texture_layout,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                depth_write_enabled: false,
                label: "Scene Renderer Alpha Pipeline",
            },
        );
        let sprite_texture_layout =
            create_texture_bind_group_layout(device, "Scene Sprite Texture Layout");
        let fallback_material_texture = create_material_texture(
            device,
            queue,
            &[255, 255, 255, 255],
            (1, 1),
            "Scene Fallback Material Texture",
        );
        let fallback_normal_texture = create_material_texture(
            device,
            queue,
            &[128, 128, 255, 255],
            (1, 1),
            "Scene Fallback Normal Texture",
        );
        let fallback_material_bind_group = create_material_texture_set_bind_group(
            device,
            &material_texture_layout,
            &fallback_material_texture,
            &fallback_normal_texture,
            &fallback_material_texture,
            "Scene Fallback Material Texture Set",
        );
        let sprite_shader = create_shader(device, SPRITE_SHADER, Some("Scene Sprite Shader"));
        let sprite_world_pipeline = create_sprite_pipeline(
            device,
            &sprite_shader,
            format,
            &camera_buffer.bind_group_layout,
            &sprite_texture_layout,
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            "Scene Sprite World Pipeline",
        );
        let sprite_overlay_pipeline = create_sprite_pipeline(
            device,
            &sprite_shader,
            format,
            &camera_buffer.bind_group_layout,
            &sprite_texture_layout,
            None,
            "Scene Sprite Overlay Pipeline",
        );
        let (sprite_vertex_buffer, sprite_index_buffer, sprite_index_count) =
            create_sprite_quad_buffers(device);
        let gizmo_shader =
            create_shader(device, GIZMO_LINE_SHADER, Some("Scene Gizmo Line Shader"));
        let gizmo_pipeline = create_gizmo_line_pipeline(
            device,
            &gizmo_shader,
            format,
            &camera_buffer.bind_group_layout,
        );
        let gizmo_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Scene Gizmo Line Vertex Buffer"),
            size: (8192 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let clear_color = wgpu::Color {
            r: 0.07,
            g: 0.09,
            b: 0.12,
            a: 1.0,
        };

        Self {
            camera_buffer,
            light_buffer,
            pipeline,
            alpha_pipeline,
            sprite_world_pipeline,
            sprite_overlay_pipeline,
            sprite_texture_layout,
            material_texture_layout,
            fallback_material_texture,
            fallback_normal_texture,
            fallback_material_bind_group,
            depth_texture: DepthTexture::new(device, width, height, Some("Scene Renderer Depth")),
            cube_mesh: Mesh3D::new_cube(device),
            sphere_meshes: HashMap::new(),
            terrain_meshes: HashMap::new(),
            view_draws: Vec::new(),
            sprite_vertex_buffer,
            sprite_index_buffer,
            sprite_index_count,
            sprite_textures: HashMap::new(),
            material_textures: HashMap::new(),
            material_texture_sets: HashMap::new(),
            target_width: width,
            target_height: height,
            gizmo_pipeline,
            gizmo_vertex_buffer,
            gizmo_vertex_count: 0,
            clear_color,
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
        self.light_buffer.update(device, queue, world);
        self.sync_material_textures(device, queue, world);
        self.sync_sprite_textures(device, queue, world);
        self.prepare_gizmo_lines(queue, world);
        self.prune_terrain_meshes(world);

        let mut cameras = active_camera_views(world);
        if cameras.is_empty() {
            cameras.push(PreparedCameraView::fallback());
        }

        self.view_draws.clear();
        let mut stats = SceneRendererStats::default();
        for camera in cameras {
            let view = self.prepare_view(device, queue, world, aspect_ratio, camera);
            stats.cube_instances += view
                .cube_instances
                .iter()
                .map(|batch| batch.count)
                .sum::<u32>();
            stats.sphere_instances += view
                .sphere_instances
                .iter()
                .map(|batch| batch.instances.count)
                .sum::<u32>();
            stats.mesh_handle_instances += view
                .mesh_handle_draws
                .iter()
                .map(|draw| draw.instances.count)
                .sum::<u32>();
            stats.terrain_instances += view.terrain_draws.len() as u32;
            stats.sprite_instances += view
                .sprite_batches
                .iter()
                .map(|batch| batch.instances.count)
                .sum::<u32>();
            stats.draw_calls += view.cube_instances.len() as u32
                + view.sphere_instances.len() as u32
                + view.mesh_handle_draws.len() as u32
                + view.terrain_draws.len() as u32
                + view.sprite_batches.len() as u32
                + u32::from(self.gizmo_vertex_count > 0);
            self.view_draws.push(view);
        }
        self.stats = stats;
    }

    pub fn queue(&mut self, view: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder) {
        for (index, scene_view) in self.view_draws.iter().enumerate() {
            let color_load = if index == 0 {
                wgpu::LoadOp::Clear(scene_view.clear_color)
            } else {
                wgpu::LoadOp::Load
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene Renderer Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: color_load,
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

            apply_render_viewport(
                &mut render_pass,
                scene_view.viewport,
                self.target_width,
                self.target_height,
            );

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &scene_view.camera.bind_group, &[]);
            render_pass.set_bind_group(1, &self.light_buffer.bind_group, &[]);
            self.queue_scene_geometry(&mut render_pass, scene_view, AlphaMode::Opaque);
            self.queue_scene_geometry(&mut render_pass, scene_view, AlphaMode::Mask);

            render_pass.set_pipeline(&self.alpha_pipeline);
            self.queue_scene_geometry(&mut render_pass, scene_view, AlphaMode::Blend);

            self.queue_sprites(&mut render_pass, scene_view);
            self.queue_gizmo_lines(&mut render_pass, scene_view);
        }
    }

    fn queue_scene_geometry<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        scene_view: &'pass SceneViewDraw,
        alpha_mode: AlphaMode,
    ) {
        if alpha_mode.is_transparent_phase() {
            self.queue_transparent_scene_geometry(render_pass, scene_view);
            return;
        }

        for instances in &scene_view.cube_instances {
            if instances.alpha_mode != alpha_mode {
                continue;
            }
            set_material_texture(render_pass, &self.fallback_material_bind_group, instances);
            draw_mesh_batch(render_pass, &self.cube_mesh, instances);
        }

        for batch in &scene_view.sphere_instances {
            if batch.instances.alpha_mode != alpha_mode {
                continue;
            }
            if let Some(mesh) = self.sphere_meshes.get(&(batch.segments, batch.rings)) {
                set_material_texture(
                    render_pass,
                    &self.fallback_material_bind_group,
                    &batch.instances,
                );
                draw_mesh_batch(render_pass, mesh, &batch.instances);
            }
        }

        for draw in &scene_view.mesh_handle_draws {
            if draw.instances.alpha_mode != alpha_mode {
                continue;
            }
            set_material_texture(
                render_pass,
                &self.fallback_material_bind_group,
                &draw.instances,
            );
            draw_prepared_mesh_batch(render_pass, draw);
        }

        for terrain in &scene_view.terrain_draws {
            if terrain.instances.alpha_mode != alpha_mode {
                continue;
            }
            if let Some(entry) = self.terrain_meshes.get(&terrain.entity) {
                set_material_texture(
                    render_pass,
                    &self.fallback_material_bind_group,
                    &terrain.instances,
                );
                draw_mesh_batch(render_pass, &entry.mesh, &terrain.instances);
            }
        }
    }

    fn queue_transparent_scene_geometry<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        scene_view: &'pass SceneViewDraw,
    ) {
        let mut draws = Vec::new();

        for instances in &scene_view.cube_instances {
            if instances.alpha_mode.is_transparent_phase() {
                draws.push(TransparentDraw::Cube(instances));
            }
        }
        for batch in &scene_view.sphere_instances {
            if !batch.instances.alpha_mode.is_transparent_phase() {
                continue;
            }
            if let Some(mesh) = self.sphere_meshes.get(&(batch.segments, batch.rings)) {
                draws.push(TransparentDraw::Sphere {
                    instances: &batch.instances,
                    mesh,
                });
            }
        }
        for draw in &scene_view.mesh_handle_draws {
            if draw.instances.alpha_mode.is_transparent_phase() {
                draws.push(TransparentDraw::Mesh(draw));
            }
        }
        for terrain in &scene_view.terrain_draws {
            if !terrain.instances.alpha_mode.is_transparent_phase() {
                continue;
            }
            if let Some(entry) = self.terrain_meshes.get(&terrain.entity) {
                draws.push(TransparentDraw::Terrain {
                    instances: &terrain.instances,
                    mesh: &entry.mesh,
                });
            }
        }

        draws.sort_by(|left, right| {
            right
                .sort_depth()
                .total_cmp(&left.sort_depth())
                .then_with(|| left.kind_order().cmp(&right.kind_order()))
        });

        for draw in draws {
            match draw {
                TransparentDraw::Cube(instances) => {
                    set_material_texture(
                        render_pass,
                        &self.fallback_material_bind_group,
                        instances,
                    );
                    draw_mesh_batch(render_pass, &self.cube_mesh, instances);
                }
                TransparentDraw::Sphere { instances, mesh } => {
                    set_material_texture(
                        render_pass,
                        &self.fallback_material_bind_group,
                        instances,
                    );
                    draw_mesh_batch(render_pass, mesh, instances);
                }
                TransparentDraw::Mesh(draw) => {
                    set_material_texture(
                        render_pass,
                        &self.fallback_material_bind_group,
                        &draw.instances,
                    );
                    draw_prepared_mesh_batch(render_pass, draw);
                }
                TransparentDraw::Terrain { instances, mesh } => {
                    set_material_texture(
                        render_pass,
                        &self.fallback_material_bind_group,
                        instances,
                    );
                    draw_mesh_batch(render_pass, mesh, instances);
                }
            }
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.depth_texture.resize(device, width, height);
        self.target_width = width;
        self.target_height = height;
    }

    fn prepare_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &mut World,
        target_aspect_ratio: f32,
        camera: PreparedCameraView,
    ) -> SceneViewDraw {
        let viewport = camera.viewport.unwrap_or_else(CameraViewport::full);
        let aspect_ratio =
            viewport_aspect_ratio(viewport, target_aspect_ratio).unwrap_or(target_aspect_ratio);
        let camera_binding = create_view_camera_binding(
            device,
            queue,
            &self.camera_buffer.bind_group_layout,
            &camera,
            aspect_ratio,
        );
        let (cube_instances, sphere_instances) =
            self.prepare_instances(device, world, camera.layers, camera.camera.0.position);
        let mesh_handle_draws =
            self.prepare_mesh_handle_draws(device, world, camera.layers, camera.camera.0.position);
        let terrain_draws =
            self.prepare_terrain(device, world, camera.layers, camera.camera.0.position);
        let sprite_batches = self.prepare_sprites(device, world, camera);

        SceneViewDraw {
            camera: camera_binding,
            viewport,
            clear_color: camera.clear_color.unwrap_or(self.clear_color),
            cube_instances,
            sphere_instances,
            mesh_handle_draws,
            terrain_draws,
            sprite_batches,
        }
    }

    fn prepare_instances(
        &mut self,
        device: &wgpu::Device,
        world: &mut World,
        camera_layers: RenderLayers,
        camera_position: Vec3,
    ) -> (Vec<InstanceBatch>, Vec<SphereInstanceBatch>) {
        let material_library = world.get_resource::<SceneMaterialLibrary>().cloned();
        let renderables = collect_renderables(world, camera_layers);
        let mut cube_instances = BTreeMap::<MaterialBatchKey, Vec<SceneInstanceRaw>>::new();
        let mut sphere_instances =
            BTreeMap::<(u32, u32, MaterialBatchKey), Vec<SceneInstanceRaw>>::new();

        for (entity, render_mesh) in renderables {
            let model = entity_model_matrix(world, entity);
            let resolved_material = resolve_entity_material(world, entity, Some(&render_mesh));
            let material = MaterialBatchKey::from_material_with_library(
                &resolved_material,
                material_library.as_ref(),
            );
            let base_color = resolved_material.base_color_with_library(material_library.as_ref());
            let instance = SceneInstanceRaw::new(
                model,
                multiply_color(render_mesh.tint, base_color),
                material.clone(),
            );
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

        let cube_batches = cube_instances
            .iter()
            .filter_map(|(material, instances)| {
                create_material_instance_batch(
                    device,
                    "Scene Cube Instances",
                    instances,
                    material.alpha_mode,
                    batch_sort_depth(instances, camera_position),
                    self.material_bind_group_for(device, material),
                )
            })
            .collect();
        let mut sphere_batches = Vec::new();
        for ((segments, rings, material), instances) in sphere_instances {
            self.sphere_meshes
                .entry((segments, rings))
                .or_insert_with(|| Mesh3D::new_sphere(device, segments, rings));

            if let Some(batch) = create_material_instance_batch(
                device,
                "Scene Sphere Instances",
                &instances,
                material.alpha_mode,
                batch_sort_depth(&instances, camera_position),
                self.material_bind_group_for(device, &material),
            ) {
                sphere_batches.push(SphereInstanceBatch {
                    segments,
                    rings,
                    instances: batch,
                });
            }
        }

        (cube_batches, sphere_batches)
    }

    fn prepare_mesh_handle_draws(
        &mut self,
        device: &wgpu::Device,
        world: &mut World,
        camera_layers: RenderLayers,
        camera_position: Vec3,
    ) -> Vec<MeshHandleDraw> {
        let material_library = world.get_resource::<SceneMaterialLibrary>().cloned();
        let renderables = collect_mesh_filter_renderables(world, camera_layers);
        let mut batches = BTreeMap::<(u64, MaterialBatchKey), Vec<SceneInstanceRaw>>::new();

        for (entity, mesh_filter) in renderables {
            let render_mesh = world.get::<RenderMesh>(entity).cloned();
            let resolved_material = resolve_entity_material(world, entity, render_mesh.as_ref());
            let material = MaterialBatchKey::from_material_with_library(
                &resolved_material,
                material_library.as_ref(),
            );
            let base_color = resolved_material.base_color_with_library(material_library.as_ref());
            let tint = render_mesh
                .as_ref()
                .map(|render_mesh| render_mesh.tint)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let instance = SceneInstanceRaw::new(
                entity_model_matrix(world, entity),
                multiply_color(tint, base_color),
                material.clone(),
            );
            batches
                .entry((mesh_filter.mesh.id(), material))
                .or_default()
                .push(instance);
        }

        let Some(mesh_cache) = world.get_resource::<MeshCache>() else {
            return Vec::new();
        };

        let mut draws = Vec::new();
        for ((mesh_id, material), instances) in batches {
            let mesh_handle = oxide_asset::Handle::new(mesh_id);
            let Some(mesh) = mesh_cache.get(mesh_handle) else {
                continue;
            };
            if let Some(instances) = create_material_instance_batch(
                device,
                "Scene Mesh Handle Instances",
                &instances,
                material.alpha_mode,
                batch_sort_depth(&instances, camera_position),
                self.material_bind_group_for(device, &material),
            ) {
                draws.push(MeshHandleDraw {
                    vertex_buffer: mesh.vertex_buffer.clone(),
                    index_buffer: mesh.index_buffer.clone(),
                    index_count: mesh.index_count,
                    instances,
                });
            }
        }

        draws
    }

    fn prepare_terrain(
        &mut self,
        device: &wgpu::Device,
        world: &mut World,
        camera_layers: RenderLayers,
        camera_position: Vec3,
    ) -> Vec<TerrainDraw> {
        let material_library = world.get_resource::<SceneMaterialLibrary>().cloned();
        let terrains = collect_terrains(world, camera_layers);
        let mut draws = Vec::new();

        for (entity, terrain) in terrains {
            let needs_rebuild = self
                .terrain_meshes
                .get(&entity)
                .map(|entry| entry.revision != terrain.revision())
                .unwrap_or(true);
            if needs_rebuild {
                let (vertices, indices) = terrain_mesh_data(&terrain);
                self.terrain_meshes.insert(
                    entity,
                    TerrainMeshEntry {
                        mesh: Mesh3D::create(device, &vertices, &indices, Some("Terrain")),
                        revision: terrain.revision(),
                    },
                );
            }

            let model = entity_model_matrix(world, entity);
            let material = MaterialBatchKey::from_material_with_library(
                &terrain.material,
                material_library.as_ref(),
            );
            let base_color = terrain
                .material
                .base_color_with_library(material_library.as_ref());
            let material_texture = self.material_bind_group_for(device, &material);
            let alpha_mode = material.alpha_mode;
            let instance =
                SceneInstanceRaw::new(model, multiply_color(terrain.tint, base_color), material);
            if let Some(instances) = create_material_instance_batch(
                device,
                "Terrain Instances",
                &[instance],
                alpha_mode,
                batch_sort_depth(&[instance], camera_position),
                material_texture,
            ) {
                draws.push(TerrainDraw { entity, instances });
            }
        }

        draws
    }

    fn material_bind_group_for(
        &mut self,
        device: &wgpu::Device,
        material: &MaterialBatchKey,
    ) -> Option<wgpu::BindGroup> {
        let key = MaterialTextureSetKey::from_material(material);
        if key.is_empty() {
            return None;
        }

        let revisions = self.material_texture_set_revisions(&key);
        if let Some(cached) = self.material_texture_sets.get(&key) {
            if cached.revisions == revisions {
                return Some(cached.bind_group.clone());
            }
        }

        let bind_group = {
            let albedo =
                self.material_texture_or_fallback(key.albedo.as_deref(), MaterialFallback::White);
            let normal =
                self.material_texture_or_fallback(key.normal.as_deref(), MaterialFallback::Normal);
            let roughness = self
                .material_texture_or_fallback(key.roughness.as_deref(), MaterialFallback::White);
            create_material_texture_set_bind_group(
                device,
                &self.material_texture_layout,
                albedo,
                normal,
                roughness,
                "Scene Material Texture Set",
            )
        };
        self.material_texture_sets.insert(
            key,
            MaterialTextureSet {
                bind_group: bind_group.clone(),
                revisions,
            },
        );
        Some(bind_group)
    }

    fn material_texture_or_fallback(
        &self,
        label: Option<&str>,
        fallback: MaterialFallback,
    ) -> &MaterialTexture {
        if let Some(texture) = label.and_then(|label| self.material_textures.get(label)) {
            return texture;
        }
        match fallback {
            MaterialFallback::White => &self.fallback_material_texture,
            MaterialFallback::Normal => &self.fallback_normal_texture,
        }
    }

    fn material_texture_set_revisions(&self, key: &MaterialTextureSetKey) -> [u64; 3] {
        [
            self.material_texture_or_fallback(key.albedo.as_deref(), MaterialFallback::White)
                .revision,
            self.material_texture_or_fallback(key.normal.as_deref(), MaterialFallback::Normal)
                .revision,
            self.material_texture_or_fallback(key.roughness.as_deref(), MaterialFallback::White)
                .revision,
        ]
    }

    fn prepare_sprites(
        &mut self,
        device: &wgpu::Device,
        world: &mut World,
        camera: PreparedCameraView,
    ) -> Vec<SpriteBatch> {
        let Some(camera) = CameraFrame::from_view(camera) else {
            return Vec::new();
        };

        let sprites = collect_sprites(world, camera.layers);
        let mut batches = BTreeMap::<(SpriteDepthMode, SpriteId), Vec<SpriteInstanceRaw>>::new();
        for (entity, sprite) in sprites {
            if !self.sprite_textures.contains_key(&sprite.sprite) {
                continue;
            }

            let (position, rotation, scale) = entity_transform_parts(world, entity);
            let size = Vec2::new(
                sprite.size.x * scale.x.abs().max(0.001),
                sprite.size.y * scale.y.abs().max(0.001),
            );
            let instance = if sprite.depth == SpriteDepthMode::Overlay {
                SpriteInstanceRaw::overlay(position, size, sprite.tint)
            } else {
                let (right, up) = sprite_axes(sprite.facing, position, rotation, &camera);
                SpriteInstanceRaw::new(position, right, up, size, sprite.tint)
            };
            batches
                .entry((sprite.depth, sprite.sprite.clone()))
                .or_default()
                .push(instance);
        }

        batches
            .into_iter()
            .filter_map(|((depth, sprite), instances)| {
                create_instance_batch(device, "Scene Sprite Instances", &instances).map(
                    |instance_batch| SpriteBatch {
                        sprite,
                        depth,
                        instances: instance_batch,
                    },
                )
            })
            .collect()
    }

    fn prune_terrain_meshes(&mut self, world: &mut World) {
        let active_entities: HashSet<Entity> = collect_terrains(world, RenderLayers::all())
            .into_iter()
            .map(|(entity, _)| entity)
            .collect();
        self.terrain_meshes
            .retain(|entity, _| active_entities.contains(entity));
    }

    fn sync_sprite_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &mut World,
    ) {
        if !world.contains_resource::<SpriteAssets>() {
            self.sprite_textures.clear();
            return;
        }

        let active_sprite_ids: HashSet<SpriteId> = collect_sprite_ids(world).into_iter().collect();
        self.sprite_textures
            .retain(|id, _| active_sprite_ids.contains(id));

        let assets = world.resource::<SpriteAssets>();

        for id in active_sprite_ids {
            let Some(asset) = assets.get(&id) else {
                continue;
            };
            let current_revision = self
                .sprite_textures
                .get(&id)
                .map(|texture| texture.revision);
            if current_revision == Some(asset.revision) {
                continue;
            }

            let texture = Texture::from_bytes(
                device,
                queue,
                asset.image.rgba(),
                (asset.image.width(), asset.image.height()),
                Some(id.as_str()),
            )
            .with_sampler(
                device,
                &SamplerDescriptor {
                    mag_filter: wgpu::FilterMode::Nearest,
                    min_filter: wgpu::FilterMode::Nearest,
                    ..Default::default()
                },
            );

            let bind_group = create_texture_bind_group(
                device,
                &self.sprite_texture_layout,
                &texture,
                "Scene Sprite Texture Bind Group",
            );

            self.sprite_textures.insert(
                id,
                SpriteTexture {
                    _texture: texture,
                    bind_group,
                    revision: asset.revision,
                },
            );
        }
    }

    fn sync_material_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &World,
    ) {
        let Some(assets) = world.get_resource::<TextureImageAssets>() else {
            self.material_textures.clear();
            self.material_texture_sets.clear();
            return;
        };

        let active_labels: HashSet<String> = assets
            .iter_labeled()
            .map(|(label, _, _)| label.to_string())
            .collect();
        let previous_len = self.material_textures.len();
        self.material_textures
            .retain(|label, _| active_labels.contains(label));
        let mut changed = previous_len != self.material_textures.len();

        for (label, handle, image) in assets.iter_labeled() {
            let revision = assets.assets.revision(&handle).unwrap_or(0);
            let current_revision = self
                .material_textures
                .get(label)
                .map(|texture| texture.revision);
            if current_revision == Some(revision) {
                continue;
            }

            let texture = Texture::from_image(device, queue, image, Some(label));
            self.material_textures.insert(
                label.to_string(),
                MaterialTexture {
                    _texture: texture,
                    revision,
                },
            );
            changed = true;
        }

        if changed {
            self.material_texture_sets.clear();
        }
    }

    fn queue_sprites<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        scene_view: &'pass SceneViewDraw,
    ) {
        render_pass.set_vertex_buffer(0, self.sprite_vertex_buffer.slice(..));
        render_pass.set_index_buffer(
            self.sprite_index_buffer.slice(..),
            wgpu::IndexFormat::Uint16,
        );

        for batch in &scene_view.sprite_batches {
            let Some(texture) = self.sprite_textures.get(&batch.sprite) else {
                continue;
            };
            match batch.depth {
                SpriteDepthMode::World => render_pass.set_pipeline(&self.sprite_world_pipeline),
                SpriteDepthMode::Overlay => render_pass.set_pipeline(&self.sprite_overlay_pipeline),
            }
            render_pass.set_bind_group(0, &scene_view.camera.bind_group, &[]);
            render_pass.set_bind_group(1, &texture.bind_group, &[]);
            render_pass.set_vertex_buffer(1, batch.instances.buffer.slice(..));
            render_pass.draw_indexed(0..self.sprite_index_count, 0, 0..batch.instances.count);
        }
    }

    fn prepare_gizmo_lines(&mut self, queue: &wgpu::Queue, world: &World) {
        self.gizmo_vertex_count = 0;
        if !world.contains_resource::<SceneGizmoLines>() {
            return;
        }

        let lines = world.resource::<SceneGizmoLines>();
        if lines.is_empty() {
            return;
        }

        let mut vertices = Vec::with_capacity(lines.lines().len() * 2);
        for line in lines.lines().iter().take(4096) {
            let color = [line.color.x, line.color.y, line.color.z];
            vertices.push(Vertex::new(
                [line.start.x, line.start.y, line.start.z],
                color,
            ));
            vertices.push(Vertex::new([line.end.x, line.end.y, line.end.z], color));
        }

        self.gizmo_vertex_count = vertices.len() as u32;
        queue.write_buffer(
            &self.gizmo_vertex_buffer,
            0,
            bytemuck::cast_slice(&vertices),
        );
    }

    fn queue_gizmo_lines<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        scene_view: &'pass SceneViewDraw,
    ) {
        if self.gizmo_vertex_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.gizmo_pipeline);
        render_pass.set_bind_group(0, &scene_view.camera.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.gizmo_vertex_buffer.slice(..));
        render_pass.draw(0..self.gizmo_vertex_count, 0..1);
    }
}

struct ScenePipelineConfig<'a> {
    format: wgpu::TextureFormat,
    camera_layout: &'a wgpu::BindGroupLayout,
    light_layout: &'a wgpu::BindGroupLayout,
    material_texture_layout: &'a wgpu::BindGroupLayout,
    blend: Option<wgpu::BlendState>,
    depth_write_enabled: bool,
    label: &'a str,
}

fn create_scene_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    config: ScenePipelineConfig<'_>,
) -> wgpu::RenderPipeline {
    let layout_label = format!("{} Layout", config.label);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&layout_label),
        bind_group_layouts: &[
            Some(config.camera_layout),
            Some(config.light_layout),
            Some(config.material_texture_layout),
        ],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(config.label),
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
                format: config.format,
                blend: config.blend,
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
            depth_write_enabled: Some(config.depth_write_enabled),
            depth_compare: Some(if config.depth_write_enabled {
                wgpu::CompareFunction::Less
            } else {
                wgpu::CompareFunction::LessEqual
            }),
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

fn create_texture_bind_group_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_material_texture_bind_group_layout(
    device: &wgpu::Device,
    label: &str,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture: &Texture,
    label: &str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&texture.sampler),
            },
        ],
    })
}

fn create_material_texture_set_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    albedo: &MaterialTexture,
    normal: &MaterialTexture,
    roughness: &MaterialTexture,
    label: &str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&albedo._texture.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&normal._texture.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&roughness._texture.view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&albedo._texture.sampler),
            },
        ],
    })
}

fn create_material_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: &[u8],
    dimensions: (u32, u32),
    label: &str,
) -> MaterialTexture {
    let texture = Texture::from_bytes(device, queue, rgba, dimensions, Some(label));
    MaterialTexture {
        _texture: texture,
        revision: 0,
    }
}

fn create_sprite_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    sprite_texture_layout: &wgpu::BindGroupLayout,
    depth_stencil: Option<wgpu::DepthStencilState>,
    label: &str,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Scene Sprite Pipeline Layout"),
        bind_group_layouts: &[Some(camera_layout), Some(sprite_texture_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[SpriteVertex::desc(), SpriteInstanceRaw::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

fn create_gizmo_line_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Scene Gizmo Line Pipeline Layout"),
        bind_group_layouts: &[Some(camera_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Scene Gizmo Line Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
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

fn create_sprite_quad_buffers(device: &wgpu::Device) -> (wgpu::Buffer, wgpu::Buffer, u32) {
    let vertices = [
        SpriteVertex {
            local_position: [-0.5, -0.5],
            uv: [0.0, 1.0],
        },
        SpriteVertex {
            local_position: [0.5, -0.5],
            uv: [1.0, 1.0],
        },
        SpriteVertex {
            local_position: [0.5, 0.5],
            uv: [1.0, 0.0],
        },
        SpriteVertex {
            local_position: [-0.5, 0.5],
            uv: [0.0, 0.0],
        },
    ];
    let indices: [u16; 6] = [0, 1, 2, 2, 3, 0];

    let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Scene Sprite Quad Vertex Buffer"),
        size: std::mem::size_of_val(&vertices) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(bytemuck::cast_slice(&vertices));
    vertex_buffer.unmap();

    let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Scene Sprite Quad Index Buffer"),
        size: std::mem::size_of_val(&indices) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::INDEX,
        mapped_at_creation: true,
    });
    index_buffer
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(bytemuck::cast_slice(&indices));
    index_buffer.unmap();

    (vertex_buffer, index_buffer, indices.len() as u32)
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

fn create_instance_batch<T: Pod>(
    device: &wgpu::Device,
    label: &str,
    instances: &[T],
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
        alpha_mode: AlphaMode::Opaque,
        sort_depth: 0.0,
        material_texture: None,
    })
}

fn create_material_instance_batch(
    device: &wgpu::Device,
    label: &str,
    instances: &[SceneInstanceRaw],
    alpha_mode: AlphaMode,
    sort_depth: f32,
    material_texture: Option<wgpu::BindGroup>,
) -> Option<InstanceBatch> {
    create_instance_batch(device, label, instances).map(|mut batch| {
        batch.alpha_mode = alpha_mode;
        batch.sort_depth = sort_depth;
        batch.material_texture = material_texture;
        batch
    })
}

fn batch_sort_depth(instances: &[SceneInstanceRaw], camera_position: Vec3) -> f32 {
    instances
        .iter()
        .map(|instance| instance_world_position(instance).distance_squared(camera_position))
        .fold(0.0, f32::max)
}

fn instance_world_position(instance: &SceneInstanceRaw) -> Vec3 {
    Vec3::new(
        instance.model[3][0],
        instance.model[3][1],
        instance.model[3][2],
    )
}

fn create_view_camera_binding(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    camera: &PreparedCameraView,
    aspect_ratio: f32,
) -> ViewCameraBinding {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Scene View Camera Buffer"),
        size: std::mem::size_of::<CameraUniform>() as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Scene View Camera Bind Group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });

    let mut uniform = CameraUniform::new();
    uniform.update(
        camera.camera.0.view_projection_matrix(aspect_ratio),
        camera.camera.0.position,
    );
    queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&uniform));

    ViewCameraBinding {
        _buffer: buffer,
        bind_group,
    }
}

fn collect_renderables(
    world: &mut World,
    camera_layers: RenderLayers,
) -> Vec<(Entity, RenderMesh)> {
    let mut query = world.query::<(Entity, &RenderMesh)>();
    query
        .iter(world)
        .filter(|(entity, _)| is_visible(world, *entity))
        .filter(|(entity, _)| render_layers(world, *entity).intersects(camera_layers))
        .filter(|(entity, _)| world.get::<MeshFilter>(*entity).is_none())
        .map(|(entity, render_mesh)| (entity, render_mesh.clone()))
        .collect()
}

fn collect_mesh_filter_renderables(
    world: &mut World,
    camera_layers: RenderLayers,
) -> Vec<(Entity, MeshFilter)> {
    let mut query = world.query::<(Entity, &MeshFilter)>();
    query
        .iter(world)
        .filter(|(entity, _)| is_visible(world, *entity))
        .filter(|(entity, _)| render_layers(world, *entity).intersects(camera_layers))
        .map(|(entity, mesh_filter)| (entity, mesh_filter.clone()))
        .collect()
}

fn resolve_entity_material(
    world: &World,
    entity: Entity,
    render_mesh: Option<&RenderMesh>,
) -> RenderMaterial {
    if let Some(material_filter) = world.get::<MaterialFilter>(entity) {
        if let Some(descriptor) = world
            .get_resource::<MaterialDescriptorAssets>()
            .and_then(|assets| assets.assets.get(&material_filter.material))
        {
            return RenderMaterial::from_descriptor(descriptor);
        }
    }

    render_mesh
        .map(|render_mesh| render_mesh.material.clone())
        .unwrap_or_default()
}

fn collect_terrains(world: &mut World, camera_layers: RenderLayers) -> Vec<(Entity, Terrain)> {
    let mut query = world.query::<(Entity, &Terrain)>();
    query
        .iter(world)
        .filter(|(entity, _)| is_visible(world, *entity))
        .filter(|(entity, _)| render_layers(world, *entity).intersects(camera_layers))
        .map(|(entity, terrain)| (entity, terrain.clone()))
        .collect()
}

fn collect_sprites(
    world: &mut World,
    camera_layers: RenderLayers,
) -> Vec<(Entity, SpriteBillboard)> {
    let mut query = world.query::<(Entity, &SpriteBillboard)>();
    query
        .iter(world)
        .filter(|(entity, _)| is_visible(world, *entity))
        .filter(|(entity, _)| render_layers(world, *entity).intersects(camera_layers))
        .map(|(entity, sprite)| (entity, sprite.clone()))
        .collect()
}

fn collect_sprite_ids(world: &mut World) -> Vec<SpriteId> {
    let mut query = world.query::<&SpriteBillboard>();
    query
        .iter(world)
        .map(|sprite| sprite.sprite.clone())
        .collect()
}

#[derive(Clone, Copy)]
struct PreparedCameraView {
    camera: CameraComponent,
    layers: RenderLayers,
    viewport: Option<CameraViewport>,
    clear_color: Option<wgpu::Color>,
}

impl PreparedCameraView {
    fn fallback() -> Self {
        Self {
            camera: CameraComponent::default(),
            layers: RenderLayers::default(),
            viewport: Some(CameraViewport::full()),
            clear_color: None,
        }
    }
}

#[derive(Clone, Copy)]
struct CameraFrame {
    position: Vec3,
    right: Vec3,
    up: Vec3,
    layers: RenderLayers,
}

impl CameraFrame {
    fn from_view(view: PreparedCameraView) -> Option<Self> {
        let forward = view.camera.0.forward().normalize_or_zero();
        if forward.length_squared() <= f32::EPSILON {
            return None;
        }
        let mut up = view.camera.0.up.normalize_or_zero();
        if up.length_squared() <= f32::EPSILON {
            up = Vec3::Y;
        }
        let right = forward.cross(up).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();

        Some(Self {
            position: view.camera.0.position,
            right,
            up,
            layers: view.layers,
        })
    }
}

fn active_camera_views(world: &mut World) -> Vec<PreparedCameraView> {
    let cameras = {
        let mut query = world.query::<(Entity, &CameraComponent)>();
        query
            .iter(world)
            .map(|(entity, camera)| (entity, *camera))
            .collect::<Vec<_>>()
    };

    let mut cameras = cameras
        .into_iter()
        .filter_map(|(entity, camera)| {
            let view = world
                .get::<CameraRenderView>(entity)
                .copied()
                .unwrap_or_default();
            if !view.is_active {
                return None;
            }
            Some((entity, camera, view))
        })
        .collect::<Vec<_>>();
    cameras.sort_by_key(|(entity, _, view)| (view.order, entity.index(), entity.generation()));

    cameras
        .into_iter()
        .map(|(entity, camera, view)| PreparedCameraView {
            camera,
            layers: render_layers(world, entity),
            viewport: view.viewport,
            clear_color: view
                .clear_color
                .map(|[r, g, b, a]| wgpu::Color { r, g, b, a }),
        })
        .collect()
}

fn viewport_aspect_ratio(viewport: CameraViewport, target_aspect_ratio: f32) -> Option<f32> {
    if viewport.is_empty() {
        return None;
    }
    Some(target_aspect_ratio * viewport.width / viewport.height)
}

fn apply_render_viewport<'pass>(
    render_pass: &mut wgpu::RenderPass<'pass>,
    viewport: CameraViewport,
    target_width: u32,
    target_height: u32,
) {
    let viewport = viewport.clamped();
    if viewport.is_empty() || target_width == 0 || target_height == 0 {
        return;
    }

    let x = viewport.x * target_width as f32;
    let y = viewport.y * target_height as f32;
    let width = (viewport.width * target_width as f32).max(1.0);
    let height = (viewport.height * target_height as f32).max(1.0);

    render_pass.set_viewport(x, y, width, height, 0.0, 1.0);
    let scissor_x = x.floor().clamp(0.0, target_width as f32) as u32;
    let scissor_y = y.floor().clamp(0.0, target_height as f32) as u32;
    let scissor_max_x = (x + width).ceil().clamp(0.0, target_width as f32) as u32;
    let scissor_max_y = (y + height).ceil().clamp(0.0, target_height as f32) as u32;
    render_pass.set_scissor_rect(
        scissor_x,
        scissor_y,
        scissor_max_x.saturating_sub(scissor_x).max(1),
        scissor_max_y.saturating_sub(scissor_y).max(1),
    );
}

fn render_layers(world: &World, entity: Entity) -> RenderLayers {
    world
        .get::<RenderLayers>(entity)
        .copied()
        .unwrap_or_default()
}

fn sprite_axes(
    facing: SpriteFacing,
    position: Vec3,
    rotation: Quat,
    camera: &CameraFrame,
) -> (Vec3, Vec3) {
    match facing {
        SpriteFacing::Camera => (camera.right, camera.up),
        SpriteFacing::Fixed => (rotation * Vec3::X, rotation * Vec3::Y),
        SpriteFacing::YBillboard => {
            let mut forward = camera.position - position;
            forward.y = 0.0;
            let forward = forward.normalize_or_zero();
            if forward.length_squared() <= f32::EPSILON {
                (Vec3::X, Vec3::Y)
            } else {
                (Vec3::Y.cross(forward).normalize_or_zero(), Vec3::Y)
            }
        }
    }
}

fn entity_transform_parts(world: &World, entity: Entity) -> (Vec3, Quat, Vec3) {
    world
        .get::<TransformComponent>(entity)
        .map(|transform| {
            (
                transform.transform.position,
                transform.transform.rotation,
                transform.transform.scale,
            )
        })
        .unwrap_or((Vec3::ZERO, Quat::IDENTITY, Vec3::ONE))
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

fn terrain_mesh_data(terrain: &Terrain) -> (Vec<Vertex3D>, Vec<u16>) {
    let mut vertices =
        Vec::with_capacity((terrain.columns + 1) as usize * (terrain.rows + 1) as usize);
    for row in 0..=terrain.rows {
        for column in 0..=terrain.columns {
            let x_ratio = column as f32 / terrain.columns as f32;
            let z_ratio = row as f32 / terrain.rows as f32;
            let x = (x_ratio - 0.5) * terrain.width;
            let z = (z_ratio - 0.5) * terrain.depth;
            let y = terrain.height_at(column, row);
            let normal = terrain_normal(terrain, column, row);
            vertices.push(Vertex3D::new(
                [x, y, z],
                [normal.x, normal.y, normal.z],
                [x_ratio, z_ratio],
            ));
        }
    }

    let mut indices = Vec::with_capacity(terrain.columns as usize * terrain.rows as usize * 6);
    for row in 0..terrain.rows {
        for column in 0..terrain.columns {
            let stride = terrain.columns + 1;
            let i0 = row * stride + column;
            let i1 = i0 + 1;
            let i2 = i0 + stride;
            let i3 = i2 + 1;
            indices.extend_from_slice(&[
                i0 as u16, i2 as u16, i1 as u16, i1 as u16, i2 as u16, i3 as u16,
            ]);
        }
    }

    (vertices, indices)
}

fn terrain_normal(terrain: &Terrain, column: u32, row: u32) -> Vec3 {
    let left = terrain.height_at(column.saturating_sub(1), row);
    let right = terrain.height_at((column + 1).min(terrain.columns), row);
    let down = terrain.height_at(column, row.saturating_sub(1));
    let up = terrain.height_at(column, (row + 1).min(terrain.rows));
    let cell_width = terrain.width / terrain.columns as f32;
    let cell_depth = terrain.depth / terrain.rows as f32;

    Vec3::new(
        (left - right) / cell_width.max(0.001),
        2.0,
        (down - up) / cell_depth.max(0.001),
    )
    .normalize_or_zero()
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

fn set_material_texture<'pass>(
    render_pass: &mut wgpu::RenderPass<'pass>,
    fallback: &'pass wgpu::BindGroup,
    instances: &'pass InstanceBatch,
) {
    let bind_group = instances.material_texture.as_ref().unwrap_or(fallback);
    render_pass.set_bind_group(2, bind_group, &[]);
}

fn draw_prepared_mesh_batch<'pass>(
    render_pass: &mut wgpu::RenderPass<'pass>,
    draw: &'pass MeshHandleDraw,
) {
    render_pass.set_vertex_buffer(0, draw.vertex_buffer.slice(..));
    render_pass.set_vertex_buffer(1, draw.instances.buffer.slice(..));
    render_pass.set_index_buffer(draw.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    render_pass.draw_indexed(0..draw.index_count, 0, 0..draw.instances.count);
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_transform::{visibility_propagate_system, Visibility};

    #[test]
    fn builtin_unlit_material_maps_to_unlit_instance_mode() {
        let material = RenderMaterial::Builtin {
            shader: BuiltinShader::Unlit,
            material_type: MaterialType::Unlit,
            name: "ui".to_string(),
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic_factor: 0.0,
            roughness_factor: 0.5,
            emissive_color: [0.0, 0.0, 0.0],
            alpha_mode: AlphaMode::Opaque,
            albedo_texture: None,
            normal_texture: None,
            roughness_texture: None,
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
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic_factor: 0.0,
            roughness_factor: 0.5,
            emissive_color: [0.0, 0.0, 0.0],
            alpha_mode: AlphaMode::Opaque,
            albedo_texture: None,
            normal_texture: None,
            roughness_texture: None,
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

    #[test]
    fn material_batch_key_preserves_texture_identity() {
        let first = MaterialBatchKey::from_material(&RenderMaterial::Builtin {
            shader: BuiltinShader::Lit,
            material_type: MaterialType::Lit,
            name: "textured".to_string(),
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic_factor: 0.0,
            roughness_factor: 0.5,
            emissive_color: [0.0, 0.0, 0.0],
            alpha_mode: AlphaMode::Opaque,
            albedo_texture: Some("#image_0".to_string()),
            normal_texture: Some("#normal_0".to_string()),
            roughness_texture: Some("#roughness_0".to_string()),
        });
        let second = MaterialBatchKey::from_material(&RenderMaterial::Builtin {
            shader: BuiltinShader::Lit,
            material_type: MaterialType::Lit,
            name: "textured".to_string(),
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic_factor: 0.0,
            roughness_factor: 0.5,
            emissive_color: [0.0, 0.0, 0.0],
            alpha_mode: AlphaMode::Opaque,
            albedo_texture: Some("#image_1".to_string()),
            normal_texture: Some("#normal_1".to_string()),
            roughness_texture: Some("#roughness_1".to_string()),
        });

        assert_ne!(first, second);
        assert_eq!(first.albedo_texture.as_deref(), Some("#image_0"));
        let instance = SceneInstanceRaw::new(Mat4::IDENTITY, [1.0; 4], first);
        assert_eq!(instance.material[1], 1.0);
        assert_eq!(instance.texture_flags, [1.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn material_batch_key_resolves_named_scene_materials() {
        let mut library = SceneMaterialLibrary::new();
        library.register(
            "matte",
            RenderMaterial::Builtin {
                shader: BuiltinShader::Unlit,
                material_type: MaterialType::Unlit,
                name: "matte".to_string(),
                base_color: [0.5, 0.75, 1.0, 1.0],
                metallic_factor: 0.0,
                roughness_factor: 0.5,
                emissive_color: [0.0, 0.0, 0.0],
                alpha_mode: AlphaMode::Opaque,
                albedo_texture: None,
                normal_texture: None,
                roughness_texture: None,
            },
        );

        let unresolved =
            MaterialBatchKey::from_material(&RenderMaterial::Named("matte".to_string()));
        let resolved = MaterialBatchKey::from_material_with_library(
            &RenderMaterial::Named("matte".to_string()),
            Some(&library),
        );

        assert_eq!(unresolved.mode, MaterialMode::Lit);
        assert_eq!(resolved.mode, MaterialMode::Unlit);
        assert_eq!(
            resolved.identity,
            "builtin:Unlit:Unlit:matte:[0.5, 0.75, 1.0, 1.0]:0.0:0.5:[0.0, 0.0, 0.0]:Opaque:None:None:None"
        );
    }

    #[test]
    fn material_batch_key_preserves_alpha_mode() {
        let material = RenderMaterial::Builtin {
            shader: BuiltinShader::Unlit,
            material_type: MaterialType::Unlit,
            name: "leaf".to_string(),
            base_color: [1.0, 1.0, 1.0, 0.5],
            metallic_factor: 0.0,
            roughness_factor: 0.5,
            emissive_color: [0.0, 0.0, 0.0],
            alpha_mode: AlphaMode::Mask,
            albedo_texture: None,
            normal_texture: None,
            roughness_texture: None,
        };

        let key = MaterialBatchKey::from_material(&material);
        let instance = SceneInstanceRaw::new(Mat4::IDENTITY, [1.0; 4], key.clone());

        assert_eq!(key.alpha_mode, AlphaMode::Mask);
        assert!(key.identity.contains(":Mask:"));
        assert_eq!(instance.material[2], AlphaMode::Mask.shader_value());
        assert!(!AlphaMode::Mask.is_transparent_phase());
        assert!(AlphaMode::Blend.is_transparent_phase());
    }

    #[test]
    fn material_batch_key_encodes_material_factors() {
        let material = RenderMaterial::Builtin {
            shader: BuiltinShader::Lit,
            material_type: MaterialType::Lit,
            name: "brushed_metal".to_string(),
            base_color: [0.8, 0.8, 0.75, 1.0],
            metallic_factor: 0.9,
            roughness_factor: 0.25,
            emissive_color: [0.1, 0.2, 0.3],
            alpha_mode: AlphaMode::Opaque,
            albedo_texture: None,
            normal_texture: None,
            roughness_texture: None,
        };

        let key = MaterialBatchKey::from_material(&material);
        let instance = SceneInstanceRaw::new(Mat4::IDENTITY, [1.0; 4], key);

        assert_eq!(instance.material[3], 0.25);
        assert_eq!(instance.material_factors, [0.9, 0.1, 0.2, 0.3]);
    }

    #[test]
    fn transparent_batch_sort_depth_uses_farthest_instance() {
        let near = SceneInstanceRaw::new(
            Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0)),
            [1.0; 4],
            MaterialBatchKey::from_material(&RenderMaterial::default()),
        );
        let far = SceneInstanceRaw::new(
            Mat4::from_translation(Vec3::new(0.0, 0.0, 5.0)),
            [1.0; 4],
            MaterialBatchKey::from_material(&RenderMaterial::default()),
        );

        assert_eq!(instance_world_position(&near), Vec3::new(0.0, 0.0, 2.0));
        assert_eq!(batch_sort_depth(&[near, far], Vec3::ZERO), 25.0);
    }

    #[test]
    fn named_material_base_color_multiplies_instance_tint() {
        let mut library = SceneMaterialLibrary::new();
        library.register(
            "bronze",
            RenderMaterial::Builtin {
                shader: BuiltinShader::Unlit,
                material_type: MaterialType::Unlit,
                name: "bronze".to_string(),
                base_color: [0.5, 0.25, 0.75, 0.5],
                metallic_factor: 0.0,
                roughness_factor: 0.5,
                emissive_color: [0.0, 0.0, 0.0],
                alpha_mode: AlphaMode::Opaque,
                albedo_texture: None,
                normal_texture: None,
                roughness_texture: None,
            },
        );
        let material = RenderMaterial::Named("bronze".to_string());
        let base_color = material.base_color_with_library(Some(&library));
        let instance = SceneInstanceRaw::new(
            Mat4::IDENTITY,
            multiply_color([0.8, 0.6, 0.4, 0.5], base_color),
            MaterialBatchKey::from_material_with_library(&material, Some(&library)),
        );

        assert_eq!(instance.tint, [0.4, 0.15, 0.3, 0.25]);
    }

    #[test]
    fn material_filter_resolves_descriptor_asset_before_render_mesh_material() {
        let mut world = World::new();
        let handle = oxide_asset::Handle::new(12);
        let mut material_assets = MaterialDescriptorAssets::default();
        material_assets.assets.insert(
            handle,
            oxide_renderer::descriptor::MaterialDescriptor {
                name: "asset_material".to_string(),
                material_type: MaterialType::Lit,
                shader: oxide_renderer::descriptor::ShaderDescriptor::Builtin {
                    shader: "lit".to_string(),
                },
                fallback_shader: Some("lit".to_string()),
                base_color: [0.25, 0.5, 0.75, 1.0],
                metallic_factor: 0.6,
                roughness_factor: 0.35,
                emissive_color: [0.01, 0.02, 0.03],
                alpha_mode: AlphaMode::Blend,
                albedo_texture: Some("#image_0".to_string()),
                normal_texture: Some("#normal_0".to_string()),
                roughness_texture: Some("#roughness_0".to_string()),
            },
        );
        world.insert_resource(material_assets);
        let entity = world
            .spawn((
                MaterialFilter::new(handle),
                RenderMesh::new(
                    MeshPrimitive::Cube,
                    RenderMaterial::Builtin {
                        shader: BuiltinShader::Unlit,
                        material_type: MaterialType::Unlit,
                        name: "component_material".to_string(),
                        base_color: [1.0, 0.0, 0.0, 1.0],
                        metallic_factor: 0.0,
                        roughness_factor: 0.5,
                        emissive_color: [0.0, 0.0, 0.0],
                        alpha_mode: AlphaMode::Opaque,
                        albedo_texture: None,
                        normal_texture: None,
                        roughness_texture: None,
                    },
                ),
            ))
            .id();

        let material = resolve_entity_material(&world, entity, world.get::<RenderMesh>(entity));

        assert_eq!(
            material.base_color_with_library(None),
            [0.25, 0.5, 0.75, 1.0]
        );
        assert_eq!(material.albedo_texture_with_library(None), Some("#image_0"));
        assert_eq!(
            material.normal_texture_with_library(None),
            Some("#normal_0")
        );
        assert_eq!(
            material.roughness_texture_with_library(None),
            Some("#roughness_0")
        );
        assert_eq!(material.alpha_mode_with_library(None), AlphaMode::Blend);
        assert_eq!(
            material.factors_with_library(None),
            crate::MaterialFactors {
                metallic_factor: 0.6,
                roughness_factor: 0.35,
                emissive_color: [0.01, 0.02, 0.03],
            }
        );
        assert_eq!(
            MaterialBatchKey::from_material(&material).mode,
            MaterialMode::Lit
        );
    }

    #[test]
    fn collect_renderables_skips_hidden_entities() {
        let mut world = World::new();
        let visible = world
            .spawn(RenderMesh::new(
                MeshPrimitive::Cube,
                RenderMaterial::default(),
            ))
            .id();
        let hidden = world
            .spawn((
                Visibility::Hidden,
                RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
            ))
            .id();

        visibility_propagate_system(&mut world);
        let renderables = collect_renderables(&mut world, RenderLayers::default());

        assert!(renderables.iter().any(|(entity, _)| *entity == visible));
        assert!(!renderables.iter().any(|(entity, _)| *entity == hidden));
    }

    #[test]
    fn collect_renderables_filters_by_camera_layers() {
        let mut world = World::new();
        let world_entity = world
            .spawn(RenderMesh::new(
                MeshPrimitive::Cube,
                RenderMaterial::default(),
            ))
            .id();
        let weapon_entity = world
            .spawn((
                RenderLayers::layer(1),
                RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
            ))
            .id();

        let default_renderables = collect_renderables(&mut world, RenderLayers::default());
        assert!(default_renderables
            .iter()
            .any(|(entity, _)| *entity == world_entity));
        assert!(!default_renderables
            .iter()
            .any(|(entity, _)| *entity == weapon_entity));

        let weapon_renderables = collect_renderables(&mut world, RenderLayers::layer(1));
        assert!(!weapon_renderables
            .iter()
            .any(|(entity, _)| *entity == world_entity));
        assert!(weapon_renderables
            .iter()
            .any(|(entity, _)| *entity == weapon_entity));
    }

    #[test]
    fn mesh_filter_entities_use_handle_renderable_path() {
        let mut world = World::new();
        let mesh_handle = oxide_asset::Handle::new(99);
        let imported = world
            .spawn((
                MeshFilter::new(mesh_handle),
                RenderMesh::new(MeshPrimitive::Cube, RenderMaterial::default()),
            ))
            .id();

        let primitive_renderables = collect_renderables(&mut world, RenderLayers::default());
        let handle_renderables =
            collect_mesh_filter_renderables(&mut world, RenderLayers::default());

        assert!(!primitive_renderables
            .iter()
            .any(|(entity, _)| *entity == imported));
        assert_eq!(
            handle_renderables,
            vec![(imported, MeshFilter::new(mesh_handle))]
        );
    }

    #[test]
    fn active_camera_views_sort_by_order_and_skip_disabled() {
        let mut world = World::new();
        world.spawn((
            CameraComponent::default(),
            CameraRenderView::disabled().with_order(-10),
            RenderLayers::layer(4),
        ));
        world.spawn((
            CameraComponent::default(),
            CameraRenderView::new().with_order(10),
            RenderLayers::layer(1),
        ));
        world.spawn((
            CameraComponent::default(),
            CameraRenderView::new()
                .with_order(-1)
                .with_viewport(CameraViewport::new(0.5, 0.0, 0.5, 0.25))
                .with_clear_color([0.1, 0.2, 0.3, 1.0]),
            RenderLayers::layer(2),
        ));

        let cameras = active_camera_views(&mut world);
        assert_eq!(cameras.len(), 2);

        let camera = cameras[0];
        assert_eq!(camera.layers, RenderLayers::layer(2));
        assert_eq!(
            camera.viewport,
            Some(CameraViewport::new(0.5, 0.0, 0.5, 0.25))
        );
        assert_eq!(
            viewport_aspect_ratio(camera.viewport.unwrap(), 16.0 / 9.0),
            Some(32.0 / 9.0)
        );
        assert_eq!(
            camera.clear_color,
            Some(wgpu::Color {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            })
        );
        assert_eq!(cameras[1].layers, RenderLayers::layer(1));
    }
}
