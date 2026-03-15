struct CameraUniform {
    view_proj: mat4x4<f32>,
    position: vec4f,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Material textures (Group 1)
@group(1) @binding(0)
var albedo_texture: texture_2d<f32>;

@group(1) @binding(1)
var albedo_sampler: sampler;

// Light uniform buffer (Group 2)
struct GpuDirectionalLight {
    direction: vec4f,
    color_intensity: vec4f,
}

struct GpuPointLight {
    position_radius: vec4f,
    color_intensity: vec4f,
    _padding: vec4f,
}

struct LightUniform {
    ambient_color_intensity: vec4f,
    directional_count: u32,
    point_count: u32,
    _padding: vec2u,
    directional_lights: array<GpuDirectionalLight, 4>,
}

struct PointLightStorage {
    lights: array<GpuPointLight>,
}

@group(2) @binding(0)
var<uniform> lights: LightUniform;

@group(2) @binding(1)
var<storage, read> point_light_storage: PointLightStorage;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
}

struct VertexOutput {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.pos = camera.view_proj * vec4f(in.position, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Magenta/black checker pattern to indicate fallback material
    let band = floor(in.uv.x * 8.0) + floor(in.uv.y * 8.0);
    let checker = fract(band * 0.5);
    let color_a = vec3f(1.0, 0.0, 1.0);
    let color_b = vec3f(0.06, 0.06, 0.06);
    return vec4f(mix(color_a, color_b, checker), 1.0);
}
