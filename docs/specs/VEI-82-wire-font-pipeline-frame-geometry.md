# VEI-82: Wire Font Pipeline + Terminal State into Frame Geometry (Text Quads)

## Context

`build_frame_geometry()` in `crates/veil/src/frame.rs` produces solid-color background quads (cell backgrounds, cursor, dividers, focus border) but never generates text vertex data. The `text_vertices` and `text_indices` fields on `FrameGeometry` are hardcoded to `Vec::new()` (lines 150-151) and annotated with `#[allow(dead_code)]` (lines 41-44). Meanwhile, the font pipeline (`crates/veil/src/font_pipeline.rs`) is complete and tested -- it handles glyph rasterization, atlas packing, and exposes `ensure_glyph()` to get atlas UV coordinates for any character. The `TextVertex` type and `text_quad_vertices()` function in `crates/veil/src/font/text_vertex.rs` are also complete and tested.

After VEI-81 (completed), PTY output flows through `TerminalMap` and `TerminalWriter::render_cells()` can return a `CellGrid` with per-cell content. This task wires the font pipeline and terminal state into the frame builder so that each visible character in the terminal produces a textured quad with correct atlas UV coordinates. It also replaces hardcoded `BG_COLOR` with real per-cell background colors from terminal state.

After this task, text quads exist in frame geometry but will not render visually on screen until the GPU pipeline supports a textured render pass (next issue). This is the CPU-side geometry generation half of text rendering.

### What exists

- **`FontPipeline`** (`crates/veil/src/font_pipeline.rs`) -- `new(config: &FontConfig) -> Result<Self>`, `ensure_glyph(ch: char) -> Option<AtlasRegion>`, `cell_width()`, `cell_height()`, `ascent()`, `atlas()`, `atlas_mut()`. Creates `FontData`, `Shaper`, `Rasterizer`, and `GlyphAtlas` (512x512). Fully tested.
- **`FontConfig`** (`crates/veil/src/font/loader.rs`) -- `path: Option<PathBuf>`, `size_pt: f32`, `dpi: f32`. Requires a font file path; `None` means "no bundled default" (returns error).
- **`TextVertex`** (`crates/veil/src/font/text_vertex.rs`) -- `position: [f32; 2]`, `uv: [f32; 2]`, `color: [f32; 4]`. 32 bytes, `Pod`/`Zeroable`. Has `buffer_layout()` for wgpu.
- **`text_quad_vertices(cell_x, cell_y, &AtlasRegion, ascent, color) -> [TextVertex; 4]`** -- Generates 4 vertices for a glyph quad using bearing offsets and atlas UVs.
- **`text_quad_indices(base: u16) -> [u16; 6]`** -- Generates index pattern for a text quad.
- **`AtlasRegion`** (`crates/veil/src/font/atlas.rs`) -- `u_min`, `v_min`, `u_max`, `v_max`, `width`, `height`, `bearing_x`, `bearing_y`. Zero-dimension entries for space/control chars.
- **`TerminalMap`** (`crates/veil/src/terminal_map.rs`) -- `get_mut(SurfaceId) -> Option<&mut Box<dyn TerminalWriter>>`. `TerminalWriter::render_cells() -> Option<CellGrid>`.
- **`CellGrid`** (`crates/veil-ghostty/src/render_state.rs`) -- `cols: u16`, `rows: u16`, `cells: Vec<Vec<CellData>>`, `cursor: CursorState`, `colors: RenderColors`.
- **`CellData`** -- `graphemes: Vec<char>`, `fg_color: Option<Color>`, `bg_color: Option<Color>`, `bold: bool`. Empty `graphemes` for blank cells.
- **`RenderColors`** -- `background: Color`, `foreground: Color`, `cursor: Option<Color>`.
- **`color_to_f32(Color) -> [f32; 4]`** (`crates/veil/src/quad_builder.rs`) -- Converts u8 RGB to normalized RGBA with alpha 1.0. Currently `#[allow(dead_code)]`.
- **`cell_fg_color(&CellData, default_fg: Color) -> [f32; 4]`** (`crates/veil/src/frame.rs`) -- Resolves cell fg color with fallback to default. Currently `#[allow(dead_code)]`.
- **`CellGridParams`** (`crates/veil/src/quad_builder.rs`) -- Already supports `cell_bg_colors: Option<Vec<Option<[f32; 4]>>>` for per-cell background colors.
- **`build_frame_geometry(app_state, focus, window_width, window_height) -> FrameGeometry`** -- Uses hardcoded `DEFAULT_COLS`/`DEFAULT_ROWS` (80x24) and `BG_COLOR` for all cells.
- **`VeilApp`** (`crates/veil/src/main.rs`) -- Owns `terminal_map: TerminalMap`, `app_state`, `focus`, `renderer`. `mod font` and `mod font_pipeline` are annotated with `#[allow(dead_code)]` (lines 15-17).
- **Test font fixture** at `crates/veil/test_fixtures/test_font.ttf` (Hack Regular, MIT license).

