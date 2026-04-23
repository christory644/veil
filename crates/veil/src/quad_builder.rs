//! Quad builders -- cell backgrounds, cursor, dividers, focus border.
//!
//! Functions that consume layout geometry (from `veil-core`) and produce
//! `Vertex`/index arrays. Pure geometry, no GPU state. Fully unit-testable.

use veil_core::layout::{PaneLayout, Rect};

use crate::vertex::{quad_indices, quad_vertices, vertex_base, Vertex};

/// Maximum distance (in pixels) between two pane edges for them to be
/// considered adjacent and warrant a divider line.
const DIVIDER_EDGE_TOLERANCE: f32 = 1.0;

/// Width of a vertical divider line in pixels.
const DIVIDER_WIDTH_PX: f32 = 1.0;

/// Height of a horizontal divider line in pixels.
const DIVIDER_HEIGHT_PX: f32 = 1.0;

/// Parameters for building cell background quads for a pane.
pub struct CellGridParams {
    /// The pane's pixel rectangle (from `compute_layout`).
    pub rect: Rect,
    /// Number of columns in the terminal grid.
    pub cols: u16,
    /// Number of rows in the terminal grid.
    pub rows: u16,
    /// Background color as RGBA.
    pub bg_color: [f32; 4],
    /// Per-cell background colors. If provided, indexed as `row * cols + col`.
    /// Each entry is `None` (use `bg_color` default) or `Some(color)`.
    pub cell_bg_colors: Option<Vec<Option<[f32; 4]>>>,
}

/// Convert a `Color` (u8 RGB) to `[f32; 4]` (normalized RGBA with alpha 1.0).
#[allow(dead_code)] // Used by frame.rs cell_fg_color; wired into frame builder when text rendering is integrated.
pub fn color_to_f32(color: veil_ghostty::Color) -> [f32; 4] {
    [f32::from(color.r) / 255.0, f32::from(color.g) / 255.0, f32::from(color.b) / 255.0, 1.0]
}

/// Build cell background quads for a single pane.
///
/// Generates one quad per cell (`cols * rows` quads total). Each cell
/// is sized to evenly fill the pane rect. When `cell_bg_colors` is
/// provided, each cell uses its per-cell color (falling back to `bg_color`
/// for `None` entries or out-of-bounds indices).
///
/// Returns (vertices, indices) ready for GPU upload.
#[allow(clippy::cast_precision_loss)] // Grid indices (col/row) fit comfortably in f32.
pub fn build_cell_background_quads(params: &CellGridParams) -> (Vec<Vertex>, Vec<u16>) {
    if params.cols == 0 || params.rows == 0 {
        return (Vec::new(), Vec::new());
    }

    let cols = params.cols as usize;
    let rows = params.rows as usize;
    let total_quads = cols * rows;
    let cell_width = params.rect.width / f32::from(params.cols);
    let cell_height = params.rect.height / f32::from(params.rows);

    let mut vertices = Vec::with_capacity(total_quads * 4);
    let mut indices = Vec::with_capacity(total_quads * 6);

    for row in 0..rows {
        for col in 0..cols {
            let x = params.rect.x + col as f32 * cell_width;
            let y = params.rect.y + row as f32 * cell_height;
            let idx = row * cols + col;
            let color = params
                .cell_bg_colors
                .as_ref()
                .and_then(|colors| colors.get(idx).copied().flatten())
                .unwrap_or(params.bg_color);
            vertices.extend_from_slice(&quad_vertices(x, y, cell_width, cell_height, color));
            indices.extend_from_slice(&quad_indices(vertex_base(idx)));
        }
    }

    (vertices, indices)
}

