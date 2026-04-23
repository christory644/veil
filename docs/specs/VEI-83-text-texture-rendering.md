# VEI-83: Add Text Texture Rendering to wgpu Pipeline (Glyph Atlas)

## Context

After VEI-82 (completed), `build_frame_geometry()` in `crates/veil/src/frame.rs` populates
`FrameGeometry.text_vertices` and `FrameGeometry.text_indices` with textured quad data for
every visible character. The CPU-side geometry exists, but nothing renders it.

`crates/veil/src/renderer.rs` only draws `FrameGeometry.vertices` (solid-color quads) in
`render_terminal_pass()` (lines 284–330). It never touches `text_vertices` or `text_indices`.
The shader at `crates/veil/src/shader.wgsl` has no texture sampler: the fragment function
returns `in.color` directly (line 33) with no UV lookup. There is no GPU-side glyph atlas
texture, no atlas bind group, and no second pipeline for textured quads.

This task wires the CPU glyph atlas into the GPU render path so terminal text becomes visible
on screen when running `cargo run`.

### What exists

**`Renderer`** (`crates/veil/src/renderer.rs`):
- Fields: `surface`, `device`, `queue`, `config`, `render_pipeline`, `window_uniform_buffer`,
  `window_bind_group`, `size`, `egui` (line 27–38).
- `WindowUniform` struct — 8 bytes: `width: f32`, `height: f32` (lines 16–21).
- `create_window_bind_group()` — creates a bind group layout with a single uniform buffer at
  `binding(0)`, visible to `VERTEX` stage (lines 87–112).
- `create_render_pipeline()` — builds a `RenderPipeline` using `Vertex::buffer_layout()`,
  `vs_main` / `fs_main` entry points, `ALPHA_BLENDING`, `TriangleList` topology (lines 41–84).
- `render_terminal_pass()` — creates vertex/index buffers on the fly from
  `frame_geometry.vertices` / `frame_geometry.indices`, draws with `render_pipeline` and
  `window_bind_group`. Never accesses `text_vertices` or `text_indices` (lines 284–330).
- `render()` — calls `render_terminal_pass()` then optionally calls `egui.render()` (lines
  235–281).
- `resize()` — reconfigures surface and updates uniform buffer (lines 209–221).

**`shader.wgsl`** (`crates/veil/src/shader.wgsl`):
- `VertexInput`: `@location(0) position: vec2<f32>`, `@location(1) color: vec4<f32>`.
- `VertexOutput`: `@builtin(position) clip_position`, `@location(0) color`.
- `vs_main`: converts pixel coords to clip space (lines 20–29).
- `fs_main`: returns `in.color` — no texture sampling, no UV coordinate (lines 31–34).
- One bind group, `@group(0) @binding(0)` — the `WindowUniform` buffer (lines 6–7).

**`TextVertex`** (`crates/veil/src/font/text_vertex.rs`):
- 32 bytes: `position: [f32; 2]` at offset 0, `uv: [f32; 2]` at offset 8, `color: [f32; 4]`
  at offset 16 (lines 26–34).
- `TextVertex::buffer_layout()` — wgpu layout with 3 attributes: position (`Float32x2` @ 0),
  uv (`Float32x2` @ 8), color (`Float32x4` @ 16) (lines 43–69).

**`FrameGeometry`** (`crates/veil/src/frame.rs`):
- `text_vertices: Vec<TextVertex>` — populated by `build_frame_geometry()` (line 48).
- `text_indices: Vec<u16>` — populated by `build_frame_geometry()` (line 51).
- `#[allow(dead_code)]` annotations on both text fields are already removed (completed in VEI-82).

**`GlyphAtlas`** (`crates/veil/src/font/atlas.rs`):
- `bitmap(): &[u8]` — raw grayscale pixel data, 1 byte per pixel (line 264).
- `dimensions() -> (u32, u32)` — current width/height of the atlas (line 269).
- `is_dirty() -> bool` — true if the atlas has changed since last `mark_clean()` (line 251).
- `mark_clean(&mut self)` — call after uploading to GPU (line 256).
- Atlas is initialized as 512×512 in `FontPipeline::new()` and grows as glyphs are added
  (line 25 in `font_pipeline.rs`).

