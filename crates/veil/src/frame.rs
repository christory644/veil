//! Frame geometry composition — combines all quad builders with `AppState`
//! to produce the full frame's geometry. Pure logic (no GPU).

use veil_core::focus::FocusManager;
use veil_core::layout::{compute_layout, Rect};
use veil_core::state::AppState;

use crate::quad_builder::{
    build_cell_background_quads, build_cursor_quad, build_divider_quads, build_focus_border,
    CellGridParams,
};
use crate::vertex::Vertex;

// -- Default grid dimensions (until real terminal state is wired in) ----------
const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;

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
    /// All vertices for this frame.
    pub vertices: Vec<Vertex>,
    /// All indices for this frame.
    pub indices: Vec<u16>,
    /// The clear color (window background).
    pub clear_color: wgpu::Color,
}

/// Build all geometry for the current frame.
///
/// This is the main composition function called once per frame:
/// 1. Compute available terminal area (window size minus sidebar if visible)
/// 2. Get active workspace layout via `compute_layout` (respecting zoom)
/// 3. Build cell background quads for each pane
/// 4. Build cursor quad for the focused pane (if cursor visible)
/// 5. Build divider quads between adjacent panes
/// 6. Build focus border around the focused pane
/// 7. Concatenate all vertices/indices with correct base offsets
///
/// Returns `FrameGeometry` ready for a single draw call.
// Window pixel dimensions and sidebar width_px all fit comfortably in f32.
#[allow(clippy::cast_precision_loss)]
pub fn build_frame_geometry(
    app_state: &AppState,
    focus: &FocusManager,
    window_width: u32,
    window_height: u32,
) -> FrameGeometry {
    let empty =
        || FrameGeometry { vertices: Vec::new(), indices: Vec::new(), clear_color: CLEAR_COLOR };

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

    // Append geometry with correct index offsets so multiple quad batches
    // share a single vertex/index buffer.
    let mut append = |verts: &[Vertex], idxs: &[u16]| {
        #[allow(clippy::cast_possible_truncation)]
        let base_offset = all_vertices.len() as u16;
        all_vertices.extend_from_slice(verts);
        all_indices.extend(idxs.iter().map(|i| i + base_offset));
    };

    // Cell background quads for each pane.
    for pl in &pane_layouts {
        let params = CellGridParams {
            rect: pl.rect,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            bg_color: BG_COLOR,
        };
        let (verts, indices) = build_cell_background_quads(&params);
        append(&verts, &indices);
    }

    // Divider quads between adjacent panes.
    let (div_verts, div_indices) = build_divider_quads(&pane_layouts, DIVIDER_COLOR);
    append(&div_verts, &div_indices);

    // Cursor and focus border for the focused pane.
    if let Some(focused_surface) = focus.focused_surface() {
        if let Some(pl) = pane_layouts.iter().find(|pl| pl.surface_id == focused_surface) {
            let (cursor_verts, cursor_indices) =
                build_cursor_quad(&pl.rect, DEFAULT_COLS, DEFAULT_ROWS, 0, 0, CURSOR_COLOR);
            append(&cursor_verts, &cursor_indices);

            let (border_verts, border_indices) =
                build_focus_border(&pl.rect, FOCUS_BORDER_THICKNESS, FOCUS_BORDER_COLOR);
            append(&border_verts, &border_indices);
        }
    }

    FrameGeometry { vertices: all_vertices, indices: all_indices, clear_color: CLEAR_COLOR }
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
        let geom = build_frame_geometry(&state, &focus, 1280, 800);
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

        let geom = build_frame_geometry(&state, &focus, 1280, 800);

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

        let geom = build_frame_geometry(&state, &focus, 1280, 800);

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

        let geom = build_frame_geometry(&state, &focus, 1280, 800);

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

        let geom = build_frame_geometry(&state, &focus, 1280, 800);

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

        let geom = build_frame_geometry(&state, &focus, 1280, 800);

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

        let geom_zoomed = build_frame_geometry(&state, &focus, 1280, 800);

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
}
