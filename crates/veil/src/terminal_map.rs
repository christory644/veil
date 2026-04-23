//! Terminal map -- manages `TerminalWriter` instances keyed by `SurfaceId`.
//!
//! `TerminalWriter` abstracts over `veil_ghostty::Terminal` so that all output
//! routing, resize propagation, and cleanup logic is testable without FFI.

use std::collections::HashMap;

use veil_core::workspace::SurfaceId;

// -- TerminalWriter trait -------------------------------------------------------

/// Abstraction over terminal state management.
///
/// The real implementation (`GhosttyTerminalWriter`) wraps `veil_ghostty::Terminal`.
/// Tests use lightweight mocks.
///
/// Several methods in this trait (and corresponding `TerminalMap` accessors) are
/// not yet called from the binary target but are exercised by tests and will be
/// wired in by upcoming tasks (resize propagation, cell rendering). The
/// `#[allow(dead_code)]` annotations on those call-sites are intentional.
#[allow(dead_code)] // trait is only used via cfg-gated impls + test mocks
pub trait TerminalWriter {
    /// Feed VT-encoded bytes to the terminal's parser.
    fn write_vt(&mut self, data: &[u8]);

    /// Resize the terminal to new cell dimensions and pixel sizes.
    fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<(), String>;

    /// Query the terminal's current column count.
    fn cols(&self) -> u16;

    /// Query the terminal's current row count.
    fn rows(&self) -> u16;

    /// Extract a snapshot of cell data for rendering.
    /// Returns `None` if the terminal has no render state available.
    fn render_cells(&mut self) -> Option<veil_ghostty::CellGrid> {
        None
    }
}

// -- GhosttyTerminalWriter (real FFI-backed impl) -------------------------------

/// Wraps a `veil_ghostty::Terminal` to implement `TerminalWriter`.
#[cfg(not(no_libghosty))]
struct GhosttyTerminalWriter {
    terminal: veil_ghostty::Terminal,
}

#[cfg(not(no_libghosty))]
impl TerminalWriter for GhosttyTerminalWriter {
    fn write_vt(&mut self, data: &[u8]) {
        self.terminal.write_vt(data);
    }

    fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<(), String> {
        self.terminal.resize(cols, rows, cell_width_px, cell_height_px).map_err(|e| e.to_string())
    }

    fn cols(&self) -> u16 {
        self.terminal.cols().unwrap_or(80)
    }

    fn rows(&self) -> u16 {
        self.terminal.rows().unwrap_or(24)
    }

    fn render_cells(&mut self) -> Option<veil_ghostty::CellGrid> {
        // MVP: returns None until cell-iteration FFI is wired (VEI-77).
        None
    }
}

/// Create a real terminal writer backed by libghosty.
///
/// Returns `None` if terminal creation fails or if libghosty is not available.
/// The returned writer wraps a `veil_ghostty::Terminal` configured with the
/// given cell dimensions and 10,000 lines of scrollback.
#[cfg(not(no_libghosty))]
pub fn create_ghostty_terminal(cols: u16, rows: u16) -> Option<Box<dyn TerminalWriter>> {
    let config = veil_ghostty::TerminalConfig { cols, rows, max_scrollback: 10_000 };
    match veil_ghostty::Terminal::new(config) {
        Ok(terminal) => Some(Box::new(GhosttyTerminalWriter { terminal })),
        Err(e) => {
            tracing::error!("failed to create ghostty terminal: {e}");
            None
        }
    }
}

/// Stub factory when libghosty is not available. Always returns `None`.
#[cfg(no_libghosty)]
pub fn create_ghostty_terminal(_cols: u16, _rows: u16) -> Option<Box<dyn TerminalWriter>> {
    tracing::warn!("libghosty not available, terminal emulation disabled");
    None
}

// -- TerminalMap ----------------------------------------------------------------

/// Manages `TerminalWriter` instances keyed by `SurfaceId`.
pub struct TerminalMap {
    terminals: HashMap<SurfaceId, Box<dyn TerminalWriter>>,
}

impl TerminalMap {
    /// Create a new empty `TerminalMap`.
    pub fn new() -> Self {
        Self { terminals: HashMap::new() }
    }

    /// Insert a terminal for a surface. Returns the old terminal if one existed.
    pub fn insert(
        &mut self,
        surface_id: SurfaceId,
        terminal: Box<dyn TerminalWriter>,
    ) -> Option<Box<dyn TerminalWriter>> {
        self.terminals.insert(surface_id, terminal)
    }

    /// Remove a terminal for a surface.
    pub fn remove(&mut self, surface_id: SurfaceId) -> Option<Box<dyn TerminalWriter>> {
        self.terminals.remove(&surface_id)
    }