### What's missing

1. **`FontPipeline` not instantiated** -- No code creates a `FontPipeline` or stores it in `VeilApp`. The `mod font` and `mod font_pipeline` declarations have `#[allow(dead_code)]`.
2. **`build_frame_geometry()` ignores terminal state** -- Does not accept `TerminalMap` or `FontPipeline`. Uses hardcoded `DEFAULT_COLS`/`DEFAULT_ROWS` and `BG_COLOR` instead of reading `CellGrid` data.
3. **No text quad generation** -- `text_vertices` and `text_indices` always `Vec::new()`. No code calls `ensure_glyph()` or `text_quad_vertices()` from `build_frame_geometry()`.
4. **Cell backgrounds hardcoded** -- All cells use `BG_COLOR = [0.1, 0.1, 0.1, 1.0]` instead of reading `cell.bg_color` from `CellData`.
5. **`cell_fg_color()` and `color_to_f32()` marked dead** -- Both are implemented and tested but annotated `#[allow(dead_code)]` because nothing calls them from non-test code.

## Implementation Units

### Unit 1: Instantiate FontPipeline in VeilApp

**Location:** `crates/veil/src/main.rs`

**What it does:**

Adds a `font_pipeline: Option<FontPipeline>` field to `VeilApp` and creates it during `resumed()` after the renderer is created. Removes `#[allow(dead_code)]` from `mod font` and `mod font_pipeline`. The font pipeline does not require the wgpu device for construction (it is purely CPU-side atlas packing), so it can be created independently of the renderer. The wgpu device will be needed later when uploading the atlas texture to the GPU (separate issue).

**Changes:**

1. In `main.rs`, remove `#[allow(dead_code)]` from lines 15-16:
   ```rust
   // Before:
   #[allow(dead_code)]
   mod font;
   #[allow(dead_code)]
   mod font_pipeline;

   // After:
   mod font;
   mod font_pipeline;
   ```

2. Add `use crate::font_pipeline::FontPipeline;` import.

3. Add field to `VeilApp`:
   ```rust
   font_pipeline: Option<FontPipeline>,
   ```

4. Initialize to `None` in `VeilApp::new()`.

5. In `resumed()`, after the renderer is created, create the font pipeline:
   ```rust
   let font_pipeline = {
       let config = crate::font::loader::FontConfig {
           path: crate::font::loader::find_default_font(),
           size_pt: self.app_config.terminal.font_size,
           dpi: 96.0, // TODO: query actual DPI from window
       };
       match FontPipeline::new(&config) {
           Ok(pipeline) => {
               tracing::info!(
                   cell_width = pipeline.cell_width(),
                   cell_height = pipeline.cell_height(),
                   "font pipeline initialized"
               );
               Some(pipeline)
           }
           Err(e) => {
               tracing::warn!("font pipeline init failed, text rendering disabled: {e}");
               None
           }
       }
   };
   self.font_pipeline = font_pipeline;
   ```

   **Note on font path:** The `FontConfig.path` currently requires a `Some(PathBuf)`. Since no `find_default_font()` function exists yet, the implementation should use the test font fixture path as a temporary default, or try to discover a system monospace font. The simplest approach for now: use the bundled test font (`test_fixtures/test_font.ttf`). This can be enhanced later when font configuration is fully wired (VEI-17 config system already has `terminal.font_family` and `terminal.font_size` fields).

   If the font path is not available, `FontPipeline::new()` returns `Err` and `self.font_pipeline` stays `None`. Frame building gracefully falls back to the current behavior (no text quads).

