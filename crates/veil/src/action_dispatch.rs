//! Action dispatcher -- translates `KeyAction` into state mutations and
//! side effects (`ActionEffect`).
//!
//! This is a pure-logic module: it operates on `AppState` and `FocusManager`
//! without touching PTYs, windows, or the GPU. The event loop reads the
//! returned `ActionEffect` values and executes them.

use std::path::PathBuf;

use veil_core::focus::FocusManager;
use veil_core::keyboard::KeyAction;
#[allow(unused_imports)] // Used by implementation, not stubs
use veil_core::layout::{compute_layout, PaneLayout, Rect};
#[allow(unused_imports)] // Used by implementation, not stubs
use veil_core::navigation::{find_pane_in_direction, Direction};
use veil_core::state::AppState;
#[allow(unused_imports)] // Used by implementation, not stubs
use veil_core::workspace::{PaneId, PaneNode, SplitDirection, SurfaceId, WorkspaceId};

/// Side effects produced by dispatching a key action.
/// The event loop reads these and executes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionEffect {
    /// Spawn a new PTY for the given surface in the given working directory.
    SpawnPty {
        /// Surface that needs a PTY.
        surface_id: SurfaceId,
        /// Working directory for the new shell.
        working_directory: PathBuf,
    },
    /// Close the PTY for the given surface.
    ClosePty {
        /// Surface whose PTY should be closed.
        surface_id: SurfaceId,
    },
    /// Request a redraw (layout changed).
    Redraw,
    /// No effect (action was a no-op or not applicable).
    None,
}

/// Dispatch a key action against the current state.
/// Returns a list of effects for the event loop to execute.
pub fn dispatch_action(
    action: &KeyAction,
    app_state: &mut AppState,
    focus: &mut FocusManager,
    window_width: u32,
    window_height: u32,
) -> Vec<ActionEffect> {
    // Stub: always returns empty vec. Implementation will handle each action.
    let _ = action;
    let _ = app_state;
    let _ = focus;
    let _ = window_width;
    let _ = window_height;
    vec![]
}

