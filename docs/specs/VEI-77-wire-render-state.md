# VEI-77: Wire libghosty Render State into Frame Builder

## Context

The frame builder (`crates/veil/src/frame.rs`) currently uses hardcoded constants to render the terminal grid:

- `DEFAULT_COLS = 80`, `DEFAULT_ROWS = 24` for all panes
- Static `BG_COLOR = [0.1, 0.1, 0.1, 1.0]` for every cell background
- Static `CURSOR_COLOR = [0.9, 0.9, 0.9, 1.0]` at position `(0, 0)`
- No text rendering at all -- the font pipeline (`loader`, `shaper`, `rasterizer`, `atlas`, `text_vertex`) exists but is not called

This means the terminal window shows a dark grid with a white cursor in the top-left corner, but no text, no colors, and no actual terminal content. VEI-76 wired PTY output into `veil_ghostty::Terminal` instances via the `TerminalMap`, so terminal state is now live and updated. This task makes that state visible by:

1. Reading per-cell data (characters, fg/bg colors) from libghosty's render state row/cell iteration API
2. Shaping and rasterizing those characters through the font pipeline
3. Generating correctly colored background quads and textured glyph quads
4. Uploading the glyph atlas texture to the GPU and rendering text via a new text pipeline

### What exists

**Terminal state pipeline (VEI-76):**
- `TerminalMap` in `crates/veil/src/terminal_map.rs` -- `HashMap<SurfaceId, Box<dyn TerminalWriter>>`. Owns terminal instances, keyed by `SurfaceId`. The `TerminalWriter` trait has `write_vt()`, `resize()`, `cols()`, `rows()`.
- `process_state_update()` -- drains `StateUpdate::PtyOutput` events and calls `write_vt()` on the correct terminal.
- `VeilApp` in `main.rs` -- owns `pty_manager`, `app_state`, `focus`, `renderer`. The render loop calls `build_frame_geometry()` and `renderer.render()` every frame.

**libghosty render state (VEI-6):**
- `veil_ghostty::RenderState` -- `new()`, `update(&mut self, terminal: &mut Terminal)`, `dirty()`, `set_dirty()`, `cursor()`, `colors()`, `cols()`, `rows()`.
- Row/cell iteration C API (in `vendor/ghostty/include/ghostty/vt/render.h`, not yet wrapped in Rust):
  - `ghostty_render_state_row_iterator_new/next/free` -- iterates rows
  - `ghostty_render_state_row_cells_new/next/select/free` -- iterates cells within a row
  - Per-cell data: `GRAPHEMES_LEN` (u32), `GRAPHEMES_BUF` (u32 codepoints), `BG_COLOR` (GhosttyColorRgb, INVALID_VALUE if default), `FG_COLOR` (GhosttyColorRgb, INVALID_VALUE if default), `STYLE` (GhosttyStyle)
  - `ghostty_render_state_row_get(iter, ROW_DATA_DIRTY, &bool)` -- per-row dirty flag
  - `ghostty_render_state_row_set(iter, ROW_OPTION_DIRTY, &bool)` -- clear per-row dirty

**Font pipeline (VEI-8):**
- `FontData::load(config)` -- loads a font, extracts `cell_width`, `cell_height`, `ascent`, `descent`, `size_px`
- `Shaper::new(font_data)` + `shape(text) -> Vec<ShapedGlyph>` -- rustybuzz shaping, returns glyph IDs + advances
- `Rasterizer::new()` + `rasterize(font_data, glyph_id) -> Option<RasterizedGlyph>` -- swash rasterization, returns bitmap + bearings
- `GlyphAtlas::new(w, h)` + `insert(glyph) -> AtlasRegion` + `get(glyph_id) -> Option<&AtlasRegion>` -- shelf-packed CPU-side atlas, tracks dirty flag
- `TextVertex` -- 32-byte vertex (position, uv, color), with `buffer_layout()` for wgpu
- `text_quad_vertices(cell_x, cell_y, region, ascent, color)` -- generates 4 vertices for a textured glyph quad
- `text_quad_indices(base)` -- generates 6 indices for a quad

