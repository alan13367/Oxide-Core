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
    @location(0) uv: vec2f,
}

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let offset = vec3f(f32(instance_index) * 1.6, 0.0, 0.0);
    out.pos = camera.view_proj * vec4f(in.position + offset, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let tint = vec3f(0.95, 0.82, 0.58);
    let checker = step(0.5, fract(in.uv.x * 10.0) + fract(in.uv.y * 10.0));
    let factor = mix(0.80, 1.0, checker);
    return vec4f(tint * factor, 1.0);
}
