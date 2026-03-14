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
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.pos = camera.view_proj * vec4f(in.position, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let band = floor(in.uv.x * 8.0) + floor(in.uv.y * 8.0);
    let checker = fract(band * 0.5);
    let color_a = vec3f(1.0, 0.0, 1.0);
    let color_b = vec3f(0.06, 0.06, 0.06);
    return vec4f(mix(color_a, color_b, checker), 1.0);
}