**GPU pipeline (VEI-7):**
- `Renderer` -- owns wgpu device, queue, surface, pipeline. Single render pipeline using `Vertex` (position + color, no UV). Uses `shader.wgsl` which only handles solid color quads.
- `FrameGeometry` -- `vertices: Vec<Vertex>`, `indices: Vec<u16>`, `clear_color`.
- The shader converts pixel coords to clip space and outputs solid color. No texture sampling.

### What's missing

1. **Row/cell iteration FFI** -- The `ffi.rs` module declares terminal and render-state lifecycle functions but not the row iterator or cell iterator functions. These C API functions need Rust `extern "C"` declarations and safe wrappers.

2. **Cell data extraction** -- No Rust API to iterate rows/cells and read per-cell graphemes, bg_color, fg_color. The `RenderState` wrapper only exposes global state (cols, rows, cursor, colors).

3. **TerminalWriter trait expansion** -- The `TerminalWriter` trait lacks render-state access. It only has `write_vt()`, `resize()`, `cols()`, `rows()`. To render cell content, the frame builder needs access to per-cell data. Since `Terminal` is not `Send` and lives behind `dyn TerminalWriter`, we need a way to extract a snapshot of cell data that can be consumed by the frame builder.

4. **Font pipeline integration** -- `FontData`, `Shaper`, `Rasterizer`, and `GlyphAtlas` exist but are never instantiated in `VeilApp` or the render loop. No font is loaded at startup.

5. **Text render pipeline** -- The GPU has only one pipeline (solid color quads). Text rendering requires a second pipeline with a texture sampler, the `TextVertex` layout (position + UV + color), and a text shader that samples from the glyph atlas.

6. **Per-cell background colors** -- `build_cell_background_quads()` uses a single `bg_color` for the entire grid. It needs to accept per-cell colors.

7. **Frame builder wiring** -- `build_frame_geometry()` takes only `AppState` and `FocusManager`. It needs access to terminal cell data and the font pipeline to generate text quads.

### Design decisions

**Cell data snapshot via a `CellData` struct, not direct FFI handle passing.**

The row/cell iteration API requires sequential calls (`row_iterator_next`, `row_cells_next`, `row_cells_get`) that borrow from the render state. Rather than threading FFI handles through the frame builder, we extract a `CellGrid` snapshot (a 2D grid of `CellData` structs) from the render state before frame building. This keeps the frame builder pure (no FFI) and testable.

**Two-pass rendering: background quads first, text quads second.**

The existing pipeline handles solid-color quads. Text quads need a separate pipeline with texture sampling. We render in two passes within the same render pass: (1) solid-color quads (cell backgrounds, cursor, dividers, focus border) via the existing pipeline, (2) textured quads (glyphs) via a new text pipeline. This avoids mixing vertex types in a single draw call.

**Font loaded at startup, atlas populated lazily.**

`FontData` is loaded once during `VeilApp::resumed()`. The `Shaper`, `Rasterizer`, and `GlyphAtlas` are created alongside it. Glyphs are rasterized on demand (first time a character is seen) and cached in the atlas. The atlas texture is uploaded to the GPU when dirty.

**`TerminalWriter` extended with `render_cells()` method.**

Adding a `render_cells(&mut self) -> CellGrid` method to the `TerminalWriter` trait. The real implementation creates a `RenderState`, calls `update()`, iterates rows/cells, and returns a `CellGrid`. The mock implementation returns a configurable grid. This keeps the frame builder testable without FFI.

## Implementation Units

### Unit 1: Add row/cell iteration FFI declarations and safe wrappers

**Location:** `crates/veil-ghostty/src/ffi.rs`, `crates/veil-ghostty/src/render_state.rs`, `crates/veil-ghostty/src/lib.rs`

