struct VertexOutput {
    @builtin(position) pos: vec4f,
    @location(0) color: vec4f,
}

@vertex
fn vs_main(@location(0) pos: vec3f, @location(1) color: vec3f) -> VertexOutput {
    var out: VertexOutput;
    out.pos = vec4f(pos, 1.0);
    out.color = vec4f(color, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    return in.color;
}