**`FontPipeline`** (`crates/veil/src/font_pipeline.rs`):
- `atlas() -> &GlyphAtlas` — immutable access for GPU upload (line 79).
- `atlas_mut() -> &mut GlyphAtlas` — mutable access for `mark_clean()` (line 85).

**`VeilApp`** (`crates/veil/src/main.rs`):
- Owns `renderer: Option<Renderer>` and `font_pipeline: Option<FontPipeline>` (lines 52, 82).
- `handle_redraw()` calls `build_frame_geometry()` and passes the result to
  `renderer.render()` (lines 428–453).

### What's missing

1. **No GPU atlas texture** — No `wgpu::Texture`, `wgpu::TextureView`, or `wgpu::Sampler` for
   the glyph atlas exists anywhere in the codebase.
2. **No text bind group layout or bind group** — `create_window_bind_group()` only binds a
   uniform buffer. No layout exists for (uniform + texture + sampler).
3. **No text render pipeline** — `create_render_pipeline()` uses `Vertex::buffer_layout()` with
   a 2-attribute layout (position, color). No pipeline exists that consumes
   `TextVertex::buffer_layout()` (position, uv, color) or samples from a texture.
4. **`render_terminal_pass()` ignores text data** — `frame_geometry.text_vertices` and
   `frame_geometry.text_indices` are accessed nowhere in the renderer.
5. **Shader has no texture sampling** — `shader.wgsl` has no `@group(?) @binding(?)
   var<...> t_atlas` or `var<...> s_atlas` declarations, no `textureSample()` call, and no UV
   attribute in its output struct.
6. **No atlas upload logic** — Nothing calls `queue.write_texture()` or creates a texture from
   `atlas.bitmap()`. The `Renderer` struct holds no atlas-related fields.

## Implementation Units

### Unit 1: WGSL text shader

**Location:** New file `crates/veil/src/text_shader.wgsl`

**What it does:**

Adds a second WGSL shader specifically for the text (textured quad) pipeline. This shader
handles `TextVertex` data: pixel-space position, UV coordinates, and foreground RGBA color. The
vertex stage converts pixel coordinates to clip space (same math as `shader.wgsl` `vs_main`).
The fragment stage samples the glyph atlas texture at the given UV and multiplies the sampled
grayscale coverage by the vertex foreground color to produce the final pixel.

The atlas is a grayscale R8Unorm texture, so `textureSample()` returns `vec4<f32>` where only
the `.r` channel contains the coverage value. The fragment output is:
`vec4<f32>(in.color.rgb, in.color.a * sample.r)` — this gives correct alpha-blended text where
the glyph shape is defined by the atlas coverage and the color comes from the terminal foreground.

**Shader structure:**

```wgsl
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
```

**Key details:**
- `@group(0)` for all three bindings: uniform buffer, texture, sampler. The bind group layout
  must declare all three at the same group index. This matches the existing convention in the
  solid-color pipeline where the window uniform is `@group(0) @binding(0)`.
- The atlas texture is `texture_2d<f32>` not `texture_2d<u32>` because wgpu exposes R8Unorm
  as a float-format sample.
- The sampler uses nearest-neighbor filtering (pixel-perfect for glyphs). The sampler binding
  type in the bind group layout is `wgpu::SamplerBindingType::Filtering` with
  `wgpu::FilterMode::Nearest`.
- Fragment output color multiplies foreground RGB by the coverage alpha, enabling GPU alpha
  blending to composite text on top of the background quads.

**Test strategy:**

The WGSL source is validated at startup by `device.create_shader_module()`. If the shader is
syntactically or semantically invalid, wgpu panics with a clear error message. No separate
unit test is needed for the WGSL source itself, but:

- **Compile-time validation**: `cargo build` will catch WGSL errors via `include_str!()` and
  the wgpu shader module creation in `Renderer::new()`.