**Test strategy:**

This unit is pure wiring -- adding a field and creating it in `resumed()`. Testing is via:

- **Compile-time:** Removing `#[allow(dead_code)]` from `mod font` and `mod font_pipeline` means the compiler verifies these modules are used. `cargo clippy -- -D warnings` catches any remaining dead code.
- **Existing `font_pipeline.rs` tests pass:** All 14 tests covering `FontPipeline::new()`, `ensure_glyph()`, cell metrics, atlas behavior.
- **`cargo test --workspace` passes** -- No regressions.

### Unit 2: Pass terminal state and font pipeline to build_frame_geometry

**Location:** `crates/veil/src/frame.rs`, `crates/veil/src/main.rs`

**What it does:**

Updates `build_frame_geometry()` to accept `&mut TerminalMap` and `Option<&mut FontPipeline>` parameters. When a font pipeline is available, for each visible pane: queries `terminal_map.get_mut(surface_id)` to get the `TerminalWriter`, calls `render_cells()` to get a `CellGrid`, and uses the cell data for two purposes:

1. **Per-cell background colors:** Reads `cell.bg_color` from each `CellData` and passes the colors to `build_cell_background_quads()` via its existing `cell_bg_colors` parameter.
2. **Text quads:** For each cell with a non-empty `graphemes`, calls `font_pipeline.ensure_glyph()` to get the atlas region, then `text_quad_vertices()` to generate 4 `TextVertex` vertices, and `text_quad_indices()` to generate 6 indices.

When the font pipeline is `None` or `render_cells()` returns `None` for a surface, the function falls back to the current behavior (hardcoded `DEFAULT_COLS`/`DEFAULT_ROWS`, `BG_COLOR`, empty text arrays).

**Changes:**

1. Update `build_frame_geometry()` signature:
   ```rust
   pub fn build_frame_geometry(
       app_state: &AppState,
       focus: &FocusManager,
       window_width: u32,
       window_height: u32,
       terminal_map: &mut TerminalMap,
       font_pipeline: Option<&mut FontPipeline>,
   ) -> FrameGeometry
   ```

2. Add imports at top of `frame.rs`:
   ```rust
   use crate::font::text_vertex::{text_quad_vertices, text_quad_indices, TextVertex};
   use crate::font_pipeline::FontPipeline;
   use crate::terminal_map::TerminalMap;
   use crate::quad_builder::color_to_f32;
   ```
   (Note: `TextVertex` import already exists; `text_quad_vertices` and `text_quad_indices` need adding.)

