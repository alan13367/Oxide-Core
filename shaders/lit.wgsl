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
    position: vec4f,
    color_intensity: vec4f,
    radius: vec4f,
}

struct LightUniform {
    ambient_color_intensity: vec4f,
    directional_count: u32,
    point_count: u32,
    _padding: vec2u,
    directional_lights: array<GpuDirectionalLight, 4>,
    point_lights: array<GpuPointLight, 8>,
}

@group(2) @binding(0)
var<uniform> lights: LightUniform;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
}

struct VertexOutput {
    @builtin(position) pos: vec4f,
    @location(0) normal: vec3f,
    @location(1) world_pos: vec3f,
    @location(2) uv: vec2f,
    @location(3) @interpolate(flat) instance_id: u32,
}

const INSTANCE_POSITIONS: array<vec3f, 10> = array<vec3f, 10>(
    vec3f(-3.0,  0.0, -2.0),
    vec3f(-1.2, -0.3, -1.0),
    vec3f( 0.8,  0.2, -2.6),
    vec3f( 2.6, -0.2, -1.4),
    vec3f( 3.8,  0.3, -3.4),
    vec3f(-2.4, -1.0,  1.5),
    vec3f(-0.5,  1.3, -0.2),
    vec3f( 1.9,  1.0, -1.8),
    vec3f( 3.1,  1.4, -0.6),
    vec3f( 4.3,  0.9, -2.5),
);

const INSTANCE_COLORS: array<vec3f, 10> = array<vec3f, 10>(
    vec3f(0.92, 0.42, 0.36),
    vec3f(0.90, 0.64, 0.22),
    vec3f(0.67, 0.82, 0.29),
    vec3f(0.26, 0.81, 0.63),
    vec3f(0.29, 0.65, 0.93),
    vec3f(0.53, 0.52, 0.92),
    vec3f(0.82, 0.44, 0.92),
    vec3f(0.94, 0.40, 0.68),
    vec3f(0.50, 0.76, 0.95),
    vec3f(0.96, 0.73, 0.45),
);

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let idx = instance_index % 10u;
    let world_pos = in.position + INSTANCE_POSITIONS[idx];

    out.pos = camera.view_proj * vec4f(world_pos, 1.0);
    out.normal = in.normal;
    out.world_pos = world_pos;
    out.uv = in.uv;
    out.instance_id = idx;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let normal = normalize(in.normal);
    let view_dir = normalize(camera.position.xyz - in.world_pos);

    // Start with ambient light
    var total_light = lights.ambient_color_intensity.rgb * lights.ambient_color_intensity.a;

    // Process directional lights
    for (var i = 0u; i < lights.directional_count; i++) {
        let light = lights.directional_lights[i];
        let light_dir = normalize(light.direction.xyz);
        let diffuse = max(dot(normal, light_dir), 0.0);
        total_light += light.color_intensity.rgb * light.color_intensity.a * diffuse;
    }

    // Process point lights
    for (var i = 0u; i < lights.point_count; i++) {
        let light = lights.point_lights[i];
        let light_dir = light.position.xyz - in.world_pos;
        let distance = length(light_dir);
        let light_dir_norm = normalize(light_dir);

        // Distance attenuation
        let radius = light.radius.x;
        let attenuation = 1.0 - smoothstep(0.0, radius, distance);

        let diffuse = max(dot(normal, light_dir_norm), 0.0);
        total_light += light.color_intensity.rgb * light.color_intensity.a * diffuse * attenuation;
    }

    // Add rim lighting
    let rim = pow(1.0 - max(dot(normal, view_dir), 0.0), 2.0) * 0.20;
    total_light += vec3f(rim);

    // Sample the albedo texture (fallback is white, so this works for untextured materials too)
    let texture_color = textureSample(albedo_texture, albedo_sampler, in.uv).rgb;

    // Blend instance color with texture color
    let base_color = INSTANCE_COLORS[in.instance_id] * texture_color;

    let distance_fade = clamp(1.0 - length(in.world_pos) * 0.05, 0.35, 1.0);
    return vec4f(base_color * total_light * distance_fade, 1.0);
}