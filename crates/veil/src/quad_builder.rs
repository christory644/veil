//! Quad builders -- cell backgrounds, cursor, dividers, focus border.
//!
//! Functions that consume layout geometry (from `veil-core`) and produce
//! `Vertex`/index arrays. Pure geometry, no GPU state. Fully unit-testable.

use veil_core::layout::{PaneLayout, Rect};

use crate::vertex::{quad_indices, quad_vertices, vertex_base, Vertex};

/// Parameters for building cell background quads for a pane.
pub struct CellGridParams {
    /// The pane's pixel rectangle (from compute_layout).
    pub rect: Rect,
    /// Number of columns in the terminal grid.
    pub cols: u16,
    /// Number of rows in the terminal grid.
    pub rows: u16,
    /// Background color as RGBA.
    pub bg_color: [f32; 4],
}

/// Build cell background quads for a single pane.
///
/// Generates one quad per cell (cols * rows quads total). Each cell
/// is sized to evenly fill the pane rect. All cells use the same
/// background color (real per-cell colors are a follow-up).
///
/// Returns (vertices, indices) ready for GPU upload.
pub fn build_cell_background_quads(params: &CellGridParams) -> (Vec<Vertex>, Vec<u16>) {
    todo!()
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
    todo!()
}

/// Build divider quads between adjacent pane edges.
///
/// Examines all pairs of pane layouts and, where two panes share
/// an edge (within a tolerance), generates a thin line quad (1px
/// wide for vertical dividers, 1px tall for horizontal dividers).
///
/// Returns (vertices, indices) for all divider quads.
pub fn build_divider_quads(
    pane_layouts: &[PaneLayout],
    divider_color: [f32; 4],
) -> (Vec<Vertex>, Vec<u16>) {
    todo!()
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
    todo!()
}

#[cfg(test)]
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
        };
        let (vertices, indices) = build_cell_background_quads(&params);
        let expected_quads = 80 * 24;
        assert_eq!(vertices.len(), expected_quads * 4, "4 vertices per quad");
        assert_eq!(indices.len(), expected_quads * 6, "6 indices per quad");
    }

    #[test]
    fn cell_bg_single_cell_fills_rect() {
        let r = rect(10.0, 20.0, 100.0, 50.0);
        let params = CellGridParams { rect: r, cols: 1, rows: 1, bg_color: [1.0, 0.0, 0.0, 1.0] };
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
        };
        let (vertices, indices) = build_cell_background_quads(&params);
        assert!(vertices.is_empty());
        assert!(indices.is_empty());
    }

    #[test]
    fn cell_bg_vertices_within_rect() {
        let r = rect(50.0, 50.0, 400.0, 300.0);
        let params = CellGridParams { rect: r, cols: 10, rows: 5, bg_color: default_color() };
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
}