- **Shader constant test** (in `renderer.rs`): Add a test asserting that the text shader
  source string is non-empty (simple smoke test ensuring the file is included).

---

### Unit 2: Atlas texture creation and GPU upload in `Renderer`

**Location:** `crates/veil/src/renderer.rs`

**What it does:**

Adds a glyph atlas `wgpu::Texture` (R8Unorm format), a `TextureView`, and a `Sampler` to
`Renderer`. Adds a method `upload_atlas()` that reads the current atlas bitmap from
`GlyphAtlas::bitmap()` and writes it to the GPU texture using `queue.write_texture()`. Called
once at renderer creation with an empty atlas, then called again each frame when
`GlyphAtlas::is_dirty()` is true.

**New fields added to `Renderer`:**

```rust
atlas_texture: wgpu::Texture,
atlas_texture_view: wgpu::TextureView,
atlas_sampler: wgpu::Sampler,
```

**New helper function:**

```rust
fn create_atlas_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Sampler)
```

This creates:
1. A `wgpu::Texture` with:
   - `dimension: wgpu::TextureDimension::D2`
   - `format: wgpu::TextureFormat::R8Unorm`
   - `size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 }`
   - `mip_level_count: 1`
   - `sample_count: 1`
   - `usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST`
   - `label: Some("glyph atlas texture")`
2. A `wgpu::TextureView` with `TextureViewDescriptor::default()`.
3. A `wgpu::Sampler` with:
   - `address_mode_*: wgpu::AddressMode::ClampToEdge`
   - `mag_filter: wgpu::FilterMode::Nearest`
   - `min_filter: wgpu::FilterMode::Nearest`
   - `mipmap_filter: wgpu::FilterMode::Nearest`
   - `label: Some("glyph atlas sampler")`

**Upload method:**

```rust
fn upload_atlas(
    &self,
    queue: &wgpu::Queue,
    atlas: &crate::font::atlas::GlyphAtlas,
)
```

Calls `queue.write_texture()` to copy `atlas.bitmap()` into `self.atlas_texture`:

```rust
queue.write_texture(
    wgpu::TexelCopyTextureInfo {
        texture: &self.atlas_texture,
        mip_level: 0,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
    },
    atlas.bitmap(),
    wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(atlas.dimensions().0),
        rows_per_image: Some(atlas.dimensions().1),
    },
    wgpu::Extent3d {
        width: atlas.dimensions().0,
        height: atlas.dimensions().1,
        depth_or_array_layers: 1,
    },
);
```

**Important:** If the atlas grows (doubles its dimensions after `insert()` is called), the
texture must be recreated because the texture dimensions are fixed at creation. The simplest
correct strategy is to recreate the atlas texture and views whenever the atlas dimensions change.
Store the last-known atlas size in `Renderer` and compare each frame.

```rust
atlas_size: (u32, u32),
```

In the per-frame upload logic (in `render()`, before `render_terminal_pass()`):

```rust
if let Some(fp) = font_pipeline {
    let current_size = fp.atlas().dimensions();
    if current_size != self.atlas_size {
        // Atlas grew — recreate texture at new dimensions.
        let (tex, view, sampler) = create_atlas_texture(&self.device, current_size.0, current_size.1);
        self.atlas_texture = tex;
        self.atlas_texture_view = view;
        self.atlas_sampler = sampler;
        self.atlas_size = current_size;
        // Recreate text bind group since the texture view changed.
        self.text_bind_group = create_text_bind_group(
            &self.device,
            &self.text_bind_group_layout,
            &self.window_uniform_buffer,
            &self.atlas_texture_view,
            &self.atlas_sampler,
        );
    }
    if fp.atlas().is_dirty() {
        self.upload_atlas(&self.queue, fp.atlas());
        fp.atlas_mut().mark_clean();
    }
}
```

**Test strategy:**

The atlas texture operates on a real GPU device, so tests require a wgpu context (not feasible
in pure unit tests without GPU). Tests are structured as:

