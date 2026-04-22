# VEI-8: Font Rendering -- Core Glyph Pipeline

## Context

Veil's wgpu renderer (VEI-7) currently draws solid-color quads for cell backgrounds, cursors, dividers, and focus borders. There is no text on screen. This task implements the core font pipeline that turns terminal cell content into visible glyphs: load a font file, shape text runs through rustybuzz, rasterize individual glyphs with swash, pack them into a CPU-side glyph atlas, upload the atlas as a GPU texture, and render textured quads for each glyph cell.

This is the foundation that subsequent tasks build on:
- **VEI-42** (platform font discovery) adds CoreText/fontconfig/DirectWrite lookup so users don't need to specify a font path
- **VEI-43** (fallback chain) adds multi-font fallback for missing glyphs, Nerd Fonts, and emoji
- **VEI-44** (ligature rendering) adds multi-cell ligature cluster support

For this task, the font pipeline works with a single font loaded from an explicit file path (or a bundled fallback). No platform font discovery, no fallback chain, no ligature support. The goal is a working end-to-end path from font file to pixels on the GPU.

### What already exists

- **`crates/veil/src/renderer.rs`** -- `Renderer` struct owning wgpu device, queue, surface, a single render pipeline for solid-color quads, and a `WindowUniform` bind group
- **`crates/veil/src/vertex.rs`** -- `Vertex` type (position + color, 24 bytes), `quad_vertices`, `quad_indices`, `vertex_base` helpers
- **`crates/veil/src/quad_builder.rs`** -- `build_cell_background_quads`, `build_cursor_quad`, `build_divider_quads`, `build_focus_border`
- **`crates/veil/src/frame.rs`** -- `FrameGeometry` struct, `build_frame_geometry` composing all quads per frame
- **`crates/veil/src/shader.wgsl`** -- WGSL shader converting pixel-coordinate vertices to clip space; fragment shader returns solid vertex color
- **`crates/veil/src/main.rs`** -- winit `ApplicationHandler` wiring `AppState` + `FocusManager` into the render loop

### Key design decisions

**rustybuzz for shaping, swash for rasterization.** Per the system design doc and AGENTS.md tech stack table. rustybuzz is a complete HarfBuzz port to Rust -- it takes a font and a text run, and returns positioned glyph IDs. swash reads font files (via `FontRef`) and rasterizes individual glyph IDs to grayscale bitmaps at a given size and DPI. Together they form a complete pipeline without needing cosmic-text or platform font APIs.

**Separate text pipeline, not modifying the existing solid-color pipeline.** Text rendering requires texture sampling (the glyph atlas). The existing pipeline uses a simple position+color vertex layout with no texture coordinates. Rather than modifying the solid-color pipeline (which works well for backgrounds, cursors, dividers, borders), we add a second render pipeline for textured quads. Both pipelines run in the same render pass, ordered: backgrounds first, then text on top.

**CPU-side glyph atlas with lazy GPU upload.** The atlas is built and managed on the CPU as an `Vec<u8>` grayscale bitmap. When new glyphs are rasterized and added, the atlas is re-uploaded to the GPU texture. This is simpler than incremental GPU texture updates and adequate for the glyph counts in a terminal (a few hundred unique glyphs at most). The atlas uses a simple shelf-packing algorithm (row-by-row) for glyph placement.

**Monospace grid assumption.** Every glyph occupies exactly one cell width. The cell width and height are derived from the font metrics (advance width of a representative character like 'M', and the ascent + descent + line gap). This simplification is correct for single-cell characters and will be extended for wide characters and ligatures in follow-up tasks.

**Grayscale alpha rendering, not subpixel.** Glyph bitmaps are single-channel (alpha/coverage). The fragment shader multiplies the atlas sample by a foreground color uniform or per-vertex color. Subpixel rendering is platform-dependent and significantly more complex -- it can be added later as an optimization.

**Font configuration is a file path + size for now.** No config file parsing, no Ghostty config import. The font pipeline accepts a `FontConfig` struct with a path (or None for a bundled default) and a size in points. Platform font discovery (VEI-42) and config integration come later.