    /// Feed VT data to the terminal for a surface. Returns false if surface not found.
    pub fn write_vt(&mut self, surface_id: SurfaceId, data: &[u8]) -> bool {
        if let Some(terminal) = self.terminals.get_mut(&surface_id) {
            terminal.write_vt(data);
            true
        } else {
            false
        }
    }

    /// Resize the terminal for a surface. Returns `Err` if surface not found.
    #[allow(dead_code)]
    pub fn resize(
        &mut self,
        surface_id: SurfaceId,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<(), String> {
        match self.terminals.get_mut(&surface_id) {
            Some(terminal) => terminal.resize(cols, rows, cell_width_px, cell_height_px),
            None => Err(format!("surface {surface_id:?} not found in terminal map")),
        }
    }

    /// Get a reference to a terminal.
    #[allow(dead_code)]
    pub fn get(&self, surface_id: SurfaceId) -> Option<&dyn TerminalWriter> {
        self.terminals.get(&surface_id).map(AsRef::as_ref)
    }

    /// Get a mutable reference to a terminal writer.
    #[allow(dead_code)]
    pub fn get_mut(&mut self, surface_id: SurfaceId) -> Option<&mut Box<dyn TerminalWriter>> {
        self.terminals.get_mut(&surface_id)
    }

    /// Number of active terminals.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.terminals.len()
    }

    /// Whether the map is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.terminals.is_empty()
    }

    /// Iterate over surface IDs and their mutable terminal writers.
    #[allow(dead_code)]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (SurfaceId, &mut Box<dyn TerminalWriter>)> {
        self.terminals.iter_mut().map(|(&k, v)| (k, v))
    }
}

// -- Free functions -------------------------------------------------------------

/// Compute terminal cell dimensions from a pane rect and cell pixel sizes.
///
/// Returns `(cols, rows)` clamped to at least `(1, 1)`.
#[allow(
    dead_code,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn compute_pane_cells(
    pane_width_px: f32,
    pane_height_px: f32,
    cell_width_px: u32,
    cell_height_px: u32,
) -> (u16, u16) {
    debug_assert!(cell_width_px > 0, "cell_width_px must be positive");
    debug_assert!(cell_height_px > 0, "cell_height_px must be positive");
    if cell_width_px == 0 || cell_height_px == 0 {
        return (1, 1);
    }
    let cols = (pane_width_px / cell_width_px as f32).floor() as u16;
    let rows = (pane_height_px / cell_height_px as f32).floor() as u16;
    (cols.max(1), rows.max(1))
}

/// Process a single `StateUpdate` against the `TerminalMap`.
///
/// For `PtyOutput`, calls `write_vt` on the correct terminal.
/// For `SurfaceExited`, removes the terminal from the map.
///
/// Returns `true` if the update was a terminal-related variant
/// (`PtyOutput` or `SurfaceExited`), regardless of whether the
/// surface was present in the map. Returns `false` for all other
/// `StateUpdate` variants.
#[allow(dead_code)]
pub fn process_state_update(
    update: &veil_core::message::StateUpdate,
    terminal_map: &mut TerminalMap,
) -> bool {
    match update {
        veil_core::message::StateUpdate::PtyOutput { surface_id, data } => {
            if !terminal_map.write_vt(*surface_id, data) {
                tracing::debug!(?surface_id, "PtyOutput for unknown surface, ignoring");
            }
            true
        }
        veil_core::message::StateUpdate::SurfaceExited { surface_id, .. } => {
            if terminal_map.remove(*surface_id).is_none() {
                tracing::debug!(?surface_id, "SurfaceExited for unknown surface, ignoring");
            }
            true
        }
        _ => false,
    }
}

// -- Surface exit handling ------------------------------------------------------

/// Outcome of handling a surface exit in the workspace layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceExitOutcome {
    /// The surface was not found in any workspace.
    SurfaceNotFound,
    /// The surface was the only pane; workspace left intact.
    SinglePaneRetained,
    /// The pane was closed and focus was shifted to the given surface.
    PaneClosedFocusShifted(SurfaceId),
    /// The pane was closed but focus was already on a different surface.
    PaneClosedFocusUnchanged,
}