/// Build a cursor quad at the given grid position within a pane rect.
///
/// `col` and `row` are zero-indexed grid positions. The cursor quad
/// occupies the cell at (col, row) with the given color.
///
/// Returns (vertices, indices) for a single quad.
pub fn build_cursor_quad(
    rect: &Rect,
    cols: u16,
    rows: u16,
    col: u16,
    row: u16,
    color: [f32; 4],
) -> (Vec<Vertex>, Vec<u16>) {
    if cols == 0 || rows == 0 {
        return (Vec::new(), Vec::new());
    }

    let clamped_col = col.min(cols - 1);
    let clamped_row = row.min(rows - 1);
    let cell_width = rect.width / f32::from(cols);
    let cell_height = rect.height / f32::from(rows);
    let x = rect.x + f32::from(clamped_col) * cell_width;
    let y = rect.y + f32::from(clamped_row) * cell_height;

    let vertices = quad_vertices(x, y, cell_width, cell_height, color).to_vec();
    let indices = quad_indices(0).to_vec();

    (vertices, indices)
}

/// Accumulates divider quad geometry across multiple edge-pair checks.
///
/// This avoids threading `&mut Vec<Vertex>`, `&mut Vec<u16>`, and
/// `&mut usize` through every helper call.
struct DividerCollector {
    vertices: Vec<Vertex>,
    indices: Vec<u16>,
    quad_count: usize,
    color: [f32; 4],
}

impl DividerCollector {
    fn new(color: [f32; 4]) -> Self {
        Self { vertices: Vec::new(), indices: Vec::new(), quad_count: 0, color }
    }

    /// Append a single divider quad.
    fn push(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.vertices.extend_from_slice(&quad_vertices(x, y, w, h, self.color));
        self.indices.extend_from_slice(&quad_indices(vertex_base(self.quad_count)));
        self.quad_count += 1;
    }

    /// If `left`'s right edge aligns with `right`'s left edge, emit a
    /// vertical divider spanning their vertical overlap.
    fn try_vertical(&mut self, left: &Rect, right: &Rect) {
        let left_right_edge = left.x + left.width;
        if (left_right_edge - right.x).abs() >= DIVIDER_EDGE_TOLERANCE {
            return;
        }
        let overlap_top = left.y.max(right.y);
        let overlap_bottom = (left.y + left.height).min(right.y + right.height);
        if overlap_bottom <= overlap_top {
            return;
        }
        self.push(
            left_right_edge - DIVIDER_WIDTH_PX / 2.0,
            overlap_top,
            DIVIDER_WIDTH_PX,
            overlap_bottom - overlap_top,
        );
    }

    /// If `top`'s bottom edge aligns with `bottom`'s top edge, emit a
    /// horizontal divider spanning their horizontal overlap.
    fn try_horizontal(&mut self, top: &Rect, bottom: &Rect) {
        let top_bottom_edge = top.y + top.height;
        if (top_bottom_edge - bottom.y).abs() >= DIVIDER_EDGE_TOLERANCE {
            return;
        }
        let overlap_left = top.x.max(bottom.x);
        let overlap_right = (top.x + top.width).min(bottom.x + bottom.width);
        if overlap_right <= overlap_left {
            return;
        }
        self.push(
            overlap_left,
            top_bottom_edge - DIVIDER_HEIGHT_PX / 2.0,
            overlap_right - overlap_left,
            DIVIDER_HEIGHT_PX,
        );
    }

    /// Consume the collector and return the accumulated geometry.
    fn finish(self) -> (Vec<Vertex>, Vec<u16>) {
        (self.vertices, self.indices)
    }
}

