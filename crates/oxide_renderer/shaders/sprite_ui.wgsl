struct VertexOutput {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs_main(
    @location(0) pos: vec3f,
    @location(1) color: vec3f
) -> VertexOutput {
    var out: VertexOutput;
    out.pos = vec4f(pos.xy, 0.0, 1.0);
    out.uv = color.xy;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let border = smoothstep(0.0, 0.06, in.uv.x) * smoothstep(0.0, 0.06, in.uv.y);
    return vec4f(vec3f(0.9, 0.9, 0.95) * border, 1.0);
}