- **`atlas_texture_format_is_r8unorm`** (unit test, non-GPU): Test the computed parameters
  (format enum value, usage flags) are correct before passing to `create_atlas_texture()` —
  assert the format constant equals `wgpu::TextureFormat::R8Unorm`.
- **`upload_atlas_bytes_per_row_equals_width`** (unit test, non-GPU): For a 512×512 atlas,
  assert `bytes_per_row = 512` is the correct layout for R8Unorm (1 byte per pixel × width).
- **Integration:** Covered by `cargo run` and the acceptance criteria (text visible on screen).
  GPU-dependent creation logic is tested implicitly by the application starting without panic.

---

### Unit 3: Text bind group layout and bind group

**Location:** `crates/veil/src/renderer.rs`

**What it does:**

Creates a `wgpu::BindGroupLayout` for the text pipeline, containing three entries:

| Binding | Type | Visibility | Resource |
|---------|------|------------|----------|
| 0 | Uniform buffer | `VERTEX` | `window_uniform_buffer` |
| 1 | Texture (filtering) | `FRAGMENT` | `atlas_texture_view` |
| 2 | Sampler (filtering) | `FRAGMENT` | `atlas_sampler` |

Creates a corresponding `wgpu::BindGroup` referencing the live atlas texture view and sampler.
Stores both in `Renderer` as `text_bind_group_layout` and `text_bind_group`.

The bind group layout must be created before the text pipeline layout, and the text pipeline
layout references only this one bind group layout (binding group 0).

**New helper function:**

```rust
fn create_text_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("text bind group layout"),
        entries: &[
            // binding 0: window uniform buffer (VERTEX)
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // binding 1: glyph atlas texture (FRAGMENT)
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            // binding 2: atlas sampler (FRAGMENT)
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}
```

```rust
fn create_text_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    atlas_view: &wgpu::TextureView,
    atlas_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("text bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(atlas_sampler),
            },
        ],
    })
}
```

**New fields added to `Renderer`:**

```rust
text_bind_group_layout: wgpu::BindGroupLayout,
text_bind_group: wgpu::BindGroup,
```

**Test strategy:**

- **`text_bind_group_layout_has_three_entries`** (unit test, non-GPU): The bind group layout
  descriptor slice has exactly 3 entries. Test the constant slice length used in
  `create_text_bind_group_layout()`.
- **`text_bind_group_entry_bindings_are_0_1_2`** (unit test, non-GPU): Assert the binding
  indices are 0, 1, 2 in order.
- **Compile-time validation**: If the bind group layout does not match the shader's
  `@group(0) @binding(1/2)` declarations, wgpu emits a validation error on pipeline creation.
  This surfaces as a panic in `Renderer::new()` during `cargo run`.

---

### Unit 4: Text render pipeline

**Location:** `crates/veil/src/renderer.rs`

**What it does:**

Creates a second `wgpu::RenderPipeline` using the text shader, the text bind group layout,
and `TextVertex::buffer_layout()`. Stores it as `text_render_pipeline` on `Renderer`. The
pipeline layout references only one bind group layout (the text bind group layout from Unit 3).

**New helper function:**

```rust
fn create_text_render_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline
```

This function mirrors `create_render_pipeline()` (lines 41–84 of `renderer.rs`) but:
- Uses `TextVertex::buffer_layout()` instead of `Vertex::buffer_layout()`.
- Uses `vs_text` / `fs_text` entry points from `text_shader.wgsl`.
- Uses `wgpu::BlendState::ALPHA_BLENDING` so text alpha-composites over solid backgrounds.
- Topology remains `wgpu::PrimitiveTopology::TriangleList`, `cull_mode: None`.

**New field added to `Renderer`:**

```rust
text_render_pipeline: wgpu::RenderPipeline,
```

The text shader module is loaded separately from the solid-color shader:

```rust
let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("veil text shader"),
    source: wgpu::ShaderSource::Wgsl(include_str!("text_shader.wgsl").into()),
});
```