Add to `ffi.rs`:
- Opaque handle types: `GhosttyRenderStateRowIterator = *mut c_void`, `GhosttyRenderStateRowCells = *mut c_void`
- Constants for `GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR = 4`
- Constants for `GhosttyRenderStateRowData`: `DIRTY = 1`, `CELLS = 3`
- Constants for `GhosttyRenderStateRowOption`: `DIRTY = 0`
- Constants for `GhosttyRenderStateRowCellsData`: `GRAPHEMES_LEN = 3`, `GRAPHEMES_BUF = 4`, `BG_COLOR = 5`, `FG_COLOR = 6`
- `GhosttyStyle` struct (sized struct with `size`, `fg_color`, `bg_color`, `bold`, `italic`, etc.)
- `GhosttyStyleColor` struct (tagged union with `tag` and value union)
- `GhosttyStyleColorTag` constants: `NONE = 0`, `PALETTE = 1`, `RGB = 2`
- extern "C" function declarations:
  - `ghostty_render_state_row_iterator_new(allocator, out_iterator) -> i32`
  - `ghostty_render_state_row_iterator_free(iterator)`
  - `ghostty_render_state_row_iterator_next(iterator) -> bool`
  - `ghostty_render_state_row_get(iterator, data, out) -> i32`
  - `ghostty_render_state_row_set(iterator, option, value) -> i32`
  - `ghostty_render_state_row_cells_new(allocator, out_cells) -> i32`
  - `ghostty_render_state_row_cells_next(cells) -> bool`
  - `ghostty_render_state_row_cells_select(cells, x) -> i32`
  - `ghostty_render_state_row_cells_get(cells, data, out) -> i32`
  - `ghostty_render_state_row_cells_free(cells)`

Add to `render_state.rs` a safe `CellGrid` extraction method on `RenderState`:
- `pub fn extract_cells(&mut self, terminal: &mut Terminal) -> Result<CellGrid, GhosttyError>` -- calls `update()`, creates row iterator, populates iterator via `ghostty_render_state_get(state, ROW_ITERATOR, &iter)`, iterates rows/cells, reads graphemes + bg_color + fg_color for each cell, returns a `CellGrid`.

Add new public types to `render_state.rs` (and re-export from `lib.rs`):
- `CellData` -- `graphemes: Vec<char>`, `fg_color: Option<Color>`, `bg_color: Option<Color>`, `bold: bool`
- `CellGrid` -- `cols: u16`, `rows: u16`, `cells: Vec<Vec<CellData>>`, `cursor: CursorState`, `colors: RenderColors`

**Tests (require libghosty, `#[cfg(not(no_libghosty))]`):**
- Happy path: create terminal, write "Hello", extract cells, verify `cells[0][0..5]` contain 'H','e','l','l','o' with default fg/bg
- Happy path: write ANSI red foreground `"\x1b[31mX"`, extract cells, verify cell fg_color is red (palette index resolved)
- Happy path: empty terminal, all cells have empty graphemes
- Edge case: terminal with cursor moved, verify cursor position in CellGrid
- Edge case: terminal resized, extract cells, verify cols/rows match

### Unit 2: Extend `TerminalWriter` trait with `render_cells()` and implement for real terminal

**Location:** `crates/veil/src/terminal_map.rs`

Extend the `TerminalWriter` trait:
```rust
/// Extract a snapshot of cell data for rendering.
/// Returns None if the terminal has no render state available.
fn render_cells(&mut self) -> Option<veil_ghostty::CellGrid>;
```

The real `TerminalWriter` implementation (which wraps `veil_ghostty::Terminal`) needs to own a `veil_ghostty::RenderState` alongside the `Terminal`. On `render_cells()`, it calls `render_state.extract_cells(&mut terminal)` and returns the result.

Since the real implementation lives in `main.rs` or a new module in the binary crate (because it depends on `veil_ghostty`), create `crates/veil/src/ghostty_terminal.rs`:
- `GhosttyTerminalWriter` struct wrapping `veil_ghostty::Terminal` + `veil_ghostty::RenderState`
- Implements `TerminalWriter` for `GhosttyTerminalWriter`
- `render_cells()` calls `self.render_state.extract_cells(&mut self.terminal)` and maps the result

Update `VeilApp::resumed()` in `main.rs` to create `GhosttyTerminalWriter` instances (wrapping both `Terminal` and `RenderState`) when inserting into the `TerminalMap`.

Update the `MockTerminalWriter` in tests to return a configurable `CellGrid` from `render_cells()`.

**Tests:**
- Happy path: mock returns a CellGrid, frame builder receives it
- Happy path: mock returns None (no render state), frame builder falls back to default behavior
- Edge case: TerminalMap with mix of terminals, some returning CellGrid, some None

### Unit 3: Per-cell background quads in the frame builder