3. Inside the pane loop, replace the hardcoded `CellGridParams` with grid data from terminal state:
   ```rust
   let mut all_text_vertices: Vec<TextVertex> = Vec::new();
   let mut all_text_indices: Vec<u16> = Vec::new();

   for pl in &pane_layouts {
       // Try to get terminal cell data for this surface.
       let cell_grid = terminal_map
           .get_mut(pl.surface_id)
           .and_then(|tw| tw.render_cells());

       let (cols, rows, cell_bg_colors) = if let Some(ref grid) = cell_grid {
           let bg_colors: Vec<Option<[f32; 4]>> = grid.cells.iter().flat_map(|row| {
               row.iter().map(|cell| cell.bg_color.map(color_to_f32))
           }).collect();
           (grid.cols, grid.rows, Some(bg_colors))
       } else {
           (DEFAULT_COLS, DEFAULT_ROWS, None)
       };

       let default_bg = cell_grid.as_ref()
           .map(|g| color_to_f32(g.colors.background))
           .unwrap_or(BG_COLOR);

       let params = CellGridParams {
           rect: pl.rect,
           cols,
           rows,
           bg_color: default_bg,
           cell_bg_colors,
       };
       let (verts, indices) = build_cell_background_quads(&params);
       append(&verts, &indices);

       // Generate text quads if font pipeline and cell data are available.
       if let (Some(ref mut pipeline), Some(ref grid)) = (&mut font_pipeline, &cell_grid) {
           let cell_width = pl.rect.width / f32::from(cols);
           let cell_height = pl.rect.height / f32::from(rows);
           let ascent = pipeline.ascent();
           let default_fg = grid.colors.foreground;

           for (row_idx, row) in grid.cells.iter().enumerate() {
               for (col_idx, cell) in row.iter().enumerate() {
                   if cell.graphemes.is_empty() {
                       continue;
                   }
                   let ch = cell.graphemes[0]; // First char of grapheme cluster
                   if ch == ' ' || ch.is_control() {
                       continue; // Skip spaces and control characters
                   }

                   if let Some(region) = pipeline.ensure_glyph(ch) {
                       if region.width == 0 || region.height == 0 {
                           continue; // Skip zero-dimension glyphs
                       }

                       let cell_x = pl.rect.x + col_idx as f32 * cell_width;
                       let cell_y = pl.rect.y + row_idx as f32 * cell_height;
                       let fg_color = cell_fg_color(cell, default_fg);

                       let verts = text_quad_vertices(cell_x, cell_y, &region, ascent, fg_color);
                       let base = all_text_vertices.len() as u16;
                       let indices = text_quad_indices(base);

                       all_text_vertices.extend_from_slice(&verts);
                       all_text_indices.extend_from_slice(&indices);
                   }
               }
           }
       }
   }
   ```

   **Important:** The `font_pipeline` parameter is `Option<&mut FontPipeline>` because `ensure_glyph()` takes `&mut self` (it mutates the atlas). However, this creates a borrowing challenge since the function iterates over `pane_layouts` while also needing the mutable pipeline. Since `font_pipeline` is a parameter (not borrowed from the same struct), there is no conflict.

4. Update the return to use the generated text data:
   ```rust
   FrameGeometry {
       vertices: all_vertices,
       indices: all_indices,
       text_vertices: all_text_vertices,
       text_indices: all_text_indices,
       clear_color: CLEAR_COLOR,
   }
   ```

5. Use real cursor position from `CellGrid` when building cursor quads:
   ```rust
   if let Some(focused_surface) = focus.focused_surface() {
       if let Some(pl) = pane_layouts.iter().find(|pl| pl.surface_id == focused_surface) {
           let cell_grid = terminal_map
               .get_mut(pl.surface_id)
               .and_then(|tw| tw.render_cells());

           let (cols, rows, cursor_col, cursor_row, cursor_visible) =
               if let Some(ref grid) = cell_grid {
                   (grid.cols, grid.rows, grid.cursor.x, grid.cursor.y, grid.cursor.visible)
               } else {
                   (DEFAULT_COLS, DEFAULT_ROWS, 0, 0, true)
               };

           if cursor_visible {
               let (cursor_verts, cursor_indices) =
                   build_cursor_quad(&pl.rect, cols, rows, cursor_col, cursor_row, CURSOR_COLOR);
               append(&cursor_verts, &cursor_indices);
           }

           let (border_verts, border_indices) =
               build_focus_border(&pl.rect, FOCUS_BORDER_THICKNESS, FOCUS_BORDER_COLOR);
           append(&border_verts, &border_indices);
       }
   }
   ```

   **Note:** Calling `render_cells()` twice (once in the pane loop, once for cursor) is wasteful. A better approach is to cache the `CellGrid` results from the pane loop and look up the focused surface's grid. This avoids a second `render_cells()` call. Implementation detail: collect `CellGrid` results into a `HashMap<SurfaceId, CellGrid>` before building geometry, or restructure the loop.