/// Handle surface exit in `AppState`: close the pane if the workspace has more
/// than one, otherwise leave it. If the exited surface was focused, shift focus
/// to a sibling.
pub fn handle_surface_exit(
    surface_id: SurfaceId,
    app_state: &mut veil_core::state::AppState,
    focus: &mut veil_core::focus::FocusManager,
) -> SurfaceExitOutcome {
    // Find the workspace containing this surface.
    let Some(ws_id) = app_state
        .workspaces
        .iter()
        .find(|ws| ws.layout.surface_ids().contains(&surface_id))
        .map(|ws| ws.id)
    else {
        return SurfaceExitOutcome::SurfaceNotFound;
    };

    let Some(ws) = app_state.workspace(ws_id) else {
        return SurfaceExitOutcome::SurfaceNotFound;
    };

    // If this is the only pane, leave the workspace intact.
    if ws.layout.pane_count() <= 1 {
        return SurfaceExitOutcome::SinglePaneRetained;
    }

    // Find the pane_id for this surface.
    let Some(pane_id) =
        app_state.workspace(ws_id).and_then(|ws| ws.pane_id_for_surface(surface_id))
    else {
        return SurfaceExitOutcome::SurfaceNotFound;
    };

    // Gather surfaces before closing so we can pick a new focus target.
    let surfaces_before =
        app_state.workspace(ws_id).map(|ws| ws.layout.surface_ids()).unwrap_or_default();

    // Close the pane.
    if let Err(e) = app_state.close_pane(ws_id, pane_id) {
        tracing::warn!(?surface_id, ?pane_id, ?e, "close_pane failed after surface was resolved");
        return SurfaceExitOutcome::SurfaceNotFound;
    }

    let surfaces_after =
        app_state.workspace(ws_id).map(|ws| ws.layout.surface_ids()).unwrap_or_default();

    if focus.focused_surface() == Some(surface_id) {
        let new_focus = pick_focus_replacement(&surfaces_before, &surfaces_after, surface_id);
        if let Some(s) = new_focus {
            focus.focus_surface(s);
            SurfaceExitOutcome::PaneClosedFocusShifted(s)
        } else {
            SurfaceExitOutcome::PaneClosedFocusUnchanged
        }
    } else {
        SurfaceExitOutcome::PaneClosedFocusUnchanged
    }
}

