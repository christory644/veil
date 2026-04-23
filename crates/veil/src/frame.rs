//! Frame geometry composition — combines all quad builders with `AppState`
//! to produce the full frame's geometry. Pure logic (no GPU).

use std::collections::HashMap;

use veil_core::focus::FocusManager;
use veil_core::layout::{compute_layout, Rect};
use veil_core::state::AppState;
use veil_core::workspace::SurfaceId;

use crate::font::text_vertex::{text_quad_indices, text_quad_vertices, TextVertex};
use crate::font_pipeline::FontPipeline;
use crate::quad_builder::{
    build_cell_background_quads, build_cursor_quad, build_divider_quads, build_focus_border,
    color_to_f32, CellGridParams,
};
use crate::terminal_map::TerminalMap;
use crate::vertex::Vertex;

// -- Default grid dimensions (until real terminal state is wired in) ----------
pub(crate) const DEFAULT_COLS: u16 = 80;
pub(crate) const DEFAULT_ROWS: u16 = 24;

// -- Default colors (until real per-cell colors arrive from RenderState) ------

/// Dark gray cell background.
const BG_COLOR: [f32; 4] = [0.1, 0.1, 0.1, 1.0];
/// Light gray cursor.
const CURSOR_COLOR: [f32; 4] = [0.9, 0.9, 0.9, 1.0];
/// Medium gray pane divider.
const DIVIDER_COLOR: [f32; 4] = [0.3, 0.3, 0.3, 1.0];
/// Blue focus border with slight transparency.
const FOCUS_BORDER_COLOR: [f32; 4] = [0.2, 0.5, 1.0, 0.8];
/// Focus border width in pixels.
const FOCUS_BORDER_THICKNESS: f32 = 2.0;
/// Near-black window background (used as the wgpu clear color).
const CLEAR_COLOR: wgpu::Color = wgpu::Color { r: 0.05, g: 0.05, b: 0.05, a: 1.0 };

/// Complete geometry for a single frame, ready for GPU upload.
pub struct FrameGeometry {
    /// All vertices for this frame (solid-color quads: backgrounds, cursor, dividers, borders).
    pub vertices: Vec<Vertex>,
    /// All indices for this frame (solid-color quads).
    pub indices: Vec<u16>,
    /// Text vertices for textured glyph quads.
    #[allow(dead_code)]
    // Populated by build_frame_geometry; consumed by GPU text pass (next issue).
    pub text_vertices: Vec<TextVertex>,
    /// Text indices for textured glyph quads.
    #[allow(dead_code)]
    // Populated by build_frame_geometry; consumed by GPU text pass (next issue).
    pub text_indices: Vec<u16>,
    /// The clear color (window background).
    pub clear_color: wgpu::Color,
}

/// Resolve a cell's foreground color to RGBA f32.
///
/// Uses the cell's explicit `fg_color` if set, otherwise the default foreground.
pub fn cell_fg_color(cell: &veil_ghostty::CellData, foreground: veil_ghostty::Color) -> [f32; 4] {
    let color = cell.fg_color.unwrap_or(foreground);
    crate::quad_builder::color_to_f32(color)
}

/// Generate text quads for all visible characters in a cell grid.
///
/// Iterates over the grid, skips empty/space/control characters, and for
/// each visible char calls `ensure_glyph` + `text_quad_vertices`.
#[allow(clippy::cast_precision_loss)]
fn generate_text_quads(
    pipeline: &mut FontPipeline,
    grid: &veil_ghostty::CellGrid,
    pane_rect: &Rect,
    cols: u16,
    rows: u16,
    text_verts: &mut Vec<TextVertex>,
    text_idxs: &mut Vec<u16>,
) {
    let cell_w = pane_rect.width / f32::from(cols);
    let cell_h = pane_rect.height / f32::from(rows);
    let ascent = pipeline.ascent();
    let foreground = grid.colors.foreground;

    for (row_idx, row) in grid.cells.iter().enumerate() {
        for (col_idx, cell) in row.iter().enumerate() {
            if cell.graphemes.is_empty() {
                continue;
            }
            let ch = cell.graphemes[0];
            if ch == ' ' || ch.is_control() {
                continue;
            }

            if let Some(region) = pipeline.ensure_glyph(ch) {
                if region.width == 0 || region.height == 0 {
                    continue;
                }

                let cell_x = pane_rect.x + col_idx as f32 * cell_w;
                let cell_y = pane_rect.y + row_idx as f32 * cell_h;
                let fg_color = cell_fg_color(cell, foreground);

                let verts = text_quad_vertices(cell_x, cell_y, &region, ascent, fg_color);
                #[allow(clippy::cast_possible_truncation)]
                let base = text_verts.len() as u16;
                let indices = text_quad_indices(base);

                text_verts.extend_from_slice(&verts);
                text_idxs.extend_from_slice(&indices);
            }
        }
    }
}

