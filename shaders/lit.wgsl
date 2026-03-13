struct CameraUniform {
    view_proj: mat4x4<f32>,
    position: vec4f,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
}

struct VertexOutput {
    @builtin(position) pos: vec4f,
    @location(0) normal: vec3f,
    @location(1) world_pos: vec3f,
    @location(2) @interpolate(flat) instance_id: u32,
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
    out.instance_id = idx;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let light_dir_a = normalize(vec3f(0.9, 1.2, 0.8));
    let light_dir_b = normalize(vec3f(-0.6, 0.4, -1.0));
    let normal = normalize(in.normal);
    let view_dir = normalize(camera.position.xyz - in.world_pos);

    let diffuse_a = max(dot(normal, light_dir_a), 0.0);
    let diffuse_b = max(dot(normal, light_dir_b), 0.0);
    let ambient = 0.22;
    let rim = pow(1.0 - max(dot(normal, view_dir), 0.0), 2.0) * 0.20;

    let light = ambient + diffuse_a * 0.65 + diffuse_b * 0.35 + rim;
    let base_color = INSTANCE_COLORS[in.instance_id];

    let distance_fade = clamp(1.0 - length(in.world_pos) * 0.05, 0.35, 1.0);
    return vec4f(base_color * light * distance_fade, 1.0);
}