**Location:** `crates/veil/src/quad_builder.rs`, `crates/veil/src/frame.rs`

Modify `CellGridParams` to accept per-cell colors:
```rust
pub struct CellGridParams {
    pub rect: Rect,
    pub cols: u16,
    pub rows: u16,
    /// Default background color (used when cell has no explicit bg).
    pub default_bg: [f32; 4],
    /// Per-cell background colors. If provided, must be `cols * rows` in length.
    /// Each entry is `None` (use default_bg) or `Some(color)`.
    pub cell_bg_colors: Option<Vec<Option<[f32; 4]>>>,
}
```

Update `build_cell_background_quads()` to use per-cell colors when `cell_bg_colors` is `Some`.

Update `build_frame_geometry()` to:
1. Accept a `&TerminalMap` parameter (or a trait object that can provide cell data)
2. For each pane layout, look up the terminal for that pane's `SurfaceId`
3. Call `render_cells()` to get the `CellGrid`
4. Use `CellGrid.cols`/`CellGrid.rows` instead of `DEFAULT_COLS`/`DEFAULT_ROWS`
5. Convert `CellGrid.cells[row][col].bg_color` to per-cell background colors
6. Use `CellGrid.cursor` for cursor position
7. Fall back to hardcoded defaults when no terminal is found for a pane

Helper function: `color_to_f32(color: veil_ghostty::Color) -> [f32; 4]` -- converts `Color { r, g, b }` (u8 RGB) to `[f32; 4]` (normalized RGBA with alpha 1.0).

**Tests:**
- Happy path: CellGridParams with cell_bg_colors=None uses default_bg for all cells
- Happy path: CellGridParams with per-cell colors produces vertices with correct colors
- Happy path: some cells None (default), some colored -- verify mixed colors
- Edge case: cell_bg_colors length mismatch -- clamp to available data
- Integration test: build_frame_geometry with mock TerminalMap returning CellGrid, verify vertex colors match cell bg
- Integration test: cursor at position (5, 3) from CellGrid.cursor, verify cursor quad position

### Unit 4: Font pipeline initialization and glyph caching

**Location:** `crates/veil/src/font_pipeline.rs` (new module)

Create a `FontPipeline` struct that bundles all font resources:
```rust
pub struct FontPipeline {
    font_data: FontData,
    shaper: Shaper,
    rasterizer: Rasterizer,
    atlas: GlyphAtlas,
}
```

Methods:
- `FontPipeline::new(config: &FontConfig) -> anyhow::Result<Self>` -- loads font, creates shaper, rasterizer, and 512x512 atlas
- `fn ensure_glyph(&mut self, ch: char) -> Option<AtlasRegion>` -- checks atlas cache, shapes + rasterizes on miss, inserts into atlas. Returns the atlas region for the glyph.
- `fn cell_width(&self) -> f32`
- `fn cell_height(&self) -> f32`
- `fn ascent(&self) -> f32`
- `fn atlas(&self) -> &GlyphAtlas` -- for GPU upload
- `fn atlas_mut(&mut self) -> &mut GlyphAtlas` -- for marking clean

The `ensure_glyph()` method:
1. Map char to glyph_id via `font_data.font_ref().charmap().map(ch)`
2. Check `atlas.get(glyph_id)` -- return if cached
3. Call `shaper.shape(&ch.to_string())` to get `ShapedGlyph`
4. Call `rasterizer.rasterize(&font_data, shaped.glyph_id)` -- returns `Option<RasterizedGlyph>`
5. If `Some`, call `atlas.insert(&rasterized)` and return the region
6. If `None` (space, control char), insert a zero-dimension entry and return it

**Tests:**
- Happy path: `ensure_glyph('A')` returns Some with non-zero width/height
- Happy path: calling `ensure_glyph('A')` twice returns same UV coords (cached)
- Happy path: multiple different characters all cacheable
- Edge case: space character returns zero-dimension region
- Edge case: control character returns zero-dimension region
- Edge case: character not in font returns notdef glyph

### Unit 5: Text quad generation in the frame builder

**Location:** `crates/veil/src/frame.rs`

Extend `FrameGeometry` to include text vertices:
```rust
pub struct FrameGeometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub text_vertices: Vec<TextVertex>,
    pub text_indices: Vec<u16>,
    pub clear_color: wgpu::Color,
}
```