/// Find the `PaneId` that corresponds to a `SurfaceId` in the given layout tree.
fn surface_to_pane_id(layout: &PaneNode, surface_id: SurfaceId) -> Option<PaneId> {
    match layout {
        PaneNode::Leaf { pane_id, surface_id: sid } => {
            if *sid == surface_id {
                Some(*pane_id)
            } else {
                Option::None
            }
        }
        PaneNode::Split { first, second, .. } => {
            surface_to_pane_id(first, surface_id).or_else(|| surface_to_pane_id(second, surface_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use veil_core::focus::FocusManager;
    use veil_core::keyboard::KeyAction;
    use veil_core::state::{AppState, SidebarTab};
    use veil_core::workspace::{PaneId, SplitDirection, SurfaceId};

    // ================================================================
    // Helpers
    // ================================================================

    /// Set up an AppState with one workspace and one pane, focus on the root surface.
    fn setup_single_pane() -> (AppState, FocusManager, WorkspaceId) {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws_id = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp"));
        let surface_id = state.workspace(ws_id).unwrap().layout.surface_ids()[0];
        focus.focus_surface(surface_id);
        (state, focus, ws_id)
    }

    /// Set up an AppState with one workspace and two panes (horizontal split).
    /// Focus is on the first pane.
    fn setup_two_panes() -> (AppState, FocusManager, WorkspaceId) {
        let (mut state, mut focus, ws_id) = setup_single_pane();
        let pane_id = state.workspace(ws_id).unwrap().pane_ids()[0];
        state.split_pane(ws_id, pane_id, SplitDirection::Horizontal).expect("split should succeed");
        // Focus stays on the first surface
        let first_surface = state.workspace(ws_id).unwrap().layout.surface_ids()[0];
        focus.focus_surface(first_surface);
        (state, focus, ws_id)
    }

    /// Set up an AppState with one workspace and three panes.
    /// Split first pane horizontally, then split the second pane vertically.
    fn setup_three_panes() -> (AppState, FocusManager, WorkspaceId) {
        let (mut state, mut focus, ws_id) = setup_single_pane();
        let first_pane = state.workspace(ws_id).unwrap().pane_ids()[0];
        state
            .split_pane(ws_id, first_pane, SplitDirection::Horizontal)
            .expect("split 1 should succeed");
        let second_pane = state.workspace(ws_id).unwrap().pane_ids()[1];
        state
            .split_pane(ws_id, second_pane, SplitDirection::Vertical)
            .expect("split 2 should succeed");
        let first_surface = state.workspace(ws_id).unwrap().layout.surface_ids()[0];
        focus.focus_surface(first_surface);
        (state, focus, ws_id)
    }

    const W: u32 = 1280;
    const H: u32 = 800;

    // ================================================================
    // Test 1: SplitHorizontal creates new pane
    // ================================================================

    #[test]
    fn split_horizontal_creates_new_pane() {
        let (mut state, mut focus, ws_id) = setup_single_pane();
        let pane_count_before = state.workspace(ws_id).unwrap().layout.pane_count();

        let effects = dispatch_action(&KeyAction::SplitHorizontal, &mut state, &mut focus, W, H);

        let pane_count_after = state.workspace(ws_id).unwrap().layout.pane_count();
        assert_eq!(pane_count_after, pane_count_before + 1, "SplitHorizontal should add one pane");
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::SpawnPty { .. })),
            "effects should include SpawnPty"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );
    }

    // ================================================================
    // Test 2: SplitVertical creates new pane
    // ================================================================

    #[test]
    fn split_vertical_creates_new_pane() {
        let (mut state, mut focus, ws_id) = setup_single_pane();
        let pane_count_before = state.workspace(ws_id).unwrap().layout.pane_count();

        let effects = dispatch_action(&KeyAction::SplitVertical, &mut state, &mut focus, W, H);

        let pane_count_after = state.workspace(ws_id).unwrap().layout.pane_count();
        assert_eq!(pane_count_after, pane_count_before + 1, "SplitVertical should add one pane");
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::SpawnPty { .. })),
            "effects should include SpawnPty"
        );
    }

    // ================================================================
    // Test 3: SplitHorizontal with no focus returns no effects
    // ================================================================

    #[test]
    fn split_horizontal_no_focus_returns_empty() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let _ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        // focus is not set

        let effects = dispatch_action(&KeyAction::SplitHorizontal, &mut state, &mut focus, W, H);
        assert!(effects.is_empty(), "split with no focus should return no effects");
    }

    // ================================================================
    // Test 4: ClosePane removes pane
    // ================================================================

    #[test]
    fn close_pane_removes_pane() {
        let (mut state, mut focus, ws_id) = setup_two_panes();
        let pane_count_before = state.workspace(ws_id).unwrap().layout.pane_count();
        assert_eq!(pane_count_before, 2);

        let effects = dispatch_action(&KeyAction::ClosePane, &mut state, &mut focus, W, H);

        let pane_count_after = state.workspace(ws_id).unwrap().layout.pane_count();
        assert_eq!(pane_count_after, 1, "ClosePane should remove one pane");
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::ClosePty { .. })),
            "effects should include ClosePty"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );
    }

    // ================================================================
    // Test 5: ClosePane on last pane of only workspace is no-op
    // ================================================================

    #[test]
    fn close_pane_last_pane_only_workspace_is_noop() {
        let (mut state, mut focus, ws_id) = setup_single_pane();
        let pane_count_before = state.workspace(ws_id).unwrap().layout.pane_count();
        assert_eq!(pane_count_before, 1);

        let effects = dispatch_action(&KeyAction::ClosePane, &mut state, &mut focus, W, H);

        let pane_count_after = state.workspace(ws_id).unwrap().layout.pane_count();
        assert_eq!(pane_count_after, 1, "should not close the last pane");
        assert!(
            effects.is_empty() || effects.iter().all(|e| matches!(e, ActionEffect::None)),
            "effects should be empty or None"
        );
    }

    // ================================================================
    // Test 6: ClosePane moves focus to sibling
    // ================================================================

    #[test]
    fn close_pane_moves_focus_to_sibling() {
        let (mut state, mut focus, ws_id) = setup_two_panes();
        let surfaces = state.workspace(ws_id).unwrap().layout.surface_ids();
        let focused_surface = surfaces[0];
        let other_surface = surfaces[1];
        focus.focus_surface(focused_surface);

        dispatch_action(&KeyAction::ClosePane, &mut state, &mut focus, W, H);

        let new_focus = focus.focused_surface();
        assert!(new_focus.is_some(), "focus should move to remaining pane after close");
        assert_eq!(new_focus.unwrap(), other_surface, "focus should be on the sibling pane");
    }

    // ================================================================
    // Test 7: FocusNextPane cycles forward
    // ================================================================

    #[test]
    fn focus_next_pane_cycles_forward() {
        let (mut state, mut focus, ws_id) = setup_three_panes();
        let surfaces = state.workspace(ws_id).unwrap().layout.surface_ids();
        let first_surface = surfaces[0];
        let second_surface = surfaces[1];
        focus.focus_surface(first_surface);

        dispatch_action(&KeyAction::FocusNextPane, &mut state, &mut focus, W, H);

        assert_eq!(
            focus.focused_surface(),
            Some(second_surface),
            "FocusNextPane should move to the second surface"
        );
    }

    // ================================================================
    // Test 8: FocusNextPane wraps around
    // ================================================================

    #[test]
    fn focus_next_pane_wraps_around() {
        let (mut state, mut focus, ws_id) = setup_three_panes();
        let surfaces = state.workspace(ws_id).unwrap().layout.surface_ids();
        let first_surface = surfaces[0];
        let last_surface = *surfaces.last().unwrap();
        focus.focus_surface(last_surface);

        dispatch_action(&KeyAction::FocusNextPane, &mut state, &mut focus, W, H);

        assert_eq!(
            focus.focused_surface(),
            Some(first_surface),
            "FocusNextPane from last should wrap to first"
        );
    }

    // ================================================================
    // Test 9: FocusPreviousPane wraps around
    // ================================================================

    #[test]
    fn focus_previous_pane_wraps_around() {
        let (mut state, mut focus, ws_id) = setup_three_panes();
        let surfaces = state.workspace(ws_id).unwrap().layout.surface_ids();
        let first_surface = surfaces[0];
        let last_surface = *surfaces.last().unwrap();
        focus.focus_surface(first_surface);

        dispatch_action(&KeyAction::FocusPreviousPane, &mut state, &mut focus, W, H);

        assert_eq!(
            focus.focused_surface(),
            Some(last_surface),
            "FocusPreviousPane from first should wrap to last"
        );
    }

    // ================================================================
    // Test 10: FocusPaneRight finds right neighbor
    // ================================================================

    #[test]
    fn focus_pane_right_finds_neighbor() {
        let (mut state, mut focus, ws_id) = setup_two_panes();
        let surfaces = state.workspace(ws_id).unwrap().layout.surface_ids();
        let left_surface = surfaces[0];
        let right_surface = surfaces[1];
        focus.focus_surface(left_surface);

        let effects = dispatch_action(&KeyAction::FocusPaneRight, &mut state, &mut focus, W, H);

        assert_eq!(
            focus.focused_surface(),
            Some(right_surface),
            "FocusPaneRight should move focus to the right pane"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );
    }

    // ================================================================
    // Test 11: FocusPaneRight at edge is no-op
    // ================================================================

    #[test]
    fn focus_pane_right_at_edge_is_noop() {
        let (mut state, mut focus, ws_id) = setup_two_panes();
        let surfaces = state.workspace(ws_id).unwrap().layout.surface_ids();
        let right_surface = surfaces[1];
        focus.focus_surface(right_surface);

        dispatch_action(&KeyAction::FocusPaneRight, &mut state, &mut focus, W, H);

        assert_eq!(
            focus.focused_surface(),
            Some(right_surface),
            "FocusPaneRight at right edge should not change focus"
        );
    }

    // ================================================================
    // Test 12: ToggleSidebar flips visibility
    // ================================================================

    #[test]
    fn toggle_sidebar_flips_visibility() {
        let (mut state, mut focus, _ws_id) = setup_single_pane();
        assert!(state.sidebar.visible, "sidebar should start visible");

        let effects = dispatch_action(&KeyAction::ToggleSidebar, &mut state, &mut focus, W, H);

        assert!(!state.sidebar.visible, "sidebar should be hidden after toggle");
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );

        dispatch_action(&KeyAction::ToggleSidebar, &mut state, &mut focus, W, H);
        assert!(state.sidebar.visible, "sidebar should be visible again after second toggle");
    }

    // ================================================================
    // Test 13: ZoomPane toggles zoom state
    // ================================================================

    #[test]
    fn zoom_pane_toggles_zoom() {
        let (mut state, mut focus, ws_id) = setup_two_panes();
        assert!(state.workspace(ws_id).unwrap().zoomed_pane.is_none(), "should start unzoomed");

        let effects = dispatch_action(&KeyAction::ZoomPane, &mut state, &mut focus, W, H);

        assert!(
            state.workspace(ws_id).unwrap().zoomed_pane.is_some(),
            "pane should be zoomed after dispatch"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );

        // Toggle again to unzoom
        dispatch_action(&KeyAction::ZoomPane, &mut state, &mut focus, W, H);
        assert!(
            state.workspace(ws_id).unwrap().zoomed_pane.is_none(),
            "pane should be unzoomed after second dispatch"
        );
    }

    // ================================================================
    // Test 14: CreateWorkspace adds workspace and spawns PTY
    // ================================================================

    #[test]
    fn create_workspace_adds_workspace_and_spawns_pty() {
        let (mut state, mut focus, _ws_id) = setup_single_pane();
        let ws_count_before = state.workspaces.len();

        let effects = dispatch_action(&KeyAction::CreateWorkspace, &mut state, &mut focus, W, H);

        assert_eq!(
            state.workspaces.len(),
            ws_count_before + 1,
            "CreateWorkspace should add a workspace"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::SpawnPty { .. })),
            "effects should include SpawnPty for the new workspace's root pane"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );
    }

    // ================================================================
    // Test 15: SwitchWorkspace changes active
    // ================================================================

    #[test]
    fn switch_workspace_changes_active() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));
        let _ws3 = state.create_workspace("ws3".to_string(), PathBuf::from("/tmp/3"));
        // Active is ws1 by default
        assert_eq!(state.active_workspace_id, Some(ws1));

        let effects = dispatch_action(&KeyAction::SwitchWorkspace(2), &mut state, &mut focus, W, H);

        assert_eq!(
            state.active_workspace_id,
            Some(ws2),
            "SwitchWorkspace(2) should activate the second workspace"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );
    }

    // ================================================================
    // Test 16: SwitchWorkspace out of range is no-op
    // ================================================================

    #[test]
    fn switch_workspace_out_of_range_is_noop() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let _ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));
        assert_eq!(state.active_workspace_id, Some(ws1));

        let effects = dispatch_action(&KeyAction::SwitchWorkspace(9), &mut state, &mut focus, W, H);

        assert_eq!(
            state.active_workspace_id,
            Some(ws1),
            "SwitchWorkspace(9) with only 2 workspaces should not change active"
        );
        assert!(
            effects.is_empty() || effects.iter().all(|e| matches!(e, ActionEffect::None)),
            "out-of-range switch should produce no meaningful effects"
        );
    }

    // ================================================================
    // Test 17: CloseWorkspace removes workspace
    // ================================================================

    #[test]
    fn close_workspace_removes_workspace() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let ws1_surfaces = state.workspace(ws1).unwrap().layout.surface_ids();
        focus.focus_surface(ws1_surfaces[0]);
        let _ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));
        assert_eq!(state.workspaces.len(), 2);

        // Make ws1 active and close it
        state.set_active_workspace(ws1).unwrap();
        let effects = dispatch_action(&KeyAction::CloseWorkspace, &mut state, &mut focus, W, H);

        assert_eq!(state.workspaces.len(), 1, "CloseWorkspace should remove a workspace");
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::ClosePty { .. })),
            "effects should include ClosePty for all surfaces"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );
    }

    // ================================================================
    // Test 18: SwitchToWorkspacesTab changes tab
    // ================================================================

    #[test]
    fn switch_to_workspaces_tab() {
        let (mut state, mut focus, _ws_id) = setup_single_pane();
        state.set_sidebar_tab(SidebarTab::Conversations);
        assert_eq!(state.sidebar.active_tab, SidebarTab::Conversations);

        let effects =
            dispatch_action(&KeyAction::SwitchToWorkspacesTab, &mut state, &mut focus, W, H);

        assert_eq!(
            state.sidebar.active_tab,
            SidebarTab::Workspaces,
            "tab should switch to Workspaces"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );
    }

    // ================================================================
    // Test 19: SwitchToConversationsTab changes tab
    // ================================================================

    #[test]
    fn switch_to_conversations_tab() {
        let (mut state, mut focus, _ws_id) = setup_single_pane();
        assert_eq!(state.sidebar.active_tab, SidebarTab::Workspaces);

        let effects =
            dispatch_action(&KeyAction::SwitchToConversationsTab, &mut state, &mut focus, W, H);

        assert_eq!(
            state.sidebar.active_tab,
            SidebarTab::Conversations,
            "tab should switch to Conversations"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );
    }

    // ================================================================
    // Test 20: FocusSidebar changes focus target
    // ================================================================

    #[test]
    fn focus_sidebar_changes_focus() {
        let (mut state, mut focus, _ws_id) = setup_single_pane();
        assert!(focus.is_surface_focused(), "should start with surface focus");

        let effects = dispatch_action(&KeyAction::FocusSidebar, &mut state, &mut focus, W, H);

        assert!(!focus.is_surface_focused(), "after FocusSidebar, surface should not be focused");
        assert_eq!(
            focus.current(),
            Some(veil_core::focus::FocusTarget::Sidebar),
            "focus target should be Sidebar"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );
    }

    // ================================================================
    // Test 21: FocusTerminal returns focus to surface
    // ================================================================

    #[test]
    fn focus_terminal_returns_focus_to_surface() {
        let (mut state, mut focus, ws_id) = setup_single_pane();
        // Switch to sidebar
        focus.focus_sidebar();
        assert!(!focus.is_surface_focused());

        let effects = dispatch_action(&KeyAction::FocusTerminal, &mut state, &mut focus, W, H);

        assert!(focus.is_surface_focused(), "after FocusTerminal, a surface should be focused");
        // Should focus the first surface in the active workspace
        let expected_surface = state.workspace(ws_id).unwrap().layout.surface_ids()[0];
        assert_eq!(
            focus.focused_surface(),
            Some(expected_surface),
            "should focus the first surface of the active workspace"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::Redraw)),
            "effects should include Redraw"
        );
    }

    // ================================================================
    // Test 22: Dispatch with no active workspace returns empty effects
    // ================================================================

    #[test]
    fn dispatch_with_no_workspace_returns_empty() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();

        // Test multiple actions -- all should return empty
        for action in &[
            KeyAction::SplitHorizontal,
            KeyAction::SplitVertical,
            KeyAction::ClosePane,
            KeyAction::FocusNextPane,
            KeyAction::FocusPreviousPane,
            KeyAction::ZoomPane,
            KeyAction::FocusPaneLeft,
            KeyAction::FocusPaneRight,
            KeyAction::FocusPaneUp,
            KeyAction::FocusPaneDown,
        ] {
            let effects = dispatch_action(action, &mut state, &mut focus, W, H);
            assert!(
                effects.is_empty(),
                "with no workspace, {:?} should return empty effects",
                action
            );
        }
    }

    // ================================================================
    // surface_to_pane_id helper
    // ================================================================

    #[test]
    fn surface_to_pane_id_finds_leaf() {
        let node = PaneNode::Leaf { pane_id: PaneId::new(1), surface_id: SurfaceId::new(10) };
        assert_eq!(surface_to_pane_id(&node, SurfaceId::new(10)), Some(PaneId::new(1)));
    }

    #[test]
    fn surface_to_pane_id_returns_none_for_missing() {
        let node = PaneNode::Leaf { pane_id: PaneId::new(1), surface_id: SurfaceId::new(10) };
        assert_eq!(surface_to_pane_id(&node, SurfaceId::new(99)), None);
    }

    #[test]
    fn surface_to_pane_id_searches_split() {
        let node = PaneNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneNode::Leaf {
                pane_id: PaneId::new(1),
                surface_id: SurfaceId::new(10),
            }),
            second: Box::new(PaneNode::Leaf {
                pane_id: PaneId::new(2),
                surface_id: SurfaceId::new(20),
            }),
        };
        assert_eq!(surface_to_pane_id(&node, SurfaceId::new(20)), Some(PaneId::new(2)));
    }

    // ================================================================
    // Additional edge cases
    // ================================================================

    #[test]
    fn split_horizontal_focus_stays_on_original() {
        let (mut state, mut focus, _ws_id) = setup_single_pane();
        let original_surface = focus.focused_surface().unwrap();

        dispatch_action(&KeyAction::SplitHorizontal, &mut state, &mut focus, W, H);

        assert_eq!(
            focus.focused_surface(),
            Some(original_surface),
            "focus should stay on the original pane after split"
        );
    }

    #[test]
    fn create_workspace_sets_active_and_focus() {
        let (mut state, mut focus, _ws_id) = setup_single_pane();
        let old_active = state.active_workspace_id;

        dispatch_action(&KeyAction::CreateWorkspace, &mut state, &mut focus, W, H);

        assert_ne!(
            state.active_workspace_id, old_active,
            "active workspace should change after CreateWorkspace"
        );
        assert!(focus.is_surface_focused(), "focus should be on the new workspace's root surface");
    }

    #[test]
    fn close_workspace_activates_adjacent() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));

        // Focus and activate ws1
        let ws1_surface = state.workspace(ws1).unwrap().layout.surface_ids()[0];
        focus.focus_surface(ws1_surface);
        state.set_active_workspace(ws1).unwrap();

        dispatch_action(&KeyAction::CloseWorkspace, &mut state, &mut focus, W, H);

        assert_eq!(
            state.active_workspace_id,
            Some(ws2),
            "after closing ws1, ws2 should become active"
        );
    }

    #[test]
    fn rename_workspace_is_noop() {
        let (mut state, mut focus, ws_id) = setup_single_pane();
        let name_before = state.workspace(ws_id).unwrap().name.clone();

        let effects = dispatch_action(&KeyAction::RenameWorkspace, &mut state, &mut focus, W, H);

        assert_eq!(
            state.workspace(ws_id).unwrap().name,
            name_before,
            "RenameWorkspace should not change the name (no-op for now)"
        );
        assert!(
            effects.is_empty() || effects.iter().all(|e| matches!(e, ActionEffect::None)),
            "RenameWorkspace should produce no meaningful effects"
        );
    }

    #[test]
    fn focus_pane_left_at_edge_is_noop() {
        let (mut state, mut focus, ws_id) = setup_two_panes();
        let surfaces = state.workspace(ws_id).unwrap().layout.surface_ids();
        let left_surface = surfaces[0];
        focus.focus_surface(left_surface);

        dispatch_action(&KeyAction::FocusPaneLeft, &mut state, &mut focus, W, H);

        assert_eq!(
            focus.focused_surface(),
            Some(left_surface),
            "FocusPaneLeft at left edge should not change focus"
        );
    }

    #[test]
    fn switch_workspace_focuses_first_surface() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let _ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));
        let ws2_surface = state.workspace(ws2).unwrap().layout.surface_ids()[0];

        dispatch_action(&KeyAction::SwitchWorkspace(2), &mut state, &mut focus, W, H);

        assert_eq!(
            focus.focused_surface(),
            Some(ws2_surface),
            "switching workspace should focus the first surface of the target workspace"
        );
    }

    // ================================================================
    // ClosePane on last pane with other workspaces closes workspace
    // ================================================================

    #[test]
    fn close_pane_last_pane_with_other_workspaces_closes_workspace() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let _ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));
        // Focus the only pane of ws1 and make ws1 active.
        let ws1_surface = state.workspace(ws1).unwrap().layout.surface_ids()[0];
        focus.focus_surface(ws1_surface);
        state.set_active_workspace(ws1).unwrap();
        let ws_count_before = state.workspaces.len();

        let effects = dispatch_action(&KeyAction::ClosePane, &mut state, &mut focus, W, H);

        assert_eq!(
            state.workspaces.len(),
            ws_count_before - 1,
            "closing the last pane with other workspaces should close the workspace"
        );
        assert!(
            effects.iter().any(|e| matches!(e, ActionEffect::ClosePty { .. })),
            "effects should include ClosePty"
        );
    }

    // ================================================================
    // ZoomPane with no focus is no-op
    // ================================================================

    #[test]
    fn zoom_pane_with_no_focus_is_noop() {
        let (mut state, mut focus, _ws_id) = setup_single_pane();
        focus.clear();

        let effects = dispatch_action(&KeyAction::ZoomPane, &mut state, &mut focus, W, H);

        assert!(
            effects.is_empty() || effects.iter().all(|e| matches!(e, ActionEffect::None)),
            "ZoomPane with no focus should produce no effects"
        );
    }

    // ================================================================
    // CloseWorkspace returns ClosePty for EACH surface
    // ================================================================

    #[test]
    fn close_workspace_returns_close_pty_for_each_surface() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let _ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));
        // Split ws1 so it has two surfaces.
        let ws1_pane = state.workspace(ws1).unwrap().pane_ids()[0];
        state
            .split_pane(ws1, ws1_pane, SplitDirection::Horizontal)
            .expect("split should succeed");
        let ws1_surface_count = state.workspace(ws1).unwrap().layout.surface_ids().len();
        assert_eq!(ws1_surface_count, 2, "ws1 should have 2 surfaces after split");

        let ws1_surface = state.workspace(ws1).unwrap().layout.surface_ids()[0];
        focus.focus_surface(ws1_surface);
        state.set_active_workspace(ws1).unwrap();

        let effects = dispatch_action(&KeyAction::CloseWorkspace, &mut state, &mut focus, W, H);

        let close_pty_count = effects
            .iter()
            .filter(|e| matches!(e, ActionEffect::ClosePty { .. }))
            .count();
        assert_eq!(
            close_pty_count, ws1_surface_count,
            "should return one ClosePty per surface in the closed workspace"
        );
    }

    // ================================================================
    // All actions with no active workspace return empty (exhaustive)
    // ================================================================

    #[test]
    fn all_actions_no_workspace_return_empty_effects() {
        let all_actions = [
            KeyAction::SplitHorizontal,
            KeyAction::SplitVertical,
            KeyAction::ClosePane,
            KeyAction::FocusNextPane,
            KeyAction::FocusPreviousPane,
            KeyAction::ZoomPane,
            KeyAction::ToggleSidebar,
            KeyAction::CreateWorkspace,
            KeyAction::CloseWorkspace,
            KeyAction::SwitchWorkspace(1),
            KeyAction::SwitchWorkspace(9),
            KeyAction::SwitchToWorkspacesTab,
            KeyAction::SwitchToConversationsTab,
            KeyAction::FocusSidebar,
            KeyAction::FocusTerminal,
            KeyAction::FocusPaneLeft,
            KeyAction::FocusPaneRight,
            KeyAction::FocusPaneUp,
            KeyAction::FocusPaneDown,
            KeyAction::RenameWorkspace,
        ];

        for action in &all_actions {
            let mut state = AppState::new();
            let mut focus = FocusManager::new();
            // Must not panic with no workspaces.
            let _effects = dispatch_action(action, &mut state, &mut focus, W, H);
        }
    }

    // ================================================================
    // Split with no active workspace returns empty
    // ================================================================

    #[test]
    fn split_with_no_active_workspace_returns_empty() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();

        let effects =
            dispatch_action(&KeyAction::SplitHorizontal, &mut state, &mut focus, W, H);
        assert!(
            effects.is_empty(),
            "split with no active workspace should return empty effects"
        );

        let effects =
            dispatch_action(&KeyAction::SplitVertical, &mut state, &mut focus, W, H);
        assert!(
            effects.is_empty(),
            "split vertical with no active workspace should return empty effects"
        );
    }
}