6. Update the call site in `main.rs` `handle_redraw()`:
   ```rust
   let frame_geometry = build_frame_geometry(
       &self.app_state,
       &self.focus,
       self.window_size.0,
       self.window_size.1,
       &mut self.terminal_map,
       self.font_pipeline.as_mut(),
   );
   ```

7. Remove `#[allow(dead_code)]` from:
   - `text_vertices` and `text_indices` fields in `FrameGeometry` (lines 41-44)
   - `cell_fg_color()` function (line 53)
   - `color_to_f32()` in `quad_builder.rs` (line 36)

**Test strategy:**

Tests for this unit focus on verifying that `build_frame_geometry()` populates text fields correctly given mock terminal data and a real font pipeline.

**Happy path:**
- **`frame_geometry_with_terminal_content_has_text_quads`**: Create `TerminalMap` with a `CellGridMockWriter` containing "Hello" in cells. Create `FontPipeline` from the test fixture font. Call `build_frame_geometry()` with both. Assert `text_vertices` is non-empty, `text_indices` is non-empty, `text_vertices.len() == 5 * 4` (5 visible chars, 4 verts each), `text_indices.len() == 5 * 6`.
- **`frame_geometry_text_quads_have_correct_uv`**: For a single character 'A', verify that the text vertices have UV coordinates matching what `font_pipeline.ensure_glyph('A')` returns.
- **`frame_geometry_text_quads_use_cell_fg_color`**: Create a cell with `fg_color: Some(Color { r: 255, g: 0, b: 0 })`. Verify the text quad vertices have color `[1.0, 0.0, 0.0, 1.0]`.
- **`frame_geometry_text_quads_use_default_fg_when_none`**: Create a cell with `fg_color: None` and `RenderColors.foreground = Color { r: 200, g: 200, b: 200 }`. Verify text quad color uses the default.
- **`frame_geometry_cell_bg_from_terminal_state`**: Create cells with `bg_color: Some(Color { r: 255, g: 0, b: 0 })`. Verify background quad vertices use the cell's color, not `BG_COLOR`.
- **`frame_geometry_uses_grid_dimensions`**: Create a `CellGrid` with `cols: 40, rows: 12`. Verify the geometry uses 40x12 grid dimensions (not the default 80x24) by checking vertex count or positions.

**Edge cases:**
- **`frame_geometry_no_font_pipeline_empty_text`**: Pass `font_pipeline: None`. Assert `text_vertices` and `text_indices` are empty (graceful fallback).
- **`frame_geometry_no_terminal_data_empty_text`**: `TerminalMap` has no entry for the surface. Assert text quads are empty, background quads use `DEFAULT_COLS`/`DEFAULT_ROWS` and `BG_COLOR`.
- **`frame_geometry_render_cells_returns_none_fallback`**: `TerminalWriter::render_cells()` returns `None`. Assert fallback to defaults.
- **`frame_geometry_empty_graphemes_skipped`**: Cells with empty `graphemes` produce no text quads.
- **`frame_geometry_space_chars_skipped`**: Cells containing only `' '` produce no text quads (space has zero-dimension region).
- **`frame_geometry_control_chars_skipped`**: Cells containing control characters (`'\0'`, `'\n'`) produce no text quads.
- **`frame_geometry_cursor_position_from_grid`**: When terminal state provides cursor at (5, 3), verify cursor quad is positioned at column 5, row 3 (not at 0, 0).
- **`frame_geometry_cursor_hidden_no_cursor_quad`**: When `cursor.visible == false`, no cursor quad is generated.

**Backward compatibility:**
- **All existing frame.rs tests updated**: The existing tests call `build_frame_geometry()` with the old 4-parameter signature. They need to be updated to pass `&mut TerminalMap::new()` and `None` for the font pipeline. With an empty terminal map and no font pipeline, behavior should be identical to before.