/// Build divider quads between adjacent pane edges.
///
/// Examines all pairs of pane layouts and, where two panes share
/// an edge (within [`DIVIDER_EDGE_TOLERANCE`]), generates a thin line quad
/// (`DIVIDER_WIDTH_PX` wide for vertical, `DIVIDER_HEIGHT_PX` tall for horizontal).
///
/// Returns (vertices, indices) for all divider quads.
pub fn build_divider_quads(
    pane_layouts: &[PaneLayout],
    divider_color: [f32; 4],
) -> (Vec<Vertex>, Vec<u16>) {
    let mut collector = DividerCollector::new(divider_color);

    for (i, layout_a) in pane_layouts.iter().enumerate() {
        for layout_b in &pane_layouts[i + 1..] {
            let a = &layout_a.rect;
            let b = &layout_b.rect;

            // Vertical edges: check both directions (a|b and b|a).
            collector.try_vertical(a, b);
            collector.try_vertical(b, a);

            // Horizontal edges: check both directions (a/b and b/a).
            collector.try_horizontal(a, b);
            collector.try_horizontal(b, a);
        }
    }

    collector.finish()
}

/// Build focus border quads around a pane rect.
///
/// Generates 4 quads (top, bottom, left, right edges) forming a
/// border of the given thickness around the pane rect.
///
/// Returns (vertices, indices) for the 4 border quads.
pub fn build_focus_border(
    rect: &Rect,
    border_thickness: f32,
    color: [f32; 4],
) -> (Vec<Vertex>, Vec<u16>) {
    // Clamp effective thickness to avoid negative dimensions when the
    // border is thicker than half the rect in either axis.
    let t = border_thickness.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);

    let mut vertices = Vec::with_capacity(16);
    let mut indices = Vec::with_capacity(24);

    // Top border
    vertices.extend_from_slice(&quad_vertices(rect.x, rect.y, rect.width, t, color));
    indices.extend_from_slice(&quad_indices(vertex_base(0)));

    // Bottom border
    vertices.extend_from_slice(&quad_vertices(
        rect.x,
        rect.y + rect.height - t,
        rect.width,
        t,
        color,
    ));
    indices.extend_from_slice(&quad_indices(vertex_base(1)));

    // Left border (between top and bottom)
    let side_height = (rect.height - 2.0 * t).max(0.0);
    vertices.extend_from_slice(&quad_vertices(rect.x, rect.y + t, t, side_height, color));
    indices.extend_from_slice(&quad_indices(vertex_base(2)));

    // Right border (between top and bottom)
    vertices.extend_from_slice(&quad_vertices(
        rect.x + rect.width - t,
        rect.y + t,
        t,
        side_height,
        color,
    ));
    indices.extend_from_slice(&quad_indices(vertex_base(3)));

    (vertices, indices)
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::manual_range_contains)]
mod tests {
    use super::*;
    use veil_core::layout::{PaneLayout, Rect};
    use veil_core::workspace::{PaneId, SurfaceId};