**Test strategy:**

- **`text_pipeline_uses_text_vertex_layout`** (unit test, non-GPU): Assert
  `TextVertex::buffer_layout().array_stride == 32` (not 24 like `Vertex`). This confirms the
  correct vertex type is used.
- **`text_pipeline_uses_alpha_blending`** (unit test, non-GPU): Assert the blend state used in
  the pipeline descriptor is `wgpu::BlendState::ALPHA_BLENDING` (compare constants).
- **Pipeline validation**: If the vertex layout does not match the shader's input attributes,
  wgpu emits a validation error on pipeline creation, surfacing as a panic at startup.

---

### Unit 5: Text render pass in `render_terminal_pass()` (or a new `render_text_pass()`)

**Location:** `crates/veil/src/renderer.rs`

**What it does:**

Adds a second draw call for text geometry inside `render_terminal_pass()` (or extracted into a
new `render_text_pass()` method called immediately after). This draw call:

1. Checks if `frame_geometry.text_vertices` is non-empty.
2. Creates vertex and index buffers from `text_vertices` / `text_indices` via
   `device.create_buffer_init()`.
3. Switches pipeline to `text_render_pipeline`.
4. Binds `text_bind_group` at slot 0.
5. Calls `draw_indexed()` with the text index count.

The text draw happens in the **same render pass** as the solid-color quads, after the solid-
color draw, so text is composited on top of background quads without requiring a separate
`begin_render_pass()`. This avoids a `LoadOp::Load` vs `LoadOp::Clear` issue: the first
attachment op clears to `frame_geometry.clear_color`, solid quads draw on top, then text
draws on top of that.

**Updated `render_terminal_pass()` structure:**

```rust
fn render_terminal_pass(
    &self,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    frame_geometry: &FrameGeometry,
) {
    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        // ... same as current (lines 290–305), LoadOp::Clear with clear_color
    });

    // Draw 1: solid-color quads (backgrounds, cursor, dividers, focus border)
    render_pass.set_pipeline(&self.render_pipeline);
    render_pass.set_bind_group(0, &self.window_bind_group, &[]);

    if !frame_geometry.vertices.is_empty() {
        let vertex_buffer = /* ... same as current (lines 311–315) */;
        let index_buffer = /* ... same as current (lines 317–321) */;
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..index_count, 0, 0..1);
    }

    // Draw 2: textured glyph quads (text on top of backgrounds)
    if !frame_geometry.text_vertices.is_empty() {
        let text_vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("veil text vertex buffer"),
            contents: bytemuck::cast_slice(&frame_geometry.text_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let text_index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("veil text index buffer"),
            contents: bytemuck::cast_slice(&frame_geometry.text_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        render_pass.set_pipeline(&self.text_render_pipeline);
        render_pass.set_bind_group(0, &self.text_bind_group, &[]);
        render_pass.set_vertex_buffer(0, text_vertex_buffer.slice(..));
        render_pass.set_index_buffer(text_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        let text_index_count = frame_geometry.text_indices.len() as u32;
        render_pass.draw_indexed(0..text_index_count, 0, 0..1);
    }
}
```

**Test strategy:**

- **`render_pass_draws_text_after_background`** (unit test, non-GPU): Assert that
  `text_vertices` is drawn in the same pass as `vertices` by verifying the control-flow
  contract: if `text_vertices` is empty, no text buffer is created; if non-empty, it is drawn
  after `vertices`. Test this by confirming `FrameGeometry` fields used in the draw calls.
- **`text_index_count_cast_is_safe_for_u16_max`** (unit test, non-GPU): Assert that casting
  `text_indices.len() as u32` is correct — `u16::MAX` indices fit comfortably in `u32`.