In `build_frame_geometry()`, after building background quads for each pane, generate text quads:
1. For each cell in the `CellGrid` that has non-empty graphemes:
   a. Take the first codepoint (base character) of the grapheme
   b. Call `font_pipeline.ensure_glyph(ch)` to get the `AtlasRegion`
   c. Determine fg_color: use cell's `fg_color` if set, else `CellGrid.colors.foreground`
   d. Compute cell pixel position: `cell_x = pane_rect.x + col * cell_width`, `cell_y = pane_rect.y + row * cell_height`
   e. Call `text_quad_vertices(cell_x, cell_y, &region, font_pipeline.ascent(), fg_color_f32)`
   f. Call `text_quad_indices(base)` with correct base offset
   g. Append to `text_vertices` and `text_indices`

Update `build_frame_geometry()` signature to accept `&mut FontPipeline` (mutable because `ensure_glyph` may rasterize new glyphs).

Add a helper: `fn cell_fg_color(cell: &CellData, default_fg: &Color) -> [f32; 4]` -- resolves the cell's foreground color to RGBA f32.

**Tests:**
- Happy path: CellGrid with "Hi" in first row, verify 2 text quads generated
- Happy path: text vertex positions align with cell grid positions
- Happy path: text vertex colors match cell fg_color
- Happy path: text vertex UVs are non-zero (valid atlas coords)
- Edge case: empty cell generates no text quad
- Edge case: cell with space generates no visible text quad (zero-dim region)
- Edge case: pane with no terminal falls back to empty text quads

### Unit 6: Text render pipeline (GPU shader + wgpu pipeline)

**Location:** `crates/veil/src/text_shader.wgsl` (new), `crates/veil/src/renderer.rs`

Create `text_shader.wgsl`:
```wgsl
struct WindowUniform {
    width: f32,
    height: f32,
};

@group(0) @binding(0) var<uniform> window: WindowUniform;
@group(1) @binding(0) var atlas_texture: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

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
    let alpha = textureSample(atlas_texture, atlas_sampler, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
```

The atlas is a single-channel (R8) texture. The fragment shader reads the red channel as the glyph alpha/coverage value and multiplies it with the foreground color's alpha.

Update `Renderer`:
- Add fields: `text_render_pipeline: wgpu::RenderPipeline`, `atlas_texture: Option<wgpu::Texture>`, `atlas_bind_group: Option<wgpu::BindGroup>`, `atlas_sampler: wgpu::Sampler`, `atlas_bind_group_layout: wgpu::BindGroupLayout`
- In `Renderer::new()`:
  - Create the text shader module from `text_shader.wgsl`
  - Create atlas bind group layout (texture_2d + sampler at group(1))
  - Create text pipeline layout with two bind groups: group(0) = window uniform, group(1) = atlas
  - Create text render pipeline with `TextVertex::buffer_layout()` and alpha blending
  - Create the atlas sampler (linear filtering, clamp-to-edge)
- Add `fn upload_atlas(&mut self, atlas: &GlyphAtlas)`:
  - If atlas is dirty, create/recreate the GPU texture from `atlas.bitmap()` with format `R8Unorm`
  - Write atlas data to the texture via `queue.write_texture()`
  - Recreate the atlas bind group with the new texture + sampler
  - Call `atlas.mark_clean()` (needs mutable atlas reference)
- Update `render()` to accept the new `FrameGeometry` with text data:
  - After the solid-color draw call, set the text pipeline
  - Set bind group 0 (window uniform) and bind group 1 (atlas texture)
  - Upload text vertex/index buffers, draw indexed

**Tests (require GPU, `#[ignore]`):**
- TextVertex buffer layout matches expected stride (32 bytes) and attributes -- already tested in `text_vertex.rs`
- Shader compilation test: verify both shaders compile without errors (can be tested via `device.create_shader_module`)

### Unit 7: Wire everything together in `VeilApp`

**Location:** `crates/veil/src/main.rs`

