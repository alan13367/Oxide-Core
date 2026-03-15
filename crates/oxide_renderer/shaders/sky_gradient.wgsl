struct VertexOutput {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);

    out.uv = vec2f(x, y);
    out.pos = vec4f(out.uv * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let horizon = vec3f(0.18, 0.24, 0.34);
    let zenith = vec3f(0.03, 0.06, 0.12);
    let t = clamp(in.uv.y, 0.0, 1.0);
    return vec4f(mix(horizon, zenith, t), 1.0);
}