- **Integration test** (non-GPU): The existing `render_terminal_pass` test structure in
  `renderer.rs` only tests `WindowUniform` size and `Vertex` size (lines 334–346). Extend with:
  - **`text_vertex_buffer_stride_matches_text_vertex_size`**: Assert
    `std::mem::size_of::<TextVertex>() == 32` (mirrors the existing `vertex_buffer_stride_matches_vertex_size` test pattern on line 343).
  - **`frame_geometry_with_empty_text_produces_no_draw`**: Confirms the guard
    `if !frame_geometry.text_vertices.is_empty()` prevents buffer allocation when there are no
    text quads.

---

### Unit 6: Per-frame atlas upload wired into `render()`

**Location:** `crates/veil/src/renderer.rs`, `crates/veil/src/main.rs`

**What it does:**

Connects the CPU-side atlas (owned by `FontPipeline`) to the GPU-side atlas texture (owned by
`Renderer`) on a per-frame basis. The `render()` method needs access to `FontPipeline` (or at
least `GlyphAtlas`) to check `is_dirty()` and call `upload_atlas()` + `mark_clean()`.

**Option A (preferred):** Pass `Option<&mut FontPipeline>` to `render()`:

```rust
pub fn render(
    &mut self,
    frame_geometry: &FrameGeometry,
    font_pipeline: Option<&mut crate::font_pipeline::FontPipeline>,
    egui_full_output: Option<egui::FullOutput>,
) -> anyhow::Result<()>
```

Before calling `render_terminal_pass()`:

```rust
if let Some(fp) = font_pipeline {
    let current_size = fp.atlas().dimensions();
    if current_size != self.atlas_size {
        let (tex, view, sampler) = create_atlas_texture(&self.device, current_size.0, current_size.1);
        self.atlas_texture = tex;
        self.atlas_texture_view = view;
        self.atlas_sampler = sampler;
        self.atlas_size = current_size;
        self.text_bind_group = create_text_bind_group(
            &self.device,
            &self.text_bind_group_layout,
            &self.window_uniform_buffer,
            &self.atlas_texture_view,
            &self.atlas_sampler,
        );
    }
    if fp.atlas().is_dirty() {
        self.upload_atlas(&self.queue, fp.atlas());
        fp.atlas_mut().mark_clean();
    }
}
```

**Call site in `main.rs` `handle_redraw()`:**

```rust
if let Some(renderer) = &mut self.renderer {
    match renderer.render(&frame_geometry, self.font_pipeline.as_mut(), egui_output) {
        Ok(()) => {}
        Err(e) => {
            tracing::error!("render error: {e}");
            event_loop.exit();
        }
    }
}
```

**Borrow checker concern:** In `handle_redraw()`, `self.renderer` and `self.font_pipeline` are
both fields of `VeilApp`. Rust allows this since they are distinct fields. The borrow of
`self.renderer` (via `&mut self.renderer`) is separate from the borrow of `self.font_pipeline`
(via `self.font_pipeline.as_mut()`). No `Option::as_mut()` problem exists because Rust's field
borrow splitting handles this correctly.

**Test strategy:**

- **`atlas_upload_called_when_dirty`** (unit test, non-GPU): Construct a mock `GlyphAtlas`
  with `is_dirty() = true` and verify that `mark_clean()` would be called after upload. Since
  `GlyphAtlas` is not `mockall`-compatible without effort, test this by creating a real
  `GlyphAtlas`, inserting a glyph (which sets dirty), verifying `is_dirty()` is true, calling
  `mark_clean()`, then verifying `is_dirty()` is false. This validates the contract, not the
  GPU call.
- **`atlas_not_reuploaded_when_clean`** (unit test, non-GPU): After `mark_clean()`, `is_dirty()`
  returns false — no upload should be triggered. Verified by the same pattern above.
- **`atlas_texture_recreated_on_size_change`** (non-GPU): Create `GlyphAtlas::new(32, 32)`,
  insert enough glyphs to trigger growth (see `atlas_grows_when_full` test in `atlas.rs`),
  confirm `dimensions()` returns a value larger than `(32, 32)`. The renderer logic should
  detect this and recreate the texture.
- **Integration**: Covered by `cargo run`. When the application starts and renders a frame, the
  atlas is uploaded once (initial state) and subsequent atlas mutations trigger re-upload.