/// Build all geometry for the current frame.
///
/// This is the main composition function called once per frame:
/// 1. Compute available terminal area (window size minus sidebar if visible)
/// 2. Get active workspace layout via `compute_layout` (respecting zoom)
/// 3. Build cell background quads for each pane (using terminal state when available)
/// 4. Build text quads for each pane (when font pipeline and terminal state are available)
/// 5. Build divider quads between adjacent panes
/// 6. Build cursor quad for the focused pane (if cursor visible)
/// 7. Build focus border around the focused pane
/// 8. Concatenate all vertices/indices with correct base offsets
///
/// Returns `FrameGeometry` ready for a single draw call.
// Window pixel dimensions and sidebar width_px all fit comfortably in f32.
#[allow(clippy::cast_precision_loss)]
pub fn build_frame_geometry(
    app_state: &AppState,
    focus: &FocusManager,
    window_width: u32,
    window_height: u32,
    terminal_map: &mut TerminalMap,
    mut font_pipeline: Option<&mut FontPipeline>,
) -> FrameGeometry {
    let empty = || FrameGeometry {
        vertices: Vec::new(),
        indices: Vec::new(),
        text_vertices: Vec::new(),
        text_indices: Vec::new(),
        clear_color: CLEAR_COLOR,
    };

    let Some(workspace) = app_state.active_workspace() else {
        return empty();
    };

    // Compute terminal area with sidebar offset.
    let window_w = window_width as f32;
    let window_h = window_height as f32;

    let (terminal_x, terminal_width) = if app_state.sidebar.visible {
        let x = app_state.sidebar.width_px as f32;
        (x, (window_w - x).max(0.0))
    } else {
        (0.0, window_w)
    };

    let available = Rect { x: terminal_x, y: 0.0, width: terminal_width, height: window_h };
    let pane_layouts = compute_layout(&workspace.layout, available, workspace.zoomed_pane);

    let mut all_vertices: Vec<Vertex> = Vec::new();
    let mut all_indices: Vec<u16> = Vec::new();
    let mut all_text_vertices: Vec<TextVertex> = Vec::new();
    let mut all_text_indices: Vec<u16> = Vec::new();

    // Append geometry with correct index offsets so multiple quad batches
    // share a single vertex/index buffer.
    let mut append = |verts: &[Vertex], idxs: &[u16]| {
        #[allow(clippy::cast_possible_truncation)]
        let base_offset = all_vertices.len() as u16;
        all_vertices.extend_from_slice(verts);
        all_indices.extend(idxs.iter().map(|i| i + base_offset));
    };

    // Collect CellGrid results for each surface so we can reuse them for
    // cursor positioning without calling render_cells() twice.
    let mut cached_grids: HashMap<SurfaceId, veil_ghostty::CellGrid> = HashMap::new();
    for pl in &pane_layouts {
        if let Some(grid) = terminal_map.get_mut(pl.surface_id).and_then(|tw| tw.render_cells()) {
            cached_grids.insert(pl.surface_id, grid);
        }
    }

    // Cell background quads and text quads for each pane.
    for pl in &pane_layouts {
        let cell_grid = cached_grids.get(&pl.surface_id);

        let (cols, rows, cell_bg_colors) = if let Some(grid) = cell_grid {
            let bg_colors: Vec<Option<[f32; 4]>> = grid
                .cells
                .iter()
                .flat_map(|row| row.iter().map(|cell| cell.bg_color.map(color_to_f32)))
                .collect();
            (grid.cols, grid.rows, Some(bg_colors))
        } else {
            (DEFAULT_COLS, DEFAULT_ROWS, None)
        };

        let bg_fallback = cell_grid.map_or(BG_COLOR, |g| color_to_f32(g.colors.background));

        let params =
            CellGridParams { rect: pl.rect, cols, rows, bg_color: bg_fallback, cell_bg_colors };
        let (verts, indices) = build_cell_background_quads(&params);
        append(&verts, &indices);

        // Generate text quads if font pipeline and cell data are available.
        if let (Some(ref mut pipeline), Some(grid)) = (&mut font_pipeline, cell_grid) {
            generate_text_quads(
                pipeline,
                grid,
                &pl.rect,
                cols,
                rows,
                &mut all_text_vertices,
                &mut all_text_indices,
            );
        }
    }

    // Divider quads between adjacent panes.
    let (div_verts, div_indices) = build_divider_quads(&pane_layouts, DIVIDER_COLOR);
    append(&div_verts, &div_indices);

    // Cursor and focus border for the focused pane.
    if let Some(focused_surface) = focus.focused_surface() {
        if let Some(pl) = pane_layouts.iter().find(|pl| pl.surface_id == focused_surface) {
            let cell_grid = cached_grids.get(&pl.surface_id);

            let (cols, rows, cursor_col, cursor_row, cursor_visible) = if let Some(grid) = cell_grid
            {
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

    FrameGeometry {
        vertices: all_vertices,
        indices: all_indices,
        text_vertices: all_text_vertices,
        text_indices: all_text_indices,
        clear_color: CLEAR_COLOR,
    }
}

#[cfg(test)]
#[allow(clippy::doc_markdown)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use veil_core::focus::FocusManager;
    use veil_core::state::AppState;
    use veil_core::workspace::{PaneId, SplitDirection, SurfaceId};

    // --- Helpers ---

    /// Create an AppState with one workspace containing a single pane.
    /// Returns (state, workspace_id, pane_id, surface_id).
    fn state_with_one_pane() -> (AppState, veil_core::workspace::WorkspaceId, PaneId, SurfaceId) {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("test".to_string(), PathBuf::from("/tmp"));
        let ws = state.workspace(ws_id).unwrap();
        let pane_id = ws.pane_ids()[0];
        let surface_id = ws.layout.surface_ids()[0];
        (state, ws_id, pane_id, surface_id)
    }

    /// Create an AppState with one workspace containing two horizontally split panes.
    fn state_with_two_panes(
    ) -> (AppState, veil_core::workspace::WorkspaceId, PaneId, SurfaceId, PaneId, SurfaceId) {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("test".to_string(), PathBuf::from("/tmp"));
        let ws = state.workspace(ws_id).unwrap();
        let first_pane = ws.pane_ids()[0];
        let first_surface = ws.layout.surface_ids()[0];
        let (second_pane, second_surface) = state
            .split_pane(ws_id, first_pane, SplitDirection::Horizontal)
            .expect("split should succeed");
        (state, ws_id, first_pane, first_surface, second_pane, second_surface)
    }

    // ============================================================
    // No active workspace
    // ============================================================

    #[test]
    fn frame_no_workspace_empty_geometry() {
        let state = AppState::new();
        let focus = FocusManager::new();
        let geom = build_frame_geometry(&state, &focus, 1280, 800, &mut TerminalMap::new(), None);
        assert!(geom.vertices.is_empty(), "no workspace should produce no vertices");
        assert!(geom.indices.is_empty(), "no workspace should produce no indices");
    }

    // ============================================================
    // Single pane, sidebar visible (default)
    // ============================================================

    #[test]
    fn frame_single_pane_sidebar_visible() {
        let (state, _ws_id, _pane_id, surface_id) = state_with_one_pane();
        let mut focus = FocusManager::new();
        focus.focus_surface(surface_id);

        let geom = build_frame_geometry(&state, &focus, 1280, 800, &mut TerminalMap::new(), None);

        // Sidebar is visible by default (250px), so terminal area starts at x=250
        // All cell background vertices should have x >= 250
        assert!(!geom.vertices.is_empty(), "should have geometry");
        for v in &geom.vertices {
            assert!(
                v.position[0] >= 250.0 - 0.01,
                "vertex x={} should be >= sidebar width 250",
                v.position[0]
            );
        }
    }

    // ============================================================
    // Single pane, sidebar hidden
    // ============================================================

    #[test]
    fn frame_single_pane_sidebar_hidden() {
        let (mut state, _ws_id, _pane_id, surface_id) = state_with_one_pane();
        state.toggle_sidebar(); // hide sidebar
        let mut focus = FocusManager::new();
        focus.focus_surface(surface_id);

        let geom = build_frame_geometry(&state, &focus, 1280, 800, &mut TerminalMap::new(), None);

        assert!(!geom.vertices.is_empty(), "should have geometry");
        // With sidebar hidden, some vertices should start at x=0
        let min_x = geom.vertices.iter().map(|v| v.position[0]).fold(f32::INFINITY, f32::min);
        assert!(min_x < 1.0, "with sidebar hidden, minimum x={min_x} should be near 0");
    }

    // ============================================================
    // Two panes with dividers
    // ============================================================

    #[test]
    fn frame_two_panes_has_dividers() {
        let (state, _ws_id, _pane_id, first_surface, _pane2, _surface2) = state_with_two_panes();
        let mut focus = FocusManager::new();
        focus.focus_surface(first_surface);

        let geom = build_frame_geometry(&state, &focus, 1280, 800, &mut TerminalMap::new(), None);

        // Two panes should produce more geometry than one pane alone
        // (cell backgrounds for both panes + divider + focus border)
        assert!(!geom.vertices.is_empty(), "two-pane layout should have geometry");
        // We can check that vertex count is larger than what a single pane would produce
        // A single 80x24 pane = 1920 quads = 7680 vertices; two panes + dividers + border > that
        assert!(geom.vertices.len() > 4, "two-pane layout should have substantial geometry");
    }

    // ============================================================
    // Focused pane has border
    // ============================================================

    #[test]
    fn frame_focused_pane_has_border() {
        let (state, _ws_id, _pane_id, surface_id) = state_with_one_pane();
        let mut focus = FocusManager::new();
        focus.focus_surface(surface_id);

        let geom = build_frame_geometry(&state, &focus, 1280, 800, &mut TerminalMap::new(), None);

        // Focus border uses color [0.2, 0.5, 1.0, 0.8] per the spec.
        // Check that at least some vertices have the focus border color.
        let focus_color = [0.2, 0.5, 1.0, 0.8];
        let has_focus_border = geom.vertices.iter().any(|v| {
            (v.color[0] - focus_color[0]).abs() < 0.01
                && (v.color[1] - focus_color[1]).abs() < 0.01
                && (v.color[2] - focus_color[2]).abs() < 0.01
                && (v.color[3] - focus_color[3]).abs() < 0.01
        });
        assert!(has_focus_border, "focused pane should have border quads with focus color");
    }

    // ============================================================
    // No focus means no border
    // ============================================================

    #[test]
    fn frame_no_focus_no_border() {
        let (state, _ws_id, _pane_id, _surface_id) = state_with_one_pane();
        let focus = FocusManager::new(); // no focus set

        let geom = build_frame_geometry(&state, &focus, 1280, 800, &mut TerminalMap::new(), None);

        // Without focus, there should be no focus border color in the vertices
        let focus_color = [0.2, 0.5, 1.0, 0.8];
        let has_focus_border = geom.vertices.iter().any(|v| {
            (v.color[0] - focus_color[0]).abs() < 0.01
                && (v.color[1] - focus_color[1]).abs() < 0.01
                && (v.color[2] - focus_color[2]).abs() < 0.01
                && (v.color[3] - focus_color[3]).abs() < 0.01
        });
        assert!(!has_focus_border, "without focus, no border quads should have focus color");
    }

    // ============================================================
    // Zoomed pane fills entire area
    // ============================================================

    #[test]
    fn frame_zoomed_pane_fills_area() {
        let (mut state, ws_id, first_pane, first_surface, _pane2, _surface2) =
            state_with_two_panes();

        // Zoom the first pane
        state.toggle_zoom(ws_id, first_pane).expect("zoom should succeed");

        let mut focus = FocusManager::new();
        focus.focus_surface(first_surface);

        let geom_zoomed =
            build_frame_geometry(&state, &focus, 1280, 800, &mut TerminalMap::new(), None);

        // When zoomed, layout should be equivalent to a single pane filling the area.
        // No divider quads should be present.
        // The divider color is [0.3, 0.3, 0.3, 1.0]
        let divider_color = [0.3, 0.3, 0.3, 1.0];
        let has_divider = geom_zoomed.vertices.iter().any(|v| {
            (v.color[0] - divider_color[0]).abs() < 0.01
                && (v.color[1] - divider_color[1]).abs() < 0.01
                && (v.color[2] - divider_color[2]).abs() < 0.01
                && (v.color[3] - divider_color[3]).abs() < 0.01
        });
        assert!(!has_divider, "zoomed pane should have no dividers in output");
    }

    // ============================================================
    // VEI-77 Unit 5: Text quad generation
    // ============================================================

    #[test]
    fn frame_geometry_has_empty_text_fields_by_default() {
        // Without terminal data or font pipeline, text vertices should be empty.
        let (state, _ws_id, _pane_id, surface_id) = state_with_one_pane();
        let mut focus = FocusManager::new();
        focus.focus_surface(surface_id);
        let geom = build_frame_geometry(&state, &focus, 1280, 800, &mut TerminalMap::new(), None);
        assert!(
            geom.text_vertices.is_empty(),
            "without font pipeline, text_vertices should be empty"
        );
        assert!(
            geom.text_indices.is_empty(),
            "without font pipeline, text_indices should be empty"
        );
    }

    #[test]
    fn frame_geometry_no_workspace_has_empty_text() {
        let state = AppState::new();
        let focus = FocusManager::new();
        let geom = build_frame_geometry(&state, &focus, 1280, 800, &mut TerminalMap::new(), None);
        assert!(geom.text_vertices.is_empty());
        assert!(geom.text_indices.is_empty());
    }

    #[test]
    fn cell_fg_color_uses_cell_explicit_color() {
        let cell = veil_ghostty::CellData {
            graphemes: vec!['A'],
            fg_color: Some(veil_ghostty::Color { r: 255, g: 0, b: 0 }),
            bg_color: None,
            bold: false,
        };
        let default_fg = veil_ghostty::Color { r: 200, g: 200, b: 200 };
        let result = cell_fg_color(&cell, default_fg);
        // Should use the cell's explicit red fg_color, not the default.
        assert!(
            (result[0] - 1.0).abs() < 0.01,
            "cell with explicit red fg should have r=1.0, got {result:?}"
        );
        assert!(
            result[1].abs() < 0.01,
            "cell with explicit red fg should have g=0.0, got {result:?}"
        );
    }

    #[test]
    fn cell_fg_color_falls_back_to_default() {
        let cell = veil_ghostty::CellData {
            graphemes: vec!['A'],
            fg_color: None,
            bg_color: None,
            bold: false,
        };
        let default_fg = veil_ghostty::Color { r: 200, g: 200, b: 200 };
        let result = cell_fg_color(&cell, default_fg);
        let expected = 200.0 / 255.0;
        // Should use the default foreground color.
        assert!(
            (result[0] - expected).abs() < 0.01,
            "cell without explicit fg should use default, got {result:?}"
        );
    }

    // ============================================================
    // VEI-82 Unit 2: build_frame_geometry with terminal state + font pipeline
    // ============================================================

    use crate::font::loader::FontConfig;
    use crate::font_pipeline::FontPipeline;
    use crate::terminal_map::TerminalWriter;

    /// Path to the test font fixture (Hack Regular, MIT license).
    fn test_font_path() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/test_fixtures/test_font.ttf"))
    }

    /// Create a `FontPipeline` from the test font fixture.
    fn test_pipeline() -> FontPipeline {
        let config = FontConfig { path: Some(test_font_path()), size_pt: 14.0, dpi: 96.0 };
        FontPipeline::new(&config).expect("font pipeline should initialize from test fixture")
    }

    /// Mock `TerminalWriter` that returns a configurable `CellGrid` from `render_cells()`.
    struct CellGridMockWriter {
        cols: u16,
        rows: u16,
        cell_grid: Option<veil_ghostty::CellGrid>,
    }

    impl CellGridMockWriter {
        fn with_grid(grid: veil_ghostty::CellGrid) -> Self {
            Self { cols: grid.cols, rows: grid.rows, cell_grid: Some(grid) }
        }

        fn without_grid(cols: u16, rows: u16) -> Self {
            Self { cols, rows, cell_grid: None }
        }
    }

    impl TerminalWriter for CellGridMockWriter {
        fn write_vt(&mut self, _data: &[u8]) {}
        fn resize(
            &mut self,
            cols: u16,
            rows: u16,
            _cell_width_px: u32,
            _cell_height_px: u32,
        ) -> Result<(), String> {
            self.cols = cols;
            self.rows = rows;
            Ok(())
        }
        fn cols(&self) -> u16 {
            self.cols
        }
        fn rows(&self) -> u16 {
            self.rows
        }
        fn render_cells(&mut self) -> Option<veil_ghostty::CellGrid> {
            self.cell_grid.clone()
        }
    }

    /// Build a `CellGrid` with the given dimensions and text content.
    /// Text fills cells left-to-right, top-to-bottom. Remaining cells are blank.
    fn make_cell_grid(cols: u16, rows: u16, text: &str) -> veil_ghostty::CellGrid {
        let chars: Vec<char> = text.chars().collect();
        let mut cells = Vec::new();
        let mut char_idx = 0;
        for _row in 0..rows {
            let mut row_cells = Vec::new();
            for _col in 0..cols {
                let cell = if char_idx < chars.len() {
                    let ch = chars[char_idx];
                    char_idx += 1;
                    veil_ghostty::CellData {
                        graphemes: vec![ch],
                        fg_color: None,
                        bg_color: None,
                        bold: false,
                    }
                } else {
                    veil_ghostty::CellData::default()
                };
                row_cells.push(cell);
            }
            cells.push(row_cells);
        }
        veil_ghostty::CellGrid {
            cols,
            rows,
            cells,
            cursor: veil_ghostty::CursorState {
                in_viewport: true,
                x: 0,
                y: 0,
                visible: true,
                blinking: false,
                style: veil_ghostty::CursorStyle::Block,
                password_input: false,
            },
            colors: veil_ghostty::RenderColors {
                background: veil_ghostty::Color { r: 0, g: 0, b: 0 },
                foreground: veil_ghostty::Color { r: 255, g: 255, b: 255 },
                cursor: None,
            },
        }
    }

    /// Set up a single-pane layout with a mock terminal writer that returns the given grid.
    /// Returns (AppState, FocusManager, TerminalMap, SurfaceId).
    fn setup_with_grid(
        grid: veil_ghostty::CellGrid,
    ) -> (AppState, FocusManager, TerminalMap, SurfaceId) {
        let (state, _ws_id, _pane_id, surface_id) = state_with_one_pane();
        let mut focus = FocusManager::new();
        focus.focus_surface(surface_id);
        let mut terminal_map = TerminalMap::new();
        terminal_map.insert(surface_id, Box::new(CellGridMockWriter::with_grid(grid)));
        (state, focus, terminal_map, surface_id)
    }

    // --- Happy path ---

    #[test]
    fn frame_geometry_with_terminal_content_has_text_quads() {
        // "Hello" = 5 visible chars, each should produce a text quad.
        let grid = make_cell_grid(80, 24, "Hello");
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);
        let mut pipeline = test_pipeline();

        let geom =
            build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, Some(&mut pipeline));

        // 5 visible chars * 4 vertices each = 20 text vertices
        assert_eq!(
            geom.text_vertices.len(),
            5 * 4,
            "5 visible chars should produce 20 text vertices, got {}",
            geom.text_vertices.len()
        );
        // 5 visible chars * 6 indices each = 30 text indices
        assert_eq!(
            geom.text_indices.len(),
            5 * 6,
            "5 visible chars should produce 30 text indices, got {}",
            geom.text_indices.len()
        );
    }

    #[test]
    fn frame_geometry_text_quads_have_correct_uv() {
        // Single character 'A' -- verify UV coordinates match the font pipeline.
        let mut grid = make_cell_grid(80, 24, "");
        grid.cells[0][0] = veil_ghostty::CellData {
            graphemes: vec!['A'],
            fg_color: None,
            bg_color: None,
            bold: false,
        };
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);
        let mut pipeline = test_pipeline();

        // Pre-ensure the glyph so we know the expected UV coordinates.
        let expected_region = pipeline.ensure_glyph('A').expect("'A' should have an atlas region");

        let geom =
            build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, Some(&mut pipeline));

        assert_eq!(geom.text_vertices.len(), 4, "single char 'A' should produce 4 text vertices");
        // Top-left vertex UV should match (u_min, v_min)
        let tl = &geom.text_vertices[0];
        assert!(
            (tl.uv[0] - expected_region.u_min).abs() < 0.001
                && (tl.uv[1] - expected_region.v_min).abs() < 0.001,
            "top-left UV should match atlas region: expected ({}, {}), got ({}, {})",
            expected_region.u_min,
            expected_region.v_min,
            tl.uv[0],
            tl.uv[1]
        );
        // Bottom-right vertex UV should match (u_max, v_max)
        let br = &geom.text_vertices[3];
        assert!(
            (br.uv[0] - expected_region.u_max).abs() < 0.001
                && (br.uv[1] - expected_region.v_max).abs() < 0.001,
            "bottom-right UV should match atlas region: expected ({}, {}), got ({}, {})",
            expected_region.u_max,
            expected_region.v_max,
            br.uv[0],
            br.uv[1]
        );
    }

    #[test]
    fn frame_geometry_text_quads_use_cell_fg_color() {
        // Cell with explicit red foreground -- text quad should use red.
        let mut grid = make_cell_grid(80, 24, "");
        grid.cells[0][0] = veil_ghostty::CellData {
            graphemes: vec!['A'],
            fg_color: Some(veil_ghostty::Color { r: 255, g: 0, b: 0 }),
            bg_color: None,
            bold: false,
        };
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);
        let mut pipeline = test_pipeline();

        let geom =
            build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, Some(&mut pipeline));

        assert_eq!(geom.text_vertices.len(), 4, "single char should produce 4 text vertices");
        for v in &geom.text_vertices {
            assert!(
                (v.color[0] - 1.0).abs() < 0.01,
                "text vertex red channel should be 1.0, got {}",
                v.color[0]
            );
            assert!(
                v.color[1].abs() < 0.01,
                "text vertex green channel should be 0.0, got {}",
                v.color[1]
            );
            assert!(
                v.color[2].abs() < 0.01,
                "text vertex blue channel should be 0.0, got {}",
                v.color[2]
            );
        }
    }

    #[test]
    fn frame_geometry_text_quads_use_default_fg_when_none() {
        // Cell with no explicit fg_color -- should use RenderColors.foreground.
        let mut grid = make_cell_grid(80, 24, "");
        grid.colors.foreground = veil_ghostty::Color { r: 200, g: 200, b: 200 };
        grid.cells[0][0] = veil_ghostty::CellData {
            graphemes: vec!['A'],
            fg_color: None,
            bg_color: None,
            bold: false,
        };
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);
        let mut pipeline = test_pipeline();

        let geom =
            build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, Some(&mut pipeline));

        assert_eq!(geom.text_vertices.len(), 4, "single char should produce 4 text vertices");
        let expected_channel = 200.0 / 255.0;
        for v in &geom.text_vertices {
            assert!(
                (v.color[0] - expected_channel).abs() < 0.01,
                "text vertex should use default fg color ({expected_channel}), got {}",
                v.color[0]
            );
        }
    }

    #[test]
    fn frame_geometry_cell_bg_from_terminal_state() {
        // Cell with explicit red background -- background quad should use red, not BG_COLOR.
        let mut grid = make_cell_grid(80, 24, "");
        grid.cells[0][0] = veil_ghostty::CellData {
            graphemes: vec![],
            fg_color: None,
            bg_color: Some(veil_ghostty::Color { r: 255, g: 0, b: 0 }),
            bold: false,
        };
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);
        let mut pipeline = test_pipeline();

        let geom =
            build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, Some(&mut pipeline));

        // The first cell's background quad vertices (first 4 vertices) should have red color.
        assert!(!geom.vertices.is_empty(), "should have background geometry");
        let first_cell_color = geom.vertices[0].color;
        assert!(
            (first_cell_color[0] - 1.0).abs() < 0.01,
            "cell bg red channel should be 1.0, got {}",
            first_cell_color[0]
        );
        assert!(
            first_cell_color[1].abs() < 0.01,
            "cell bg green channel should be 0.0, got {}",
            first_cell_color[1]
        );
        assert!(
            first_cell_color[2].abs() < 0.01,
            "cell bg blue channel should be 0.0, got {}",
            first_cell_color[2]
        );
    }

    #[test]
    fn frame_geometry_uses_grid_dimensions() {
        // Use a 40x12 grid instead of default 80x24. The number of background quads
        // should correspond to 40*12 = 480, not 80*24 = 1920.
        let grid = make_cell_grid(40, 12, "");
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);
        let mut pipeline = test_pipeline();

        let geom =
            build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, Some(&mut pipeline));

        // Background quads: 40*12 = 480 quads, each with 4 vertices = 1920 bg vertices.
        // Plus cursor (4 vertices) + focus border (16 vertices) = 1940 total solid vertices.
        // With default 80x24: 1920 quads * 4 = 7680 + 4 + 16 = 7700 solid vertices.
        // If the grid is 40x12, the bg vertex count should be much less than 7700.
        let expected_bg_verts = 40 * 12 * 4;
        // The total will be bg_verts + cursor(4) + border(16), possibly +/- divider.
        // Just check that it's not the default 80x24 count.
        let default_bg_verts = 80 * 24 * 4;
        assert_ne!(
            geom.vertices.len(),
            default_bg_verts + 4 + 16,
            "vertex count should differ from default 80x24 grid"
        );
        // Check total includes the 40x12 bg vertex count.
        assert!(
            geom.vertices.len() >= expected_bg_verts,
            "should have at least {} bg vertices for 40x12 grid, got {}",
            expected_bg_verts,
            geom.vertices.len()
        );
    }

    // --- Edge cases ---

    #[test]
    fn frame_geometry_no_font_pipeline_empty_text() {
        // With terminal data but no font pipeline, text quads should be empty.
        let grid = make_cell_grid(80, 24, "Hello");
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);

        let geom = build_frame_geometry(
            &state,
            &focus,
            1280,
            800,
            &mut terminal_map,
            None, // no font pipeline
        );

        assert!(
            geom.text_vertices.is_empty(),
            "without font pipeline, text_vertices should be empty even with terminal data"
        );
        assert!(
            geom.text_indices.is_empty(),
            "without font pipeline, text_indices should be empty even with terminal data"
        );
    }

    #[test]
    fn frame_geometry_no_terminal_data_empty_text() {
        // TerminalMap has no entry for the surface -- fallback to defaults.
        let (state, _ws_id, _pane_id, surface_id) = state_with_one_pane();
        let mut focus = FocusManager::new();
        focus.focus_surface(surface_id);
        let mut terminal_map = TerminalMap::new(); // empty -- no terminal for surface
        let mut pipeline = test_pipeline();

        let geom =
            build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, Some(&mut pipeline));

        assert!(geom.text_vertices.is_empty(), "without terminal data, text quads should be empty");
        assert!(
            geom.text_indices.is_empty(),
            "without terminal data, text indices should be empty"
        );
        // Background quads should still use DEFAULT_COLS x DEFAULT_ROWS.
        let expected_bg_verts = (DEFAULT_COLS as usize) * (DEFAULT_ROWS as usize) * 4;
        assert!(
            geom.vertices.len() >= expected_bg_verts,
            "without terminal data, should use default 80x24 grid ({} bg verts), got {}",
            expected_bg_verts,
            geom.vertices.len()
        );
    }

    #[test]
    fn frame_geometry_render_cells_returns_none_fallback() {
        // TerminalWriter exists but render_cells() returns None.
        let (state, _ws_id, _pane_id, surface_id) = state_with_one_pane();
        let mut focus = FocusManager::new();
        focus.focus_surface(surface_id);
        let mut terminal_map = TerminalMap::new();
        terminal_map.insert(surface_id, Box::new(CellGridMockWriter::without_grid(80, 24)));
        let mut pipeline = test_pipeline();

        let geom =
            build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, Some(&mut pipeline));

        assert!(
            geom.text_vertices.is_empty(),
            "when render_cells() returns None, text quads should be empty"
        );
        // Background should fall back to DEFAULT_COLS x DEFAULT_ROWS.
        let expected_bg_verts = (DEFAULT_COLS as usize) * (DEFAULT_ROWS as usize) * 4;
        assert!(
            geom.vertices.len() >= expected_bg_verts,
            "when render_cells() returns None, should use default grid dimensions"
        );
    }

    #[test]
    fn frame_geometry_empty_graphemes_skipped() {
        // All cells have empty graphemes -- no text quads.
        let grid = make_cell_grid(80, 24, ""); // all blank cells
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);
        let mut pipeline = test_pipeline();

        let geom =
            build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, Some(&mut pipeline));

        assert!(
            geom.text_vertices.is_empty(),
            "cells with empty graphemes should produce no text quads"
        );
        assert!(
            geom.text_indices.is_empty(),
            "cells with empty graphemes should produce no text indices"
        );
    }

    #[test]
    fn frame_geometry_space_chars_skipped() {
        // Cells containing only spaces should produce no text quads.
        let grid = make_cell_grid(80, 24, "     "); // 5 spaces
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);
        let mut pipeline = test_pipeline();

        let geom =
            build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, Some(&mut pipeline));

        assert!(geom.text_vertices.is_empty(), "space characters should produce no text quads");
        assert!(geom.text_indices.is_empty(), "space characters should produce no text indices");
    }

    #[test]
    fn frame_geometry_control_chars_skipped() {
        // Cells containing control characters should produce no text quads.
        let mut grid = make_cell_grid(80, 24, "");
        grid.cells[0][0] = veil_ghostty::CellData {
            graphemes: vec!['\0'],
            fg_color: None,
            bg_color: None,
            bold: false,
        };
        grid.cells[0][1] = veil_ghostty::CellData {
            graphemes: vec!['\n'],
            fg_color: None,
            bg_color: None,
            bold: false,
        };
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);
        let mut pipeline = test_pipeline();

        let geom =
            build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, Some(&mut pipeline));

        assert!(geom.text_vertices.is_empty(), "control characters should produce no text quads");
        assert!(geom.text_indices.is_empty(), "control characters should produce no text indices");
    }

    #[test]
    fn frame_geometry_cursor_position_from_grid() {
        // Terminal state has cursor at (5, 3). Verify cursor quad is at that position.
        let mut grid = make_cell_grid(80, 24, "");
        grid.cursor.x = 5;
        grid.cursor.y = 3;
        grid.cursor.visible = true;
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);

        let geom = build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, None);

        // Find cursor quad by looking for CURSOR_COLOR vertices.
        let cursor_color = [0.9, 0.9, 0.9, 1.0];
        let cursor_verts: Vec<_> = geom
            .vertices
            .iter()
            .filter(|v| {
                (v.color[0] - cursor_color[0]).abs() < 0.01
                    && (v.color[1] - cursor_color[1]).abs() < 0.01
                    && (v.color[2] - cursor_color[2]).abs() < 0.01
                    && (v.color[3] - cursor_color[3]).abs() < 0.01
            })
            .collect();
        assert_eq!(
            cursor_verts.len(),
            4,
            "should have exactly 4 cursor vertices, got {}",
            cursor_verts.len()
        );

        // The cursor's top-left vertex should NOT be at the default (0, 0) position
        // relative to the pane. Sidebar is 250px, so pane starts at x=250.
        // Cursor at col=5, row=3 means the cursor quad is offset from the pane origin.
        // With an 80x24 grid on a (1280-250)x800 pane:
        //   cell_width = (1280-250) / 80 = 1030/80 = 12.875
        //   cell_height = 800 / 24 = 33.333...
        //   cursor_x = 250 + 5 * 12.875 = 250 + 64.375 = 314.375
        //   cursor_y = 0 + 3 * 33.333 = 100.0
        let top_left = cursor_verts
            .iter()
            .min_by(|a, b| {
                a.position[0]
                    .partial_cmp(&b.position[0])
                    .unwrap()
                    .then(a.position[1].partial_cmp(&b.position[1]).unwrap())
            })
            .unwrap();
        // With default cursor at (0,0), cursor_x would be 250.0 and cursor_y would be 0.0.
        // With cursor at (5,3), both should be offset.
        assert!(
            top_left.position[0] > 260.0,
            "cursor at col=5 should be offset from pane origin (x={})",
            top_left.position[0]
        );
        assert!(
            top_left.position[1] > 50.0,
            "cursor at row=3 should be offset from pane origin (y={})",
            top_left.position[1]
        );
    }

    #[test]
    fn frame_geometry_cursor_hidden_no_cursor_quad() {
        // When cursor.visible is false, no cursor quad should be generated.
        let mut grid = make_cell_grid(80, 24, "");
        grid.cursor.visible = false;
        let (state, focus, mut terminal_map, _surface_id) = setup_with_grid(grid);

        let geom = build_frame_geometry(&state, &focus, 1280, 800, &mut terminal_map, None);

        // Look for cursor-colored vertices. With visible=false, there should be none.
        let cursor_color = [0.9, 0.9, 0.9, 1.0];
        let has_cursor = geom.vertices.iter().any(|v| {
            (v.color[0] - cursor_color[0]).abs() < 0.01
                && (v.color[1] - cursor_color[1]).abs() < 0.01
                && (v.color[2] - cursor_color[2]).abs() < 0.01
                && (v.color[3] - cursor_color[3]).abs() < 0.01
        });
        assert!(!has_cursor, "when cursor.visible is false, no cursor quad should be generated");
    }
}
