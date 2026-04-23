struct WindowUniform {
    width: f32,
    height: f32,
};

@group(0) @binding(0)
var<uniform> window: WindowUniform;

@group(0) @binding(1)
var t_atlas: texture_2d<f32>;

@group(0) @binding(2)
var s_atlas: sampler;

struct TextVertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct TextVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_text(in: TextVertexInput) -> TextVertexOutput {
    var out: TextVertexOutput;
    let clip_x = (in.position.x / window.width) * 2.0 - 1.0;
    let clip_y = 1.0 - (in.position.y / window.height) * 2.0;
    out.clip_position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_text(in: TextVertexOutput) -> @location(0) vec4<f32> {
    let coverage = textureSample(t_atlas, s_atlas, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