    // --- Helpers ---

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect { x, y, width, height }
    }

    fn default_color() -> [f32; 4] {
        [0.1, 0.1, 0.1, 1.0]
    }

    fn pane_layout(id: u64, x: f32, y: f32, w: f32, h: f32) -> PaneLayout {
        PaneLayout {
            pane_id: PaneId::new(id),
            surface_id: SurfaceId::new(id + 100),
            rect: rect(x, y, w, h),
        }
    }

    // ============================================================
    // build_cell_background_quads — happy path
    // ============================================================

    #[test]
    fn cell_bg_quads_count() {
        let params = CellGridParams {
            rect: rect(0.0, 0.0, 800.0, 600.0),
            cols: 80,
            rows: 24,
            bg_color: default_color(),
            cell_bg_colors: None,
        };
        let (vertices, indices) = build_cell_background_quads(&params);
        let expected_quads = 80 * 24;
        assert_eq!(vertices.len(), expected_quads * 4, "4 vertices per quad");
        assert_eq!(indices.len(), expected_quads * 6, "6 indices per quad");
    }

    #[test]
    fn cell_bg_single_cell_fills_rect() {
        let r = rect(10.0, 20.0, 100.0, 50.0);
        let params = CellGridParams {
            rect: r,
            cols: 1,
            rows: 1,
            bg_color: [1.0, 0.0, 0.0, 1.0],
            cell_bg_colors: None,
        };
        let (vertices, indices) = build_cell_background_quads(&params);
        assert_eq!(vertices.len(), 4);
        assert_eq!(indices.len(), 6);
        // The single cell should fill the entire rect
        let positions: Vec<[f32; 2]> = vertices.iter().map(|v| v.position).collect();
        assert!(positions.contains(&[10.0, 20.0]), "should contain top-left");
        assert!(positions.contains(&[110.0, 20.0]), "should contain top-right");
        assert!(positions.contains(&[10.0, 70.0]), "should contain bottom-left");
        assert!(positions.contains(&[110.0, 70.0]), "should contain bottom-right");
    }

    // ============================================================
    // build_cell_background_quads — edge cases
    // ============================================================

    #[test]
    fn cell_bg_zero_cols_empty() {
        let params = CellGridParams {
            rect: rect(0.0, 0.0, 800.0, 600.0),
            cols: 0,
            rows: 24,
            bg_color: default_color(),
            cell_bg_colors: None,
        };
        let (vertices, indices) = build_cell_background_quads(&params);
        assert!(vertices.is_empty());
        assert!(indices.is_empty());
    }

    #[test]
    fn cell_bg_zero_rows_empty() {
        let params = CellGridParams {
            rect: rect(0.0, 0.0, 800.0, 600.0),
            cols: 80,
            rows: 0,
            bg_color: default_color(),
            cell_bg_colors: None,
        };
        let (vertices, indices) = build_cell_background_quads(&params);
        assert!(vertices.is_empty());
        assert!(indices.is_empty());
    }

    #[test]
    fn cell_bg_vertices_within_rect() {
        let r = rect(50.0, 50.0, 400.0, 300.0);
        let params = CellGridParams {
            rect: r,
            cols: 10,
            rows: 5,
            bg_color: default_color(),
            cell_bg_colors: None,
        };
        let (vertices, _) = build_cell_background_quads(&params);
        for v in &vertices {
            let [x, y] = v.position;
            assert!(
                x >= 50.0 - 0.01 && x <= 450.0 + 0.01,
                "vertex x={x} out of rect bounds [50, 450]"
            );
            assert!(
                y >= 50.0 - 0.01 && y <= 350.0 + 0.01,
                "vertex y={y} out of rect bounds [50, 350]"
            );
        }
    }

    // ============================================================
    // build_cursor_quad — happy path
    // ============================================================

    #[test]
    fn cursor_quad_at_origin() {
        let r = rect(0.0, 0.0, 800.0, 600.0);
        let color = [0.9, 0.9, 0.9, 1.0];
        let (vertices, indices) = build_cursor_quad(&r, 80, 24, 0, 0, color);
        assert_eq!(vertices.len(), 4);
        assert_eq!(indices.len(), 6);
        // Cell width = 800/80 = 10, cell height = 600/24 = 25
        // Cursor at (0,0) should be at top-left corner
        let positions: Vec<[f32; 2]> = vertices.iter().map(|v| v.position).collect();
        assert!(positions.contains(&[0.0, 0.0]), "should contain top-left at origin");
    }

    #[test]
    fn cursor_quad_at_last_cell() {
        let r = rect(0.0, 0.0, 800.0, 600.0);
        let color = [0.9, 0.9, 0.9, 1.0];
        let (vertices, _) = build_cursor_quad(&r, 80, 24, 79, 23, color);
        // Cell width = 10, cell height = 25
        // Last cell starts at (79*10, 23*25) = (790, 575)
        let positions: Vec<[f32; 2]> = vertices.iter().map(|v| v.position).collect();
        assert!(
            positions.contains(&[790.0, 575.0]),
            "cursor at last cell should start at (790, 575), got: {positions:?}"
        );
    }

    #[test]
    fn cursor_quad_counts() {
        let r = rect(0.0, 0.0, 800.0, 600.0);
        let (vertices, indices) = build_cursor_quad(&r, 80, 24, 5, 5, [1.0; 4]);
        assert_eq!(vertices.len(), 4, "cursor should be exactly 1 quad = 4 vertices");
        assert_eq!(indices.len(), 6, "cursor should be exactly 1 quad = 6 indices");
    }

    // ============================================================
    // build_cursor_quad — edge cases
    // ============================================================

    #[test]
    fn cursor_quad_zero_grid_empty() {
        let r = rect(0.0, 0.0, 800.0, 600.0);
        let (vertices, indices) = build_cursor_quad(&r, 0, 0, 0, 0, [1.0; 4]);
        assert!(vertices.is_empty());
        assert!(indices.is_empty());
    }

    #[test]
    fn cursor_quad_clamps_out_of_bounds() {
        let r = rect(0.0, 0.0, 800.0, 600.0);
        // col=100 is beyond 80 cols — should clamp to col=79
        let (vertices, _) = build_cursor_quad(&r, 80, 24, 100, 50, [1.0; 4]);
        // Should not panic and should produce valid geometry
        assert_eq!(vertices.len(), 4);
        // The clamped position should be within the rect
        for v in &vertices {
            assert!(v.position[0] <= 800.0 + 0.01, "x should be within rect");
            assert!(v.position[1] <= 600.0 + 0.01, "y should be within rect");
        }
    }

    // ============================================================
    // build_divider_quads — happy path
    // ============================================================

    #[test]
    fn divider_two_panes_side_by_side() {
        // Left pane: (0,0) 400x600, Right pane: (400,0) 400x600
        // They share a vertical edge at x=400
        let layouts =
            vec![pane_layout(1, 0.0, 0.0, 400.0, 600.0), pane_layout(2, 400.0, 0.0, 400.0, 600.0)];
        let (vertices, indices) = build_divider_quads(&layouts, [0.3; 4]);
        // Should produce at least one divider
        assert!(!vertices.is_empty(), "should have divider vertices");
        assert!(!indices.is_empty(), "should have divider indices");
        // One divider = 4 vertices, 6 indices
        assert_eq!(vertices.len(), 4, "one vertical divider = 4 vertices");
        assert_eq!(indices.len(), 6, "one vertical divider = 6 indices");
    }

    #[test]
    fn divider_two_panes_stacked() {
        // Top pane: (0,0) 800x300, Bottom pane: (0,300) 800x300
        // They share a horizontal edge at y=300
        let layouts =
            vec![pane_layout(1, 0.0, 0.0, 800.0, 300.0), pane_layout(2, 0.0, 300.0, 800.0, 300.0)];
        let (vertices, indices) = build_divider_quads(&layouts, [0.3; 4]);
        assert!(!vertices.is_empty(), "should have divider vertices");
        assert!(!indices.is_empty(), "should have divider indices");
        assert_eq!(vertices.len(), 4, "one horizontal divider = 4 vertices");
        assert_eq!(indices.len(), 6, "one horizontal divider = 6 indices");
    }

    // ============================================================
    // build_divider_quads — edge cases
    // ============================================================

    #[test]
    fn divider_single_pane_none() {
        let layouts = vec![pane_layout(1, 0.0, 0.0, 800.0, 600.0)];
        let (vertices, indices) = build_divider_quads(&layouts, [0.3; 4]);
        assert!(vertices.is_empty(), "single pane should have no dividers");
        assert!(indices.is_empty());
    }

    #[test]
    fn divider_empty_layouts_none() {
        let layouts: Vec<PaneLayout> = vec![];
        let (vertices, indices) = build_divider_quads(&layouts, [0.3; 4]);
        assert!(vertices.is_empty());
        assert!(indices.is_empty());
    }

    // ============================================================
    // build_focus_border — happy path
    // ============================================================

    #[test]
    fn focus_border_quad_counts() {
        let r = rect(100.0, 100.0, 400.0, 300.0);
        let (vertices, indices) = build_focus_border(&r, 2.0, [0.2, 0.5, 1.0, 0.8]);
        // 4 border quads (top, bottom, left, right)
        assert_eq!(vertices.len(), 16, "4 border quads * 4 vertices each = 16");
        assert_eq!(indices.len(), 24, "4 border quads * 6 indices each = 24");
    }

    // ============================================================
    // build_focus_border — edge cases
    // ============================================================

    #[test]
    fn focus_border_zero_thickness() {
        let r = rect(100.0, 100.0, 400.0, 300.0);
        // Zero thickness: should produce quads with zero area but not panic
        let (vertices, indices) = build_focus_border(&r, 0.0, [1.0; 4]);
        assert_eq!(vertices.len(), 16, "still 4 quads even with zero thickness");
        assert_eq!(indices.len(), 24);
    }

    #[test]
    fn focus_border_zero_size_rect() {
        let r = rect(100.0, 100.0, 0.0, 0.0);
        // Zero-size rect: should not panic
        let (vertices, indices) = build_focus_border(&r, 2.0, [1.0; 4]);
        assert_eq!(vertices.len(), 16);
        assert_eq!(indices.len(), 24);
    }

    #[test]
    fn focus_border_thickness_exceeding_rect_clamps() {
        // border_thickness=50 on a 40x30 rect would produce negative side
        // heights without clamping. Verify all vertex positions stay valid.
        let r = rect(10.0, 20.0, 40.0, 30.0);
        let (vertices, _) = build_focus_border(&r, 50.0, [1.0; 4]);
        assert_eq!(vertices.len(), 16, "still 4 quads");
        for v in &vertices {
            let [x, y] = v.position;
            // All vertices must be within or on the rect boundary (no
            // inverted / negative-dimension quads).
            assert!(x >= 10.0 - 0.01 && x <= 50.0 + 0.01, "x={x} should be in [10, 50]");
            assert!(y >= 20.0 - 0.01 && y <= 50.0 + 0.01, "y={y} should be in [20, 50]");
        }
    }

    // ============================================================
    // VEI-77 Unit 3: Per-cell background colors
    // ============================================================

    #[test]
    fn cell_bg_no_per_cell_colors_uses_default() {
        // When cell_bg_colors is None, all cells should use bg_color.
        let bg = [0.1, 0.1, 0.1, 1.0];
        let params = CellGridParams {
            rect: rect(0.0, 0.0, 100.0, 50.0),
            cols: 2,
            rows: 2,
            bg_color: bg,
            cell_bg_colors: None,
        };
        let (vertices, _) = build_cell_background_quads(&params);
        for v in &vertices {
            assert_eq!(v.color, bg, "without per-cell colors, all cells use bg_color");
        }
    }

    #[test]
    fn cell_bg_per_cell_colors_overrides_default() {
        // When cell_bg_colors is Some, cells with Some(color) should use that color.
        let bg = [0.1, 0.1, 0.1, 1.0];
        let red = [1.0, 0.0, 0.0, 1.0];
        let params = CellGridParams {
            rect: rect(0.0, 0.0, 100.0, 50.0),
            cols: 2,
            rows: 1,
            bg_color: bg,
            cell_bg_colors: Some(vec![Some(red), None]),
        };
        let (vertices, _) = build_cell_background_quads(&params);
        // First cell (vertices 0..4) should have red color.
        for v in &vertices[0..4] {
            assert_eq!(
                v.color, red,
                "cell (0,0) with explicit color should be red, got {:?}",
                v.color
            );
        }
        // Second cell (vertices 4..8) should have default bg color.
        for v in &vertices[4..8] {
            assert_eq!(v.color, bg, "cell (0,1) without explicit color should use default bg");
        }
    }

    #[test]
    fn cell_bg_per_cell_mixed_colors() {
        let bg = [0.1, 0.1, 0.1, 1.0];
        let red = [1.0, 0.0, 0.0, 1.0];
        let green = [0.0, 1.0, 0.0, 1.0];
        let params = CellGridParams {
            rect: rect(0.0, 0.0, 300.0, 100.0),
            cols: 3,
            rows: 1,
            bg_color: bg,
            cell_bg_colors: Some(vec![Some(red), None, Some(green)]),
        };
        let (vertices, _) = build_cell_background_quads(&params);
        // Cell 0 -> red
        assert_eq!(vertices[0].color, red, "cell 0 should be red");
        // Cell 1 -> default bg
        assert_eq!(vertices[4].color, bg, "cell 1 should be default bg");
        // Cell 2 -> green
        assert_eq!(vertices[8].color, green, "cell 2 should be green");
    }

    #[test]
    fn cell_bg_per_cell_colors_short_array_clamps() {
        // If cell_bg_colors has fewer entries than cols*rows, excess cells
        // should use default bg_color without panicking.
        let bg = [0.1, 0.1, 0.1, 1.0];
        let red = [1.0, 0.0, 0.0, 1.0];
        let params = CellGridParams {
            rect: rect(0.0, 0.0, 200.0, 100.0),
            cols: 2,
            rows: 2,
            bg_color: bg,
            cell_bg_colors: Some(vec![Some(red)]), // Only 1 entry for 4 cells
        };
        let (vertices, _) = build_cell_background_quads(&params);
        // Should produce vertices for all 4 cells without panicking.
        assert_eq!(vertices.len(), 4 * 4, "should still produce all 4 cells");
        // First cell should be red.
        assert_eq!(vertices[0].color, red, "cell 0 should be red");
        // Remaining cells should fall back to default bg.
        assert_eq!(vertices[4].color, bg, "cell 1 should be default bg (short array)");
    }

    // ============================================================
    // VEI-77 Unit 3: color_to_f32 helper
    // ============================================================

    #[test]
    fn color_to_f32_converts_white() {
        let white = veil_ghostty::Color { r: 255, g: 255, b: 255 };
        let result = color_to_f32(white);
        assert!(
            (result[0] - 1.0).abs() < 0.01
                && (result[1] - 1.0).abs() < 0.01
                && (result[2] - 1.0).abs() < 0.01
                && (result[3] - 1.0).abs() < 0.01,
            "white should convert to [1.0, 1.0, 1.0, 1.0], got {result:?}"
        );
    }

    #[test]
    fn color_to_f32_converts_black() {
        let black = veil_ghostty::Color { r: 0, g: 0, b: 0 };
        let result = color_to_f32(black);
        assert!(
            result[0].abs() < 0.01
                && result[1].abs() < 0.01
                && result[2].abs() < 0.01
                && (result[3] - 1.0).abs() < 0.01,
            "black should convert to [0.0, 0.0, 0.0, 1.0], got {result:?}"
        );
    }

    #[test]
    fn color_to_f32_converts_red() {
        let red = veil_ghostty::Color { r: 255, g: 0, b: 0 };
        let result = color_to_f32(red);
        assert!(
            (result[0] - 1.0).abs() < 0.01
                && result[1].abs() < 0.01
                && result[2].abs() < 0.01
                && (result[3] - 1.0).abs() < 0.01,
            "red should convert to [1.0, 0.0, 0.0, 1.0], got {result:?}"
        );
    }

    #[test]
    fn color_to_f32_normalizes_mid_value() {
        let color = veil_ghostty::Color { r: 128, g: 64, b: 32 };
        let result = color_to_f32(color);
        let expected_r = 128.0 / 255.0;
        let expected_g = 64.0 / 255.0;
        let expected_b = 32.0 / 255.0;
        assert!(
            (result[0] - expected_r).abs() < 0.01
                && (result[1] - expected_g).abs() < 0.01
                && (result[2] - expected_b).abs() < 0.01
                && (result[3] - 1.0).abs() < 0.01,
            "mid-value color should be normalized, got {result:?}"
        );
    }
}