---

### Unit 7: Remove `#[allow(dead_code)]` from atlas-related items

**Location:** `crates/veil/src/font_pipeline.rs`, `crates/veil/src/font/atlas.rs`,
`crates/veil/src/font/text_vertex.rs`

**What it does:**

Removes dead-code annotations from items that are now used by the GPU upload and text render
path. These annotations were placed speculatively in earlier tasks.

**Changes:**

1. `crates/veil/src/font_pipeline.rs`:
   - Line 78: Remove `#[allow(dead_code)]` from `atlas()`.
   - Line 84: Remove `#[allow(dead_code)]` from `atlas_mut()`.

2. `crates/veil/src/font/atlas.rs`:
   - Line 251: Remove `#[allow(dead_code)]` from `is_dirty()`.
   - Line 256: Remove `#[allow(dead_code)]` from `mark_clean()`.
   - Line 263: Remove `#[allow(dead_code)]` from `bitmap()`.
   - Line 269: Remove `#[allow(dead_code)]` from `dimensions()`.

3. `crates/veil/src/font/text_vertex.rs`:
   - Line 13: Remove `#[allow(dead_code)]` from `UV_OFFSET`.
   - Line 18: Remove `#[allow(dead_code)]` from `COLOR_OFFSET`.
   - Line 43: Remove `#[allow(dead_code)]` from `buffer_layout()`.

**Test strategy:**

- **Compile-time**: `cargo clippy --all-targets --all-features -- -D warnings` passes without
  dead-code warnings for any of the removed items.
- **No logic changes**: All existing tests pass unchanged.

## Acceptance Criteria

1. Terminal text is visible on screen when running `cargo run` — characters appear in the grid
   at the expected positions with ANSI foreground colors.
2. Glyph atlas data (`GlyphAtlas::bitmap()`) is uploaded to a `wgpu::Texture` (R8Unorm format)
   and sampled in the fragment shader via `textureSample()`.
3. Text quads from `FrameGeometry.text_vertices` and `FrameGeometry.text_indices` are drawn in
   the same render pass as solid-color quads, after the solid-color draw call.
4. Background quads (cell backgrounds, cursor, dividers, focus border) continue to render
   correctly underneath text.
5. Text foreground colors come from `CellData.fg_color` (with fallback to
   `RenderColors.foreground`) — already generated correctly by `build_frame_geometry()`.
6. Atlas is re-uploaded to the GPU when `GlyphAtlas::is_dirty()` is true; `mark_clean()` is
   called after upload to prevent redundant uploads.
7. If the atlas grows (dimensions double), the `wgpu::Texture` is recreated at the new size
   and the text bind group is updated to reference the new texture view.
8. All existing tests pass: `cargo test --workspace`.
9. `cargo clippy --all-targets --all-features -- -D warnings` passes.
10. `#[allow(dead_code)]` annotations removed from `atlas()`, `atlas_mut()`, `is_dirty()`,
    `mark_clean()`, `bitmap()`, `dimensions()`, `UV_OFFSET`, `COLOR_OFFSET`, and
    `buffer_layout()` in the font/atlas modules.

## Dependencies

No new external crate dependencies are required. All needed crates are already declared in
`crates/veil/Cargo.toml`:

- `wgpu` (workspace) — `wgpu::Texture`, `wgpu::TextureView`, `wgpu::Sampler`, `wgpu::TextureFormat::R8Unorm`, `queue.write_texture()`.
- `bytemuck` (workspace) — `bytemuck::cast_slice()` for text vertex/index buffer creation.
- `swash`, `rustybuzz` (workspace) — already used by the font pipeline; no new usage.

The atlas texture format `R8Unorm` is universally supported across Metal, Vulkan, DX12, and
OpenGL backends. No backend-specific workarounds are needed.

**Test font fixture** at `crates/veil/test_fixtures/test_font.ttf` is required for integration
tests and for the runtime font pipeline initialization in `VeilApp::resumed()`. Already present.
