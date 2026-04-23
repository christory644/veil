// Text rendering shader for Veil's glyph atlas pipeline.
//
// This shader handles TextVertex data (position, UV, foreground color) and
// samples the glyph atlas texture to produce alpha-blended text pixels.
//
// Pipeline: one bind group (@group(0)) with three bindings:
//   binding 0 — window uniform buffer (width/height for clip-space conversion)
//   binding 1 — glyph atlas texture (R8Unorm, grayscale, nearest-neighbor)
//   binding 2 — atlas sampler (filtering, ClampToEdge)

// Window dimensions in pixels, used to convert pixel-space positions to clip space.
struct WindowUniform {
    width: f32,
    height: f32,
};

@group(0) @binding(0)
var<uniform> window: WindowUniform;

// Glyph atlas: R8Unorm grayscale texture. Only the .r channel carries data;
// 0.0 = transparent, 1.0 = fully covered. textureSample() exposes it as f32.
@group(0) @binding(1)
var t_atlas: texture_2d<f32>;

// Nearest-neighbor sampler — pixel-perfect for glyph bitmaps.
@group(0) @binding(2)
var s_atlas: sampler;

// Vertex shader input: matches TextVertex layout (32 bytes, see text_vertex.rs).
struct TextVertexInput {
    @location(0) position: vec2<f32>, // pixel-space position
    @location(1) uv: vec2<f32>,       // normalized atlas UV [0, 1]
    @location(2) color: vec4<f32>,    // foreground RGBA color
};

// Vertex-to-fragment interpolants.
struct TextVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

// Convert pixel-space position to wgpu clip space:
//   x: [0, width]  → [-1, +1]
//   y: [0, height] → [+1, -1]  (Y is flipped: screen-top = clip +1)
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

// Sample atlas coverage and modulate foreground alpha.
//
// The atlas is R8Unorm: textureSample() returns vec4<f32> where only .r is
// non-zero. Using .r as alpha gives pixel-perfect glyph shapes: transparent
// where there is no ink, fully opaque where the glyph is solid. GPU
// ALPHA_BLENDING then composites this over whatever was drawn underneath.
@fragment
fn fs_text(in: TextVertexOutput) -> @location(0) vec4<f32> {
    let coverage = textureSample(t_atlas, s_atlas, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