## Implementation Units

### Unit 1: Font loading and metrics extraction

Load a font file from disk using swash's `FontRef`, extract monospace grid metrics (cell width, cell height, baseline offset), and expose a `FontData` struct that holds the owned font data and provides metrics.

**File:** `crates/veil/src/font/mod.rs` (new module), `crates/veil/src/font/loader.rs`

**Types:**

```rust
/// User-facing font configuration.
pub struct FontConfig {
    /// Path to the font file. None means use the bundled default.
    pub path: Option<PathBuf>,
    /// Font size in points.
    pub size_pt: f32,
    /// Display DPI (pixels per inch). Defaults to 96.0 for 1x displays.
    pub dpi: f32,
}

/// Owned font data with extracted metrics.
/// Holds the raw font bytes so that `FontRef` can be derived from them.
pub struct FontData {
    /// Raw font file bytes (kept alive for FontRef borrowing).
    data: Vec<u8>,
    /// Index of the font within the file (for .ttc collections).
    index: u32,
    /// Size in pixels (computed from size_pt and dpi).
    size_px: f32,
    /// Cell width in pixels (advance width of a reference character).
    cell_width: f32,
    /// Cell height in pixels (ascent + descent + line gap).
    cell_height: f32,
    /// Ascent in pixels (distance from baseline to top of cell).
    ascent: f32,
    /// Descent in pixels (distance from baseline to bottom, positive downward).
    descent: f32,
}
```

**Functions:**

```rust
impl FontData {
    /// Load a font from a file path at the given configuration.
    ///
    /// Reads the file, parses font tables, extracts metrics at the
    /// configured size and DPI.
    pub fn load(config: &FontConfig) -> anyhow::Result<Self>

    /// Create a swash FontRef borrowing from the internal data.
    /// Used by the shaper and rasterizer.
    pub fn font_ref(&self) -> swash::FontRef<'_>

    /// Cell width in pixels.
    pub fn cell_width(&self) -> f32

    /// Cell height in pixels.
    pub fn cell_height(&self) -> f32

    /// Ascent in pixels (baseline to top of cell).
    pub fn ascent(&self) -> f32

    /// Descent in pixels (positive downward).
    pub fn descent(&self) -> f32

    /// Font size in pixels.
    pub fn size_px(&self) -> f32
}
```

**Metrics extraction approach:**

Using swash's `FontRef` and charmap:
1. Get a `FontRef` from the loaded data
2. Use `FontRef::metrics(&[])` with the target size to get `ascent`, `descent`, `leading` (line gap)
3. Cell height = `ascent + descent + leading` (all in pixels after scaling)
4. Cell width = advance width of glyph for `'M'` (or `' '` as fallback) from `FontRef::glyph_metrics(&[])` at the target size
5. Baseline offset (ascent) = distance from top of cell to the baseline

**Test strategy:**