Update `VeilApp`:
- Add fields: `font_pipeline: Option<FontPipeline>`, `terminal_map: TerminalMap`
- In `resumed()`:
  - Initialize `FontPipeline` with a hardcoded font path (or system default). For initial wiring, use the test font fixture or a configurable path. Log a warning if font loading fails and continue without text rendering.
  - Create `TerminalMap`
  - When spawning the initial PTY, also create a `GhosttyTerminalWriter` and insert it into the `TerminalMap`
- In `execute_effect()`:
  - For `SpawnPty`: also create a `GhosttyTerminalWriter` and insert into `TerminalMap`
  - For `ClosePty`: also remove from `TerminalMap`
- In the `RedrawRequested` handler:
  - Drain `StateUpdate` channel: for each `PtyOutput`, call `process_state_update()` on `terminal_map`
  - Call `build_frame_geometry()` with `&mut terminal_map` and `&mut font_pipeline`
  - If font_pipeline exists and atlas is dirty, call `renderer.upload_atlas()`
  - Call `renderer.render()` with the updated `FrameGeometry`

**Note on `TerminalMap` ownership:** The `TerminalMap` is currently `#[allow(dead_code)]` and not instantiated in `VeilApp`. It was designed in VEI-76 but deferred to this task for actual integration.

**Tests:**
- Integration-level tests are covered by the per-unit tests above. The main.rs wiring is glue code tested by running the app (E2E).
- Verify that the `TerminalMap` is created and populated during bootstrap (unit test in `bootstrap.rs`).

## Test Strategy Summary

| Unit | Test type | FFI required? | GPU required? |
|------|-----------|---------------|---------------|
| 1: Row/cell FFI | Unit + integration | Yes (`#[cfg(not(no_libghosty))]`) | No |
| 2: TerminalWriter extension | Unit (mock) | No | No |
| 3: Per-cell background quads | Unit (pure geometry) | No | No |
| 4: Font pipeline | Unit (file I/O) | No | No |
| 5: Text quad generation | Unit (pure geometry + mock CellGrid) | No | No |
| 6: Text render pipeline | Unit (`#[ignore]`, needs GPU) | No | Yes |
| 7: VeilApp wiring | E2E (run the app) | Yes | Yes |

## Acceptance Criteria

- [ ] Terminal text is visible in the window (shell prompt, typed commands, command output appear as rendered glyphs)
- [ ] Cell background colors match terminal escape sequences (ANSI colors: `\x1b[41m` for red bg, etc.)
- [ ] Cell foreground colors match terminal escape sequences (ANSI colors: `\x1b[31m` for red fg, etc.)
- [ ] Cursor is positioned at the correct cell (not hardcoded to 0,0)
- [ ] Cursor visibility respects terminal state (`\x1b[?25l` hides cursor)
- [ ] Text redraws when terminal state changes (typing updates the display)
- [ ] Empty cells show the terminal's default background color (not hardcoded gray)
- [ ] Multiple panes each show their own independent terminal content
- [ ] No regressions: existing tests in `frame.rs`, `quad_builder.rs`, `terminal_map.rs` continue to pass (tests that use hardcoded defaults need updating to use the new API)

## Dependencies

**Crate dependencies (already in Cargo.toml):**
- `swash` -- glyph rasterization (used by `font/rasterizer.rs`)
- `rustybuzz` -- text shaping (used by `font/shaper.rs`)
- `wgpu` -- GPU rendering
- `bytemuck` -- Pod/Zeroable for vertex types

**Build dependencies:**
- libghosty static library -- must include the row/cell iteration API symbols. Verify that the current vendored Ghostty commit exports `ghostty_render_state_row_iterator_new`, `ghostty_render_state_row_cells_new`, etc.

**Font fixture:**
- `crates/veil/test_fixtures/test_font.ttf` (Hack Regular) -- already exists, used by font pipeline tests. For runtime, a font path must be configured or discovered. Initial implementation can hardcode a well-known system font path or use the test fixture.

**No new crate dependencies are required.**

## Implementation Order

Units 1 and 4 are independent and can be implemented in parallel. Units 2 and 3 depend on Unit 1 (for `CellGrid` types). Unit 5 depends on Units 3 and 4. Unit 6 is independent of cell data but depends on Unit 4 for `TextVertex`. Unit 7 depends on all prior units.

Recommended sequence: 1 -> 2 -> 3 -> 4 (parallel with 1-3) -> 5 -> 6 -> 7.
