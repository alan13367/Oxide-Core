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
    // Sample the albedo texture (fallback is white)
    let texture_color = textureSample(albedo_texture, albedo_sampler, in.uv);

    // Apply a checker pattern overlay for visual interest
    let tint = vec3f(0.95, 0.82, 0.58);
    let checker = step(0.5, fract(in.uv.x * 10.0) + fract(in.uv.y * 10.0));
    let factor = mix(0.80, 1.0, checker);

    // Combine texture color with tint and checker
    let final_color = texture_color.rgb * tint * factor;
    return vec4f(final_color, 1.0);
}