Happy path:
- Load a known monospace font file (use a test fixture, e.g., a bundled copy of a small open-source mono font or use the system's default monospace via a known path in CI). For testability, embed a small open-source font as `include_bytes!` in test code (e.g., JetBrains Mono or similar with a permissive license).
- `cell_width > 0.0` and `cell_height > 0.0`
- `cell_height >= ascent + descent` (line gap may be 0)
- `ascent > 0.0`
- `size_px` is correctly computed from `size_pt * dpi / 72.0`
- `font_ref()` returns a valid `FontRef` that can resolve glyph IDs

Error cases:
- Non-existent file path returns `Err`
- Empty file (0 bytes) returns `Err`
- Non-font file (e.g., a text file) returns `Err`
- Valid font but no glyph for 'M': falls back to space or average advance

Edge cases:
- Very small font size (e.g., 1pt): produces non-zero metrics
- Very large font size (e.g., 200pt): produces valid metrics without overflow
- Font collection (.ttc) file: loads the first face (index 0)

### Unit 2: Text shaping with rustybuzz

Take a string and a `FontData` reference, run it through rustybuzz, and return a list of positioned glyph IDs. For this task, shaping is per-cell (single character at a time) -- no multi-character ligature clusters. The shaper exists as the integration point that VEI-44 will extend for ligatures.

**File:** `crates/veil/src/font/shaper.rs`

**Types:**

```rust
/// A shaped glyph with its ID and positioning offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShapedGlyph {
    /// Glyph ID within the font.
    pub glyph_id: u16,
    /// Cluster index (maps back to the source character index).
    pub cluster: u32,
    /// Horizontal offset from the cell origin in pixels.
    pub x_offset: i32,
    /// Vertical offset from the baseline in pixels.
    pub y_offset: i32,
    /// Horizontal advance in pixels (should equal cell_width for monospace).
    pub x_advance: i32,
}

/// Shapes text into positioned glyphs using rustybuzz.
pub struct Shaper {
    /// Owned copy of the font data for rustybuzz's Face.
    /// rustybuzz::Face borrows from this.
    face_data: Vec<u8>,
    face_index: u32,
    size_px: f32,
}
```

**Functions:**

```rust
impl Shaper {
    /// Create a new shaper from font data.
    pub fn new(font_data: &FontData) -> anyhow::Result<Self>

    /// Shape a text string, returning positioned glyphs.
    ///
    /// For this initial implementation, shapes the full string as a single
    /// run with default script/language detection. Each character maps to
    /// one glyph (ligatures are VEI-44).
    pub fn shape(&self, text: &str) -> Vec<ShapedGlyph>
}
```

**Shaping approach:**

1. Create a `rustybuzz::Face` from the owned data (on each call, or cache it -- Face is cheap to create)
2. Create a `rustybuzz::UnicodeBuffer`, add the text, set direction LTR
3. Call `rustybuzz::shape(&face, &[], buffer)` -- empty features list (no ligature features enabled yet)
4. Iterate the output `GlyphBuffer`, extracting glyph IDs and positions
5. Convert positions from font units to pixels using the ppem scale factor
6. Return `Vec<ShapedGlyph>`

**Test strategy:**

Happy path:
- Shape "A" -- returns 1 glyph with glyph_id != 0 (notdef), cluster == 0
- Shape "Hello" -- returns 5 glyphs, one per character, clusters 0..5
- Shape "" (empty string) -- returns empty vec
- All x_advance values are consistent (monospace: same advance for every glyph)
- Glyph IDs are non-zero for basic ASCII characters

Error cases:
- Shape a character not in the font (e.g., a rare CJK char in a Latin-only font): returns glyph_id 0 (notdef glyph). This is valid behavior, not an error.

Edge cases:
- Shape a single space character: returns 1 glyph with expected advance
- Shape a string with only newlines/control chars: returns shaped output (control chars get notdef or space glyph)
- Unicode multi-byte character (e.g., "e" with combining accent): returns shaped glyphs (shaping may merge into one glyph or keep separate depending on font)

### Unit 3: Glyph rasterization with swash

Take a glyph ID and a `FontData` reference, rasterize it to a grayscale bitmap using swash's `ScaleContext` and `Render` API. Returns the bitmap data, dimensions, and bearing offsets needed for correct placement.

**File:** `crates/veil/src/font/rasterizer.rs`

**Types:**

```rust
/// A rasterized glyph bitmap with placement metadata.
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    /// Glyph ID this bitmap was rasterized from.
    pub glyph_id: u16,
    /// Grayscale bitmap data (1 byte per pixel, alpha/coverage values).
    pub bitmap: Vec<u8>,
    /// Bitmap width in pixels.
    pub width: u32,
    /// Bitmap height in pixels.
    pub height: u32,
    /// Horizontal bearing: offset from the cell origin to the left edge
    /// of the bitmap, in pixels.
    pub bearing_x: i32,
    /// Vertical bearing: offset from the baseline to the top edge of
    /// the bitmap, in pixels (positive upward in font coordinates).
    pub bearing_y: i32,
}

/// Rasterizes glyphs using swash.
///
/// Holds a `ScaleContext` which caches internal scaling state for
/// performance across multiple rasterize calls with the same font.
pub struct Rasterizer {
    context: swash::scale::ScaleContext,
}
```

**Functions:**

```rust
impl Rasterizer {
    /// Create a new rasterizer.
    pub fn new() -> Self

    /// Rasterize a single glyph at the given size.
    ///
    /// Returns None if the glyph cannot be rasterized (e.g., notdef
    /// glyph with no outline, or a glyph ID that doesn't exist).
    pub fn rasterize(
        &mut self,
        font_data: &FontData,
        glyph_id: u16,
    ) -> Option<RasterizedGlyph>
}
```

**Rasterization approach:**

1. Get a `FontRef` from `font_data.font_ref()`
2. Create a `ScaleContext::new()` (or reuse the stored one)
3. Build a scaler: `context.builder(font_ref).size(size_px).build()`
4. Render the glyph: use `swash::scale::Render::new(&[Source::ColorOutline(0), Source::Outline]).render(&mut scaler, glyph_id)`
5. The render result contains the bitmap (image data), placement (left, top bearing), and dimensions
6. Convert to our `RasterizedGlyph` struct

**Test strategy:**

Happy path:
- Rasterize glyph for 'A' (get glyph_id via font charmap): returns `Some(RasterizedGlyph)` with non-zero width and height
- Bitmap data length equals `width * height`
- Bitmap contains non-zero values (the glyph has visible pixels)
- bearing_y is positive (glyph sits above baseline)

Error cases:
- Rasterize glyph_id 0 (notdef): may return `Some` with the notdef box or `None` depending on the font. Test that it does not panic.
- Rasterize an out-of-range glyph_id (e.g., 65535): returns `None`

Edge cases:
- Rasterize space character: may return a glyph with zero-area bitmap (space has no visible pixels). Should return `Some` with width/height 0 or `None` -- both are acceptable.
- Rasterize at very small size (1px): produces valid output without panic
- Rasterize at large size (200px): produces valid output

### Unit 4: Glyph atlas (CPU-side texture packing)

A CPU-side texture atlas that accumulates rasterized glyph bitmaps. Uses a shelf-packing algorithm: glyphs are placed left-to-right in rows ("shelves"), each shelf is as tall as the tallest glyph in that row. When a shelf runs out of horizontal space, a new shelf starts below. When the atlas is full, it grows (doubles in size and re-packs).

The atlas stores UV coordinates for each packed glyph so the renderer can map glyph quads to the correct region of the atlas texture.

**File:** `crates/veil/src/font/atlas.rs`

**Types:**

```rust
/// UV coordinates for a glyph within the atlas texture.
/// Normalized to [0.0, 1.0] range.
#[derive(Debug, Clone, Copy)]
pub struct AtlasRegion {
    /// Left edge U coordinate.
    pub u_min: f32,
    /// Top edge V coordinate.
    pub v_min: f32,
    /// Right edge U coordinate.
    pub u_max: f32,
    /// Bottom edge V coordinate.
    pub v_max: f32,
    /// Pixel width of the glyph in the atlas.
    pub width: u32,
    /// Pixel height of the glyph in the atlas.
    pub height: u32,
    /// Bearing X from the rasterized glyph.
    pub bearing_x: i32,
    /// Bearing Y from the rasterized glyph.
    pub bearing_y: i32,
}

/// A shelf (row) within the atlas for the packing algorithm.
struct Shelf {
    /// Y position of this shelf's top edge.
    y: u32,
    /// Current X cursor (next glyph goes here).
    x: u32,
    /// Height of this shelf (tallest glyph placed so far).
    height: u32,
}

/// CPU-side glyph atlas with shelf packing.
pub struct GlyphAtlas {
    /// Grayscale bitmap data (1 byte per pixel).
    bitmap: Vec<u8>,
    /// Atlas width in pixels.
    width: u32,
    /// Atlas height in pixels.
    height: u32,
    /// Packed shelves.
    shelves: Vec<Shelf>,
    /// Map from glyph_id to atlas region.
    entries: HashMap<u16, AtlasRegion>,
    /// Whether the atlas data has changed since last GPU upload.
    dirty: bool,
}
```

**Functions:**

```rust
impl GlyphAtlas {
    /// Create a new empty atlas with the given initial dimensions.
    /// Dimensions should be powers of 2 (e.g., 512x512 or 1024x1024).
    pub fn new(width: u32, height: u32) -> Self

    /// Look up a glyph in the atlas. Returns None if not yet packed.
    pub fn get(&self, glyph_id: u16) -> Option<&AtlasRegion>

    /// Insert a rasterized glyph into the atlas.
    ///
    /// Returns the AtlasRegion where the glyph was placed.
    /// If the glyph is already in the atlas, returns the existing region.
    /// If there is no room, grows the atlas and retries.
    pub fn insert(&mut self, glyph: &RasterizedGlyph) -> AtlasRegion

    /// Returns true if the atlas has been modified since the last
    /// call to `mark_clean`.
    pub fn is_dirty(&self) -> bool

    /// Mark the atlas as clean (call after uploading to GPU).
    pub fn mark_clean(&mut self)

    /// Get the raw bitmap data for GPU upload.
    pub fn bitmap(&self) -> &[u8]

    /// Get the atlas dimensions.
    pub fn dimensions(&self) -> (u32, u32)
}
```

**Shelf-packing algorithm:**

1. On `insert`, check `entries` for existing glyph -- return early if found
2. Try to place the glyph on the current (last) shelf: if `shelf.x + glyph.width <= atlas.width`, place it there
3. If no room on current shelf, start a new shelf at `y = previous_shelf.y + previous_shelf.height + 1` (1px padding)
4. If the new shelf won't fit vertically (`new_shelf_y + glyph.height > atlas.height`), grow the atlas
5. Growth: double the height, allocate new bitmap, copy old data, retry placement
6. Write glyph bitmap into the atlas at the allocated position
7. Compute UV coordinates: `u_min = x / atlas_width`, etc. (normalized to [0, 1])
8. Store the entry in `entries`, set `dirty = true`

**1px padding between glyphs** to prevent texture filtering artifacts at glyph boundaries.

**Test strategy:**

Happy path:
- Create atlas, insert one glyph: `get()` returns `Some` with correct dimensions
- Insert same glyph twice: second call returns same `AtlasRegion` (no duplication)
- Insert multiple glyphs: all are retrievable, UV regions don't overlap
- UV coordinates are in [0.0, 1.0] range
- `bitmap()` data length equals `width * height`
- `is_dirty()` returns true after insert, false after `mark_clean()`
- Glyph bitmap data is correctly copied into the atlas bitmap at the right offset

Error cases:
- Insert a glyph with 0x0 dimensions: atlas handles gracefully (stores entry with zero-area region)

Edge cases:
- Fill a shelf completely (glyph.x + width == atlas.width): next glyph starts new shelf
- Fill all shelves: atlas grows (height doubles), all existing entries remain valid with updated UV coordinates
- Single large glyph that requires immediate growth: atlas grows to accommodate
- Atlas growth: verify existing entries get correct UV coordinates after growth (UV values change because atlas height changed)

### Unit 5: Text vertex type and textured quad generation

A new vertex type for textured quads (position + UV + color) and helper functions that generate text quads from shaped glyphs and atlas regions. This parallels `vertex.rs` and `quad_builder.rs` but for text.

**File:** `crates/veil/src/font/text_vertex.rs`

**Types:**

```rust
/// A vertex with position (2D), texture coordinates (UV), and foreground color.
/// 32 bytes: 2*f32 (position) + 2*f32 (uv) + 4*f32 (color) = 32 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextVertex {
    /// Position in pixel coordinates.
    pub position: [f32; 2],
    /// Texture coordinates (UV) into the glyph atlas.
    pub uv: [f32; 2],
    /// Foreground RGBA color.
    pub color: [f32; 4],
}
```

**Functions:**

```rust
impl TextVertex {
    /// Describe the vertex buffer layout for the text pipeline.
    ///
    /// Stride is 32 bytes. Attributes:
    /// - position: Float32x2 at offset 0
    /// - uv: Float32x2 at offset 8
    /// - color: Float32x4 at offset 16
    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static>
}

/// Generate the 4 vertices for a textured glyph quad.
///
/// `cell_x`, `cell_y`: top-left corner of the cell in pixel coordinates.
/// `region`: atlas region with UV coords and glyph dimensions/bearings.
/// `ascent`: font ascent in pixels (baseline offset from top of cell).
/// `color`: foreground RGBA color.
pub fn text_quad_vertices(
    cell_x: f32,
    cell_y: f32,
    region: &AtlasRegion,
    ascent: f32,
    color: [f32; 4],
) -> [TextVertex; 4]

/// Generate 6 indices for a textured quad (identical to solid quad indices).
/// Re-exports or delegates to vertex::quad_indices.
pub fn text_quad_indices(base: u16) -> [u16; 6]
```

**Glyph quad positioning:**

The glyph bitmap is placed relative to the cell origin using the bearing offsets:
- `quad_x = cell_x + bearing_x` (horizontal offset from cell left to glyph left)
- `quad_y = cell_y + ascent - bearing_y` (vertical: baseline is at `cell_y + ascent`, glyph top is `bearing_y` pixels above baseline)
- `quad_width = region.width` (pixel width of the glyph bitmap)
- `quad_height = region.height` (pixel height of the glyph bitmap)

**Test strategy:**

Happy path:
- `TextVertex` is 32 bytes
- `TextVertex` satisfies `Pod` and `Zeroable`
- `text_quad_vertices` produces 4 vertices with correct UV coordinates matching the atlas region
- All vertex positions are offset correctly from the cell origin using bearings and ascent
- `text_quad_indices(0)` produces `[0, 2, 1, 1, 2, 3]`

Edge cases:
- Region with zero dimensions: produces degenerate quad (no panic)
- Negative bearing_x: glyph extends left of cell origin (valid for some glyphs)
- bearing_y larger than ascent: glyph extends above cell top (valid for tall glyphs)

### Unit 6: Text render pipeline and atlas GPU texture

Add a second wgpu render pipeline to the `Renderer` for drawing textured quads. This pipeline samples from the glyph atlas texture and multiplies by the per-vertex foreground color. Includes the WGSL shader for text rendering, the texture/sampler bind group, and atlas upload logic.

**File:** `crates/veil/src/font/text_pipeline.rs`, modify `crates/veil/src/shader.wgsl` (or new `text_shader.wgsl`)

**New shader (`crates/veil/src/text_shader.wgsl`):**

```wgsl
struct WindowUniform {
    width: f32,
    height: f32,
};

@group(0) @binding(0)
var<uniform> window: WindowUniform;

@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;
@group(1) @binding(1)
var atlas_sampler: sampler;

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
fn vs_main(in: TextVertexInput) -> TextVertexOutput {
    var out: TextVertexOutput;
    let clip_x = (in.position.x / window.width) * 2.0 - 1.0;
    let clip_y = 1.0 - (in.position.y / window.height) * 2.0;
    out.clip_position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: TextVertexOutput) -> @location(0) vec4<f32> {
    // Sample the alpha channel from the grayscale atlas texture.
    let alpha = textureSample(atlas_texture, atlas_sampler, in.uv).r;
    // Multiply by foreground color; atlas provides coverage, color provides hue.
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
```

**Types and functions in `text_pipeline.rs`:**

```rust
/// Manages the wgpu resources for text rendering:
/// the render pipeline, atlas texture, sampler, and bind groups.
pub struct TextPipeline {
    render_pipeline: wgpu::RenderPipeline,
    atlas_texture: wgpu::Texture,
    atlas_texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    atlas_bind_group: wgpu::BindGroup,
    atlas_bind_group_layout: wgpu::BindGroupLayout,
    atlas_size: (u32, u32),
}

impl TextPipeline {
    /// Create the text render pipeline.
    ///
    /// Requires the device, surface format, and the window uniform bind
    /// group layout (shared with the solid-color pipeline).
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        window_bind_group_layout: &wgpu::BindGroupLayout,
        initial_atlas_size: (u32, u32),
    ) -> Self

    /// Upload the glyph atlas bitmap to the GPU texture.
    ///
    /// If the atlas dimensions have changed (atlas grew), recreates
    /// the texture and bind group. Otherwise, writes data to the
    /// existing texture.
    pub fn upload_atlas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        atlas: &GlyphAtlas,
    )

    /// Get a reference to the render pipeline.
    pub fn pipeline(&self) -> &wgpu::RenderPipeline

    /// Get a reference to the atlas bind group.
    pub fn atlas_bind_group(&self) -> &wgpu::BindGroup
}
```

**GPU texture details:**

- Format: `R8Unorm` (single channel, 8-bit, matching the grayscale atlas)
- Usage: `TEXTURE_BINDING | COPY_DST`
- Sampler: linear filtering, clamp-to-edge addressing
- Bind group layout: group 1, binding 0 = texture, binding 1 = sampler
- The window uniform bind group (group 0) is shared with the solid-color pipeline

**Test strategy:**

Structural tests (no GPU):
- `TextVertex` is 32 bytes
- `TextVertex` buffer layout has correct stride and attribute offsets

Integration tests (require GPU, `#[ignore]`):
- `TextPipeline::new` succeeds with a real device
- `upload_atlas` with a small test atlas (e.g., 64x64 with known data) completes without error
- Atlas resize (upload a larger atlas after initial creation) recreates texture correctly

### Unit 7: Frame integration -- wiring text into the render loop

Extend `FrameGeometry` and `build_frame_geometry` to include text quads. Extend `Renderer` to own a `TextPipeline`, `GlyphAtlas`, `FontData`, `Shaper`, and `Rasterizer`. During frame building, for each cell that has a character, shape it, ensure the glyph is in the atlas, and generate a textured quad.

**Files modified:** `crates/veil/src/frame.rs`, `crates/veil/src/renderer.rs`, `crates/veil/src/main.rs`

**Changes to `FrameGeometry`:**

```rust
pub struct FrameGeometry {
    /// Solid-color vertices (backgrounds, cursors, dividers, borders).
    pub vertices: Vec<Vertex>,
    /// Solid-color indices.
    pub indices: Vec<u16>,
    /// Text vertices (glyph quads).
    pub text_vertices: Vec<TextVertex>,
    /// Text indices.
    pub text_indices: Vec<u16>,
    /// The clear color.
    pub clear_color: wgpu::Color,
}
```

**Changes to `build_frame_geometry`:**

The function gains a `FontData` parameter (or a `FontContext` struct bundling `FontData`, `Shaper`, `Rasterizer`, `GlyphAtlas`). For the initial integration, text rendering uses **placeholder cell content** -- a hardcoded test string (e.g., "Hello, Veil!" placed in the first row of the first pane) to validate the full pipeline end-to-end. Real terminal cell content from libghosty comes in a later integration task.

```rust
/// Mutable font context passed through the frame builder.
/// Bundles font-related state that may be mutated during frame building
/// (atlas gets new entries, rasterizer caches scale state).
pub struct FontContext {
    pub font_data: FontData,
    pub shaper: Shaper,
    pub rasterizer: Rasterizer,
    pub atlas: GlyphAtlas,
}
```

**Changes to `Renderer::render`:**

After the existing solid-color draw call, add a second draw call:
1. If `atlas.is_dirty()`, call `text_pipeline.upload_atlas()`
2. Set the text pipeline
3. Set bind groups (group 0 = window uniform, group 1 = atlas texture)
4. Upload text vertex/index buffers
5. Draw indexed
6. Call `atlas.mark_clean()`

**Test strategy:**

Happy path:
- `FrameGeometry` with both solid and text geometry: both vertex arrays are non-empty
- Text vertices have valid UV coordinates (in [0, 1] range)
- Text vertices have positions within the cell bounds

Integration test (manual/visual):
- `cargo run` shows placeholder text rendered in the first pane
- Text appears on top of the dark cell backgrounds
- Resizing the window re-renders text at correct positions

Edge cases:
- Frame with no text content: `text_vertices` and `text_indices` are empty, text draw call is skipped
- Atlas dirty flag is correctly managed across frames

## Acceptance Criteria

1. `cargo build -p veil` succeeds with `swash` and `rustybuzz` dependencies added
2. `cargo test -p veil` passes all new tests (font loading, shaping, rasterization, atlas, text vertices)
3. `cargo clippy --all-targets --all-features -- -D warnings` passes
4. `cargo fmt --check` passes
5. A font file can be loaded and metrics extracted (cell width, cell height, ascent, descent are non-zero and reasonable)
6. Text can be shaped through rustybuzz, producing valid glyph IDs for ASCII characters
7. Glyphs can be rasterized via swash, producing non-empty grayscale bitmaps
8. The glyph atlas correctly packs multiple glyphs with no overlapping UV regions
9. The atlas grows when full without losing previously packed glyphs
10. `TextVertex` type is 32 bytes with `Pod`/`Zeroable` derives
11. The text WGSL shader compiles and renders textured quads from the atlas
12. `cargo run` displays visible text characters on screen (placeholder test string)
13. Existing solid-color rendering (backgrounds, cursors, dividers, borders) continues to work unchanged
14. All modules handle edge cases without panics (empty strings, missing glyphs, zero-size inputs)

## Dependencies

**New workspace dependencies (add to root `Cargo.toml` `[workspace.dependencies]`):**

| Crate | Version | Purpose |
|-------|---------|---------|
| `swash` | `0.2` | Font introspection and glyph rasterization |
| `rustybuzz` | `0.20` | OpenType text shaping (HarfBuzz port) |

**Add to `crates/veil/Cargo.toml` `[dependencies]`:**

```toml
swash = { workspace = true }
rustybuzz = { workspace = true }
```

**Test font fixture:**

A small open-source monospace font needs to be available for tests. Options:
- Embed a small subset font as `include_bytes!` in test modules (most portable, no filesystem dependency)
- Use `JetBrains Mono` (OFL license) or `Hack` (MIT license) -- both are small and permissively licensed
- Place the font file at `crates/veil/test_fixtures/test_font.ttf` and reference via `env!("CARGO_MANIFEST_DIR")`

**New files:**

| File | Purpose |
|------|---------|
| `crates/veil/src/font/mod.rs` | Module root, re-exports |
| `crates/veil/src/font/loader.rs` | `FontConfig`, `FontData` -- font loading and metrics |
| `crates/veil/src/font/shaper.rs` | `Shaper`, `ShapedGlyph` -- rustybuzz integration |
| `crates/veil/src/font/rasterizer.rs` | `Rasterizer`, `RasterizedGlyph` -- swash rasterization |
| `crates/veil/src/font/atlas.rs` | `GlyphAtlas`, `AtlasRegion` -- shelf-packing atlas |
| `crates/veil/src/font/text_vertex.rs` | `TextVertex`, `text_quad_vertices` -- textured quad geometry |
| `crates/veil/src/font/text_pipeline.rs` | `TextPipeline` -- wgpu pipeline for text rendering |
| `crates/veil/src/text_shader.wgsl` | WGSL shader for textured glyph quads |
| `crates/veil/test_fixtures/test_font.ttf` | Small open-source mono font for tests |

**Modified files:**

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `swash`, `rustybuzz` to workspace dependencies |
| `crates/veil/Cargo.toml` | Add `swash`, `rustybuzz` to dependencies |
| `crates/veil/src/main.rs` | Add `mod font;`, create `FontContext` on startup, pass to frame builder |
| `crates/veil/src/frame.rs` | Add `text_vertices`/`text_indices` to `FrameGeometry`, generate text quads in `build_frame_geometry` |
| `crates/veil/src/renderer.rs` | Add `TextPipeline` to `Renderer`, second draw call in `render()`, atlas upload |
