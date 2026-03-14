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
}

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance_index: u32) -> VertexOutput {
    var out: VertexOutput;

    let ring = f32(instance_index % 10u) * 0.65;
    let world_pos = in.position + vec3f(cos(ring) * 2.5, sin(ring * 1.7) * 0.8, -2.0 + sin(ring) * 2.0);

    out.pos = camera.view_proj * vec4f(world_pos, 1.0);
    out.normal = in.normal;
    out.world_pos = world_pos;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let light = normalize(vec3f(0.3, 1.0, 0.5));
    let diffuse = max(dot(normalize(in.normal), light), 0.0);
    let fresnel = pow(1.0 - max(dot(normalize(in.normal), normalize(camera.position.xyz - in.world_pos)), 0.0), 3.0);

    let base = vec3f(0.20, 0.70, 0.95);
    let glow = vec3f(0.95, 0.35, 0.80) * fresnel;
    return vec4f(base * (0.25 + diffuse * 0.75) + glow * 0.35, 1.0);
}