/// Pick a replacement focus target after a surface is removed from the layout.
///
/// Prefers the surface at the same position in the original list; falls back
/// to the last surface in the remaining list.
fn pick_focus_replacement(
    surfaces_before: &[SurfaceId],
    surfaces_after: &[SurfaceId],
    closed: SurfaceId,
) -> Option<SurfaceId> {
    if surfaces_after.is_empty() {
        return None;
    }
    let pos = surfaces_before.iter().position(|s| *s == closed).unwrap_or(0);
    let idx = pos.min(surfaces_after.len().saturating_sub(1));
    Some(surfaces_after[idx])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use veil_core::focus::FocusManager;
    use veil_core::message::StateUpdate;
    use veil_core::state::AppState;
    use veil_core::workspace::{SplitDirection, SurfaceId};

    // ================================================================
    // MockTerminalWriter
    // ================================================================

    /// Tracks all calls to `write_vt` and `resize` for assertion.
    #[derive(Debug, Default, Clone)]
    struct MockWriterState {
        writes: Vec<Vec<u8>>,
        resizes: Vec<(u16, u16, u32, u32)>,
        cols: u16,
        rows: u16,
    }

    struct MockTerminalWriter {
        state: Rc<RefCell<MockWriterState>>,
    }

    impl MockTerminalWriter {
        fn new(cols: u16, rows: u16) -> (Self, Rc<RefCell<MockWriterState>>) {
            let state = Rc::new(RefCell::new(MockWriterState { cols, rows, ..Default::default() }));
            (Self { state: Rc::clone(&state) }, state)
        }
    }

    impl TerminalWriter for MockTerminalWriter {
        fn write_vt(&mut self, data: &[u8]) {
            self.state.borrow_mut().writes.push(data.to_vec());
        }

        fn resize(
            &mut self,
            cols: u16,
            rows: u16,
            cell_width_px: u32,
            cell_height_px: u32,
        ) -> Result<(), String> {
            let mut s = self.state.borrow_mut();
            s.resizes.push((cols, rows, cell_width_px, cell_height_px));
            s.cols = cols;
            s.rows = rows;
            Ok(())
        }

        fn cols(&self) -> u16 {
            self.state.borrow().cols
        }

        fn rows(&self) -> u16 {
            self.state.borrow().rows
        }
    }

    fn make_mock(cols: u16, rows: u16) -> (Box<dyn TerminalWriter>, Rc<RefCell<MockWriterState>>) {
        let (mock, state) = MockTerminalWriter::new(cols, rows);
        (Box::new(mock), state)
    }

    // ================================================================
    // Unit 2: TerminalMap basic operations
    // ================================================================

    #[test]
    fn new_terminal_map_is_empty() {
        let map = TerminalMap::new();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn insert_and_retrieve() {
        let mut map = TerminalMap::new();
        let (mock, _state) = make_mock(80, 24);
        let old = map.insert(SurfaceId::new(1), mock);
        assert!(old.is_none(), "first insert should return None");
        assert_eq!(map.len(), 1);
        assert!(!map.is_empty());
        let terminal = map.get(SurfaceId::new(1));
        assert!(terminal.is_some(), "get should return the inserted terminal");
        assert_eq!(terminal.unwrap().cols(), 80);
        assert_eq!(terminal.unwrap().rows(), 24);
    }

    #[test]
    fn write_vt_routes_to_correct_surface() {
        let mut map = TerminalMap::new();
        let (mock1, state1) = make_mock(80, 24);
        let (mock2, state2) = make_mock(80, 24);
        map.insert(SurfaceId::new(1), mock1);
        map.insert(SurfaceId::new(2), mock2);

        let result = map.write_vt(SurfaceId::new(1), b"hello");

        assert!(result, "write_vt should return true for existing surface");
        assert_eq!(state1.borrow().writes.len(), 1, "terminal 1 should have 1 write");
        assert_eq!(state1.borrow().writes[0], b"hello");
        assert_eq!(state2.borrow().writes.len(), 0, "terminal 2 should have 0 writes");
    }

    #[test]
    fn write_vt_unknown_surface_returns_false() {
        let mut map = TerminalMap::new();
        let result = map.write_vt(SurfaceId::new(999), b"data");
        assert!(!result, "write_vt for unknown surface should return false");
    }

    #[test]
    fn remove_cleans_up() {
        let mut map = TerminalMap::new();
        let (mock, _state) = make_mock(80, 24);
        map.insert(SurfaceId::new(1), mock);
        assert_eq!(map.len(), 1);

        let removed = map.remove(SurfaceId::new(1));
        assert!(removed.is_some(), "remove should return the terminal");
        assert_eq!(map.len(), 0);
        assert!(map.get(SurfaceId::new(1)).is_none(), "get after remove should return None");
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let mut map = TerminalMap::new();
        let removed = map.remove(SurfaceId::new(999));
        assert!(removed.is_none());
    }

    #[test]
    fn resize_routes_to_correct_surface() {
        let mut map = TerminalMap::new();
        let (mock1, state1) = make_mock(80, 24);
        let (mock2, state2) = make_mock(80, 24);
        map.insert(SurfaceId::new(1), mock1);
        map.insert(SurfaceId::new(2), mock2);

        let result = map.resize(SurfaceId::new(2), 120, 40, 8, 16);
        assert!(result.is_ok(), "resize should succeed for existing surface");

        assert_eq!(state1.borrow().resizes.len(), 0, "terminal 1 should not be resized");
        assert_eq!(state2.borrow().resizes.len(), 1, "terminal 2 should have 1 resize");
        assert_eq!(state2.borrow().resizes[0], (120, 40, 8, 16));
    }

    #[test]
    fn resize_unknown_surface_returns_error() {
        let mut map = TerminalMap::new();
        let result = map.resize(SurfaceId::new(999), 80, 24, 8, 16);
        assert!(result.is_err(), "resize for unknown surface should return Err");
    }

    #[test]
    fn insert_replaces_existing() {
        let mut map = TerminalMap::new();
        let (mock1, state1) = make_mock(80, 24);
        let (mock2, _state2) = make_mock(120, 40);

        map.insert(SurfaceId::new(1), mock1);
        let old = map.insert(SurfaceId::new(1), mock2);

        assert!(old.is_some(), "second insert should return the old terminal");
        assert_eq!(map.len(), 1, "map should still have 1 entry");
        let terminal = map.get(SurfaceId::new(1)).unwrap();
        assert_eq!(terminal.cols(), 120, "should have the new terminal's dimensions");
        // The old mock state should show no writes happened after replacement.
        assert_eq!(state1.borrow().writes.len(), 0);
    }

    #[test]
    fn is_empty_on_fresh_map() {
        let map = TerminalMap::new();
        assert!(map.is_empty());
    }

    #[test]
    fn get_mut_returns_mutable_reference() {
        let mut map = TerminalMap::new();
        let (mock, state) = make_mock(80, 24);
        map.insert(SurfaceId::new(1), mock);

        let terminal = map.get_mut(SurfaceId::new(1));
        assert!(terminal.is_some());
        terminal.unwrap().write_vt(b"direct write");

        assert_eq!(state.borrow().writes.len(), 1);
        assert_eq!(state.borrow().writes[0], b"direct write");
    }

    #[test]
    fn get_mut_nonexistent_returns_none() {
        let mut map = TerminalMap::new();
        assert!(map.get_mut(SurfaceId::new(999)).is_none());
    }

    // ================================================================
    // Unit 3: process_state_update
    // ================================================================

    #[test]
    fn process_pty_output_calls_write_vt() {
        let mut map = TerminalMap::new();
        let (mock, state) = make_mock(80, 24);
        map.insert(SurfaceId::new(1), mock);

        let update =
            StateUpdate::PtyOutput { surface_id: SurfaceId::new(1), data: b"test data".to_vec() };
        let handled = process_state_update(&update, &mut map);

        assert!(handled, "PtyOutput should be handled");
        assert_eq!(state.borrow().writes.len(), 1);
        assert_eq!(state.borrow().writes[0], b"test data");
    }

    #[test]
    fn process_surface_exited_removes_terminal() {
        let mut map = TerminalMap::new();
        let (mock, _state) = make_mock(80, 24);
        map.insert(SurfaceId::new(1), mock);
        assert_eq!(map.len(), 1);

        let update =
            StateUpdate::SurfaceExited { surface_id: SurfaceId::new(1), exit_code: Some(0) };
        let handled = process_state_update(&update, &mut map);

        assert!(handled, "SurfaceExited should be handled");
        assert_eq!(map.len(), 0, "terminal should be removed after SurfaceExited");
    }

    #[test]
    fn process_pty_output_unknown_surface_no_panic() {
        let mut map = TerminalMap::new();

        let update =
            StateUpdate::PtyOutput { surface_id: SurfaceId::new(999), data: b"orphan".to_vec() };
        let handled = process_state_update(&update, &mut map);

        // Should still be "handled" (recognized variant), just no terminal to write to.
        assert!(handled, "PtyOutput should be recognized even for unknown surface");
    }

    #[test]
    fn process_surface_exited_unknown_surface_no_panic() {
        let mut map = TerminalMap::new();

        let update =
            StateUpdate::SurfaceExited { surface_id: SurfaceId::new(999), exit_code: None };
        let handled = process_state_update(&update, &mut map);

        assert!(handled, "SurfaceExited should be recognized even for unknown surface");
    }

    #[test]
    fn process_multiple_pty_outputs_in_order() {
        let mut map = TerminalMap::new();
        let (mock, state) = make_mock(80, 24);
        map.insert(SurfaceId::new(1), mock);

        let updates = vec![
            StateUpdate::PtyOutput { surface_id: SurfaceId::new(1), data: b"first".to_vec() },
            StateUpdate::PtyOutput { surface_id: SurfaceId::new(1), data: b"second".to_vec() },
            StateUpdate::PtyOutput { surface_id: SurfaceId::new(1), data: b"third".to_vec() },
        ];

        for update in &updates {
            process_state_update(update, &mut map);
        }

        let writes = &state.borrow().writes;
        assert_eq!(writes.len(), 3, "should have 3 writes");
        assert_eq!(writes[0], b"first");
        assert_eq!(writes[1], b"second");
        assert_eq!(writes[2], b"third");
    }

    #[test]
    fn process_unrelated_state_update_returns_false() {
        let mut map = TerminalMap::new();

        let update = StateUpdate::ActorError {
            actor_name: "test".to_string(),
            message: "something".to_string(),
        };
        let handled = process_state_update(&update, &mut map);

        assert!(!handled, "unrelated StateUpdate variant should return false");
    }

    // ================================================================
    // Unit 4: compute_pane_cells
    // ================================================================

    #[test]
    fn compute_pane_cells_standard_dimensions() {
        // 800px wide, 600px tall, 8px wide cells, 16px tall cells
        let (cols, rows) = compute_pane_cells(800.0, 600.0, 8, 16);
        assert_eq!(cols, 100, "800 / 8 = 100 cols");
        assert_eq!(rows, 37, "600 / 16 = 37.5, floor = 37 rows");
    }

    #[test]
    fn compute_pane_cells_exact_fit() {
        let (cols, rows) = compute_pane_cells(640.0, 384.0, 8, 16);
        assert_eq!(cols, 80, "640 / 8 = 80 cols exactly");
        assert_eq!(rows, 24, "384 / 16 = 24 rows exactly");
    }

    #[test]
    fn compute_pane_cells_minimum_clamp() {
        // Very small pane (smaller than a single cell)
        let (cols, rows) = compute_pane_cells(3.0, 5.0, 8, 16);
        assert_eq!(cols, 1, "cols should be clamped to at least 1");
        assert_eq!(rows, 1, "rows should be clamped to at least 1");
    }

    #[test]
    fn compute_pane_cells_zero_width_clamps() {
        let (cols, rows) = compute_pane_cells(0.0, 0.0, 8, 16);
        assert_eq!(cols, 1, "zero width should clamp to 1 col");
        assert_eq!(rows, 1, "zero height should clamp to 1 row");
    }

    /// Zero cell dimensions hit `debug_assert!` in debug builds, so this test
    /// only exercises the release-mode early-return fallback.
    #[test]
    #[cfg(not(debug_assertions))]
    fn compute_pane_cells_zero_cell_dimensions_returns_fallback() {
        // Zero cell dimensions should return (1, 1) rather than u16::MAX
        let (cols, rows) = compute_pane_cells(800.0, 600.0, 0, 16);
        assert_eq!(cols, 1, "zero cell_width_px should return fallback");
        assert_eq!(rows, 1, "zero cell_width_px should return fallback");

        let (cols, rows) = compute_pane_cells(800.0, 600.0, 8, 0);
        assert_eq!(cols, 1, "zero cell_height_px should return fallback");
        assert_eq!(rows, 1, "zero cell_height_px should return fallback");

        let (cols, rows) = compute_pane_cells(800.0, 600.0, 0, 0);
        assert_eq!(cols, 1, "both zero should return fallback");
        assert_eq!(rows, 1, "both zero should return fallback");
    }

    #[test]
    fn compute_pane_cells_fractional_dimensions() {
        // 805px / 8 = 100.625, should floor to 100
        // 610px / 16 = 38.125, should floor to 38
        let (cols, rows) = compute_pane_cells(805.0, 610.0, 8, 16);
        assert_eq!(cols, 100, "fractional cols should floor");
        assert_eq!(rows, 38, "fractional rows should floor");
    }

    #[test]
    fn compute_pane_cells_with_sidebar_offset() {
        // Window 1280px wide, sidebar 250px -> 1030px for terminal area
        let (cols, rows) = compute_pane_cells(1030.0, 800.0, 8, 16);
        assert_eq!(cols, 128, "1030 / 8 = 128.75, floor = 128");
        assert_eq!(rows, 50, "800 / 16 = 50 exactly");
    }

    #[test]
    fn compute_pane_cells_half_width_for_split() {
        // After horizontal split, each pane gets ~515px of 1030px
        let (cols, rows) = compute_pane_cells(515.0, 800.0, 8, 16);
        assert_eq!(cols, 64, "515 / 8 = 64.375, floor = 64");
        assert_eq!(rows, 50, "800 / 16 = 50");
    }

    // ================================================================
    // Unit 4: resize propagation with mock terminals
    // ================================================================

    #[test]
    fn resize_propagates_to_terminal() {
        let mut map = TerminalMap::new();
        let (mock, state) = make_mock(80, 24);
        map.insert(SurfaceId::new(1), mock);

        let result = map.resize(SurfaceId::new(1), 120, 40, 8, 16);
        assert!(result.is_ok());

        let s = state.borrow();
        assert_eq!(s.resizes.len(), 1);
        assert_eq!(s.resizes[0], (120, 40, 8, 16));
        assert_eq!(s.cols, 120);
        assert_eq!(s.rows, 40);
    }

    #[test]
    fn resize_multiple_surfaces_independently() {
        let mut map = TerminalMap::new();
        let (mock1, state1) = make_mock(80, 24);
        let (mock2, state2) = make_mock(80, 24);
        map.insert(SurfaceId::new(1), mock1);
        map.insert(SurfaceId::new(2), mock2);

        // Resize surface 1 to wide dimensions, surface 2 to narrow.
        map.resize(SurfaceId::new(1), 200, 50, 8, 16).unwrap();
        map.resize(SurfaceId::new(2), 40, 12, 8, 16).unwrap();

        assert_eq!(state1.borrow().cols, 200);
        assert_eq!(state1.borrow().rows, 50);
        assert_eq!(state2.borrow().cols, 40);
        assert_eq!(state2.borrow().rows, 12);
    }

    // ================================================================
    // Unit 5: handle_surface_exit in AppState
    // ================================================================

    fn setup_app_with_two_panes() -> (AppState, FocusManager, SurfaceId, SurfaceId) {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let surface1 = state.workspace(ws_id).unwrap().layout.surface_ids()[0];
        let pane1 = state.workspace(ws_id).unwrap().pane_ids()[0];
        focus.focus_surface(surface1);

        state.split_pane(ws_id, pane1, SplitDirection::Horizontal).expect("split should succeed");
        let surface2 = state.workspace(ws_id).unwrap().layout.surface_ids()[1];

        (state, focus, surface1, surface2)
    }

    #[test]
    fn exit_with_multiple_panes_removes_exited_pane() {
        let (mut state, mut focus, surface1, surface2) = setup_app_with_two_panes();
        let ws_id = state.active_workspace_id.unwrap();
        let pane_count_before = state.workspace(ws_id).unwrap().layout.pane_count();
        assert_eq!(pane_count_before, 2);

        // Focus on surface1, then surface2 exits.
        focus.focus_surface(surface1);
        let outcome = handle_surface_exit(surface2, &mut state, &mut focus);

        let pane_count_after = state.workspace(ws_id).unwrap().layout.pane_count();
        assert_eq!(pane_count_after, 1, "exited pane should be removed from layout");
        assert_eq!(
            outcome,
            SurfaceExitOutcome::PaneClosedFocusUnchanged,
            "focus was on surface1, not the exited surface2"
        );
        assert_eq!(focus.focused_surface(), Some(surface1), "focus should remain on surface1");
    }

    #[test]
    fn exit_with_single_pane_retains_workspace() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let surface = state.workspace(ws_id).unwrap().layout.surface_ids()[0];
        focus.focus_surface(surface);

        let ws_count_before = state.workspaces.len();
        let outcome = handle_surface_exit(surface, &mut state, &mut focus);

        assert_eq!(
            outcome,
            SurfaceExitOutcome::SinglePaneRetained,
            "single pane exit should retain the workspace"
        );
        assert_eq!(
            state.workspaces.len(),
            ws_count_before,
            "workspace should not be removed when single pane exits"
        );
        assert!(
            state.workspace(ws_id).is_some(),
            "workspace should still exist after single pane exit"
        );
    }

    #[test]
    fn exit_unknown_surface_no_panic() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let _ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));

        // An unknown surface exits -- should not panic.
        let outcome = handle_surface_exit(SurfaceId::new(99999), &mut state, &mut focus);
        assert_eq!(
            outcome,
            SurfaceExitOutcome::SurfaceNotFound,
            "exit of unknown surface should return SurfaceNotFound"
        );
    }

    #[test]
    fn exit_focused_surface_shifts_focus_to_sibling() {
        let (mut state, mut focus, surface1, surface2) = setup_app_with_two_panes();

        // Focus on surface2 (the one that will exit).
        focus.focus_surface(surface2);
        assert_eq!(focus.focused_surface(), Some(surface2));

        let outcome = handle_surface_exit(surface2, &mut state, &mut focus);

        // After exiting the focused surface, focus should shift to the remaining surface.
        assert_eq!(
            outcome,
            SurfaceExitOutcome::PaneClosedFocusShifted(surface1),
            "focus should shift to the remaining surface after focused pane exits"
        );
        assert_eq!(focus.focused_surface(), Some(surface1));
    }

    #[test]
    fn exit_non_focused_surface_preserves_focus() {
        let (mut state, mut focus, surface1, surface2) = setup_app_with_two_panes();

        // Focus on surface1, surface2 exits.
        focus.focus_surface(surface1);
        let outcome = handle_surface_exit(surface2, &mut state, &mut focus);

        assert_eq!(
            outcome,
            SurfaceExitOutcome::PaneClosedFocusUnchanged,
            "non-focused surface exit should not change focus"
        );
        assert_eq!(
            focus.focused_surface(),
            Some(surface1),
            "focus should remain on the non-exited surface"
        );
    }

    // ================================================================
    // VEI-77 Unit 2: TerminalWriter render_cells() extension
    // ================================================================

    /// Helper to build a `CellGrid` with known content.
    fn make_cell_grid(cols: u16, rows: u16, text: &str) -> veil_ghostty::CellGrid {
        let mut cells = Vec::new();
        let chars: Vec<char> = text.chars().collect();
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

    /// Mock that supports returning a configurable `CellGrid` from `render_cells()`.
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

    #[test]
    fn render_cells_default_returns_none() {
        // The default implementation of render_cells() returns None.
        let (mut mock, _state) = make_mock(80, 24);
        let result = mock.render_cells();
        assert!(result.is_none(), "default render_cells() should return None");
    }

    #[test]
    fn render_cells_mock_returns_configured_grid() {
        let grid = make_cell_grid(80, 24, "Hello");
        let mut mock = CellGridMockWriter::with_grid(grid);
        let result = mock.render_cells();
        assert!(result.is_some(), "mock with grid should return Some");
        let grid = result.unwrap();
        assert_eq!(grid.cols, 80);
        assert_eq!(grid.rows, 24);
        assert_eq!(grid.cells[0][0].graphemes, vec!['H']);
        assert_eq!(grid.cells[0][1].graphemes, vec!['e']);
    }

    #[test]
    fn render_cells_mock_without_grid_returns_none() {
        let mut mock = CellGridMockWriter::without_grid(80, 24);
        let result = mock.render_cells();
        assert!(result.is_none(), "mock without grid should return None");
    }

    #[test]
    fn terminal_map_render_cells_via_get_mut() {
        let grid = make_cell_grid(80, 24, "Test");
        let mut map = TerminalMap::new();
        map.insert(SurfaceId::new(1), Box::new(CellGridMockWriter::with_grid(grid)));

        let terminal = map.get_mut(SurfaceId::new(1)).expect("terminal should exist");
        let result = terminal.render_cells();
        assert!(result.is_some(), "render_cells via get_mut should return Some");
        let grid = result.unwrap();
        assert_eq!(grid.cells[0][0].graphemes, vec!['T']);
    }

    #[test]
    fn terminal_map_mixed_some_none_render_cells() {
        let grid = make_cell_grid(80, 24, "A");
        let mut map = TerminalMap::new();
        map.insert(SurfaceId::new(1), Box::new(CellGridMockWriter::with_grid(grid)));
        map.insert(SurfaceId::new(2), Box::new(CellGridMockWriter::without_grid(80, 24)));

        let t1 = map.get_mut(SurfaceId::new(1)).unwrap();
        assert!(t1.render_cells().is_some(), "terminal 1 should have cell grid");

        let t2 = map.get_mut(SurfaceId::new(2)).unwrap();
        assert!(t2.render_cells().is_none(), "terminal 2 should not have cell grid");
    }

    #[test]
    fn render_cells_grid_contains_cursor_state() {
        let mut grid = make_cell_grid(80, 24, "");
        grid.cursor.x = 5;
        grid.cursor.y = 3;
        grid.cursor.visible = true;
        let mut mock = CellGridMockWriter::with_grid(grid);
        let result = mock.render_cells().unwrap();
        assert_eq!(result.cursor.x, 5);
        assert_eq!(result.cursor.y, 3);
        assert!(result.cursor.visible);
    }

    #[test]
    fn render_cells_grid_contains_colors() {
        let mut grid = make_cell_grid(80, 24, "");
        grid.colors.foreground = veil_ghostty::Color { r: 200, g: 200, b: 200 };
        grid.colors.background = veil_ghostty::Color { r: 30, g: 30, b: 30 };
        let mut mock = CellGridMockWriter::with_grid(grid);
        let result = mock.render_cells().unwrap();
        assert_eq!(result.colors.foreground.r, 200);
        assert_eq!(result.colors.background.r, 30);
    }

    // ================================================================
    // VEI-81 Unit 1: GhosttyTerminalWriter via create_ghostty_terminal
    // ================================================================

    #[cfg(not(no_libghosty))]
    #[test]
    fn create_ghostty_terminal_happy_path() {
        let writer = create_ghostty_terminal(80, 24);
        assert!(writer.is_some(), "create_ghostty_terminal(80, 24) should return Some");
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn ghostty_terminal_writer_cols_rows() {
        let writer =
            create_ghostty_terminal(80, 24).expect("create_ghostty_terminal should succeed");
        assert_eq!(writer.cols(), 80, "cols() should return 80 after creation with cols=80");
        assert_eq!(writer.rows(), 24, "rows() should return 24 after creation with rows=24");
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn ghostty_terminal_writer_write_vt_no_panic() {
        let mut writer =
            create_ghostty_terminal(80, 24).expect("create_ghostty_terminal should succeed");
        // Writing VT data should not panic. This verifies the FFI delegation works.
        writer.write_vt(b"hello world");
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn ghostty_terminal_writer_resize() {
        let mut writer =
            create_ghostty_terminal(80, 24).expect("create_ghostty_terminal should succeed");
        let result = writer.resize(120, 40, 8, 16);
        assert!(result.is_ok(), "resize should succeed");
        assert_eq!(writer.cols(), 120, "cols() should return 120 after resize");
        assert_eq!(writer.rows(), 40, "rows() should return 40 after resize");
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn ghostty_terminal_writer_render_cells_returns_none() {
        let mut writer =
            create_ghostty_terminal(80, 24).expect("create_ghostty_terminal should succeed");
        // MVP: render_cells() returns None before cell iteration FFI is wired (VEI-77).
        let result = writer.render_cells();
        assert!(result.is_none(), "render_cells() should return None in MVP (before cell FFI)");
    }
}