### Unit 3: Remove dead_code annotations

**Location:** `crates/veil/src/main.rs`, `crates/veil/src/frame.rs`, `crates/veil/src/quad_builder.rs`

**What it does:**

Removes `#[allow(dead_code)]` from items that are now used by the frame builder integration. Also removes the dead_code annotation from `TerminalMap` methods that are now called by `build_frame_geometry()`.

**Changes:**

1. `main.rs` lines 15-17: Remove `#[allow(dead_code)]` from `mod font` and `mod font_pipeline`.

2. `frame.rs` lines 41-44: Remove `#[allow(dead_code)]` from `text_vertices` and `text_indices` fields.

3. `frame.rs` line 53: Remove `#[allow(dead_code)]` from `cell_fg_color()`.

4. `quad_builder.rs` line 36: Remove `#[allow(dead_code)]` from `color_to_f32()`.

5. `terminal_map.rs`: Remove `#[allow(dead_code)]` from `get_mut()` (line 164) since it is now called from `build_frame_geometry()`.

6. Review remaining `#[allow(dead_code)]` in `terminal_map.rs` and remove any that are now reachable.

**Test strategy:**

- **Compile-time:** `cargo clippy --all-targets --all-features -- -D warnings` passes without dead_code warnings for any items that are now used.
- **No logic changes** -- this is purely annotation cleanup. All existing tests pass unchanged.

## Acceptance Criteria

1. `build_frame_geometry()` returns populated `text_vertices` and `text_indices` when a `FontPipeline` is available and terminal state has visible characters.
2. Each visible character in the terminal produces a textured quad (4 `TextVertex` vertices, 6 indices) with correct atlas UV coordinates from `FontPipeline::ensure_glyph()`.
3. Text quad foreground colors come from `CellData.fg_color` (with fallback to `RenderColors.foreground`).
4. Cell background colors come from `CellData.bg_color` (with fallback to `RenderColors.background`), not the hardcoded `BG_COLOR` constant.
5. Grid dimensions (cols, rows) come from `CellGrid` when terminal state is available, not the hardcoded `DEFAULT_COLS`/`DEFAULT_ROWS`.
6. Cursor position comes from `CellGrid.cursor.x`/`cursor.y` when terminal state is available.
7. When `FontPipeline` is `None` or terminal state is unavailable, behavior gracefully falls back to the pre-existing behavior (empty text quads, default colors, default grid).
8. `FontPipeline` is stored in `VeilApp` and initialized in `resumed()`.
9. `#[allow(dead_code)]` is removed from `mod font`, `mod font_pipeline`, `text_vertices`, `text_indices`, `cell_fg_color()`, and `color_to_f32()`.
10. All existing tests pass (`cargo test --workspace`).
11. `cargo clippy --all-targets --all-features -- -D warnings` passes.

## Dependencies

- **`crates/veil/test_fixtures/test_font.ttf`** -- Already exists (Hack Regular, MIT license). Used by `FontPipeline` tests and will be used by the new frame geometry tests.
- **`veil-ghostty` crate** -- `CellData`, `CellGrid`, `Color`, `RenderColors`, `CursorState` types. Already available, no changes needed.
- **`crate::font_pipeline`** -- `FontPipeline`, `FontConfig`. Already implemented and tested. No changes needed.
- **`crate::font::text_vertex`** -- `TextVertex`, `text_quad_vertices()`, `text_quad_indices()`. Already implemented and tested. No changes needed.
- **`crate::font::atlas`** -- `AtlasRegion`. Already implemented. No changes needed.
- **`crate::font::loader`** -- `FontConfig`, `FontData`. Already implemented. No changes needed.
- **`crate::quad_builder`** -- `CellGridParams`, `color_to_f32()`, `build_cell_background_quads()`. Already implemented. No changes needed.
- **`crate::terminal_map`** -- `TerminalMap`, `TerminalWriter`. Already implemented. No changes needed.
- **No new external crate dependencies are required.**
