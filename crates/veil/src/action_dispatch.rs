//! Key action dispatch -- translates `KeyAction` into state mutations and
//! side effects (`ActionEffect`). The event loop processes the returned
//! effects (PTY spawn/close, window redraw).

use std::path::PathBuf;

use veil_core::focus::FocusManager;
use veil_core::keyboard::KeyAction;
use veil_core::layout::{compute_layout, Rect};
use veil_core::navigation::{find_pane_in_direction, Direction};
use veil_core::state::AppState;
use veil_core::workspace::{SplitDirection, SurfaceId, WorkspaceId};

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
    match action {
        KeyAction::SplitHorizontal => dispatch_split(app_state, focus, SplitDirection::Horizontal),
        KeyAction::SplitVertical => dispatch_split(app_state, focus, SplitDirection::Vertical),
        KeyAction::ClosePane => dispatch_close_pane(app_state, focus),
        KeyAction::FocusNextPane => dispatch_focus_cycle(app_state, focus, true),
        KeyAction::FocusPreviousPane => dispatch_focus_cycle(app_state, focus, false),
        KeyAction::FocusPaneLeft => {
            dispatch_focus_direction(app_state, focus, Direction::Left, window_width, window_height)
        }
        KeyAction::FocusPaneRight => dispatch_focus_direction(
            app_state,
            focus,
            Direction::Right,
            window_width,
            window_height,
        ),
        KeyAction::FocusPaneUp => {
            dispatch_focus_direction(app_state, focus, Direction::Up, window_width, window_height)
        }
        KeyAction::FocusPaneDown => {
            dispatch_focus_direction(app_state, focus, Direction::Down, window_width, window_height)
        }
        KeyAction::ToggleSidebar => {
            app_state.toggle_sidebar();
            vec![ActionEffect::Redraw]
        }
        KeyAction::ZoomPane => dispatch_zoom(app_state, focus),
        KeyAction::CreateWorkspace => dispatch_create_workspace(app_state, focus),
        KeyAction::CloseWorkspace => dispatch_close_workspace(app_state, focus),
        KeyAction::SwitchWorkspace(n) => dispatch_switch_workspace(app_state, focus, *n),
        KeyAction::SwitchToWorkspacesTab => {
            app_state.set_sidebar_tab(veil_core::state::SidebarTab::Workspaces);
            vec![ActionEffect::Redraw]
        }
        KeyAction::SwitchToConversationsTab => {
            app_state.set_sidebar_tab(veil_core::state::SidebarTab::Conversations);
            vec![ActionEffect::Redraw]
        }
        KeyAction::FocusSidebar => {
            focus.focus_sidebar();
            vec![ActionEffect::Redraw]
        }
        KeyAction::FocusTerminal => dispatch_focus_terminal(app_state, focus),
        KeyAction::RenameWorkspace => vec![],
    }
}

fn focused_pane_context(
    app_state: &AppState,
    focus: &FocusManager,
) -> Option<(WorkspaceId, SurfaceId)> {
    let ws_id = app_state.active_workspace_id?;
    let surface_id = focus.focused_surface()?;
    Some((ws_id, surface_id))
}

fn dispatch_split(
    app_state: &mut AppState,
    focus: &FocusManager,
    direction: SplitDirection,
) -> Vec<ActionEffect> {
    let Some((ws_id, surface_id)) = focused_pane_context(app_state, focus) else {
        return vec![];
    };
    let Some(pane_id) =
        app_state.workspace(ws_id).and_then(|ws| ws.pane_id_for_surface(surface_id))
    else {
        return vec![];
    };
    let cwd = app_state.workspace(ws_id).map_or(PathBuf::new(), |ws| ws.working_directory.clone());
    match app_state.split_pane(ws_id, pane_id, direction) {
        Ok((_new_pane_id, new_surface_id)) => {
            vec![
                ActionEffect::SpawnPty { surface_id: new_surface_id, working_directory: cwd },
                ActionEffect::Redraw,
            ]
        }
        Err(_) => vec![],
    }
}

fn dispatch_close_pane(app_state: &mut AppState, focus: &mut FocusManager) -> Vec<ActionEffect> {
    let Some((ws_id, surface_id)) = focused_pane_context(app_state, focus) else {
        return vec![];
    };
    let Some(pane_id) =
        app_state.workspace(ws_id).and_then(|ws| ws.pane_id_for_surface(surface_id))
    else {
        return vec![];
    };
    let pane_count = app_state.workspace(ws_id).map_or(0, |ws| ws.layout.pane_count());
    if pane_count <= 1 {
        if app_state.workspaces.len() > 1 {
            return dispatch_close_workspace(app_state, focus);
        }
        return vec![];
    }
    let surfaces_before =
        app_state.workspace(ws_id).map_or(Vec::new(), |ws| ws.layout.surface_ids());
    match app_state.close_pane(ws_id, pane_id) {
        Ok(_) => {
            let surfaces_after =
                app_state.workspace(ws_id).map_or(Vec::new(), |ws| ws.layout.surface_ids());
            if let Some(pos) = surfaces_before.iter().position(|s| *s == surface_id) {
                let idx = if pos < surfaces_after.len() {
                    pos
                } else {
                    surfaces_after.len().saturating_sub(1)
                };
                if let Some(&new_surface) = surfaces_after.get(idx) {
                    focus.focus_surface(new_surface);
                }
            } else if let Some(&first) = surfaces_after.first() {
                focus.focus_surface(first);
            }
            vec![ActionEffect::ClosePty { surface_id }, ActionEffect::Redraw]
        }
        Err(_) => vec![],
    }
}

fn dispatch_focus_cycle(
    app_state: &AppState,
    focus: &mut FocusManager,
    forward: bool,
) -> Vec<ActionEffect> {
    let Some((ws_id, current_surface)) = focused_pane_context(app_state, focus) else {
        return vec![];
    };
    let Some(ws) = app_state.workspace(ws_id) else {
        return vec![];
    };
    let surfaces = ws.layout.surface_ids();
    if surfaces.len() <= 1 {
        return vec![ActionEffect::Redraw];
    }
    let Some(current_idx) = surfaces.iter().position(|s| *s == current_surface) else {
        return vec![];
    };
    let next_idx = if forward {
        (current_idx + 1) % surfaces.len()
    } else {
        (current_idx + surfaces.len() - 1) % surfaces.len()
    };
    focus.focus_surface(surfaces[next_idx]);
    vec![ActionEffect::Redraw]
}

fn dispatch_focus_direction(
    app_state: &AppState,
    focus: &mut FocusManager,
    direction: Direction,
    window_width: u32,
    window_height: u32,
) -> Vec<ActionEffect> {
    let Some((ws_id, current_surface)) = focused_pane_context(app_state, focus) else {
        return vec![];
    };
    let Some(ws) = app_state.workspace(ws_id) else {
        return vec![];
    };
    let Some(focused_pane) = ws.pane_id_for_surface(current_surface) else {
        return vec![];
    };
    #[allow(clippy::cast_precision_loss)] // window dimensions fit comfortably in f32
    let available =
        Rect { x: 0.0, y: 0.0, width: window_width as f32, height: window_height as f32 };
    let layouts = compute_layout(&ws.layout, available, ws.zoomed_pane);
    match find_pane_in_direction(&layouts, focused_pane, direction) {
        Some(target_pane) => {
            if let Some(target_surface) =
                layouts.iter().find(|l| l.pane_id == target_pane).map(|l| l.surface_id)
            {
                focus.focus_surface(target_surface);
                vec![ActionEffect::Redraw]
            } else {
                vec![]
            }
        }
        None => vec![],
    }
}

fn dispatch_zoom(app_state: &mut AppState, focus: &FocusManager) -> Vec<ActionEffect> {
    let Some((ws_id, surface_id)) = focused_pane_context(app_state, focus) else {
        return vec![];
    };
    let Some(pane_id) =
        app_state.workspace(ws_id).and_then(|ws| ws.pane_id_for_surface(surface_id))
    else {
        return vec![];
    };
    match app_state.toggle_zoom(ws_id, pane_id) {
        Ok(_) => vec![ActionEffect::Redraw],
        Err(_) => vec![],
    }
}

fn dispatch_create_workspace(
    app_state: &mut AppState,
    focus: &mut FocusManager,
) -> Vec<ActionEffect> {
    let cwd = app_state
        .active_workspace()
        .map_or_else(|| PathBuf::from("/"), |ws| ws.working_directory.clone());
    let ws_id = app_state.create_workspace("workspace".to_string(), cwd.clone());
    let _ = app_state.set_active_workspace(ws_id);
    let Some(ws) = app_state.workspace(ws_id) else {
        return vec![];
    };
    let surface_id = ws.layout.surface_ids()[0];
    focus.focus_surface(surface_id);
    vec![ActionEffect::SpawnPty { surface_id, working_directory: cwd }, ActionEffect::Redraw]
}

fn dispatch_close_workspace(
    app_state: &mut AppState,
    focus: &mut FocusManager,
) -> Vec<ActionEffect> {
    let Some(ws_id) = app_state.active_workspace_id else {
        return vec![];
    };
    if app_state.workspaces.len() <= 1 {
        return vec![];
    }
    let surfaces = app_state.workspace(ws_id).map_or(Vec::new(), |ws| ws.layout.surface_ids());
    match app_state.close_workspace(ws_id) {
        Ok(_) => {
            let mut effects: Vec<ActionEffect> = surfaces
                .into_iter()
                .map(|sid| ActionEffect::ClosePty { surface_id: sid })
                .collect();
            if let Some(new_ws) = app_state.workspaces.first() {
                let new_ws_id = new_ws.id;
                let _ = app_state.set_active_workspace(new_ws_id);
                if let Some(ws) = app_state.workspace(new_ws_id) {
                    if let Some(&first_surface) = ws.layout.surface_ids().first() {
                        focus.focus_surface(first_surface);
                    }
                }
            }
            effects.push(ActionEffect::Redraw);
            effects
        }
        Err(_) => vec![],
    }
}

fn dispatch_switch_workspace(
    app_state: &mut AppState,
    focus: &mut FocusManager,
    n: u8,
) -> Vec<ActionEffect> {
    let idx = (n as usize).saturating_sub(1);
    if idx >= app_state.workspaces.len() {
        return vec![];
    }
    let target_ws_id = app_state.workspaces[idx].id;
    if app_state.set_active_workspace(target_ws_id).is_err() {
        return vec![];
    }
    if let Some(ws) = app_state.workspace(target_ws_id) {
        if let Some(&first_surface) = ws.layout.surface_ids().first() {
            focus.focus_surface(first_surface);
        }
    }
    vec![ActionEffect::Redraw]
}

fn dispatch_focus_terminal(app_state: &AppState, focus: &mut FocusManager) -> Vec<ActionEffect> {
    if let Some(ws) = app_state.active_workspace() {
        if let Some(&first_surface) = ws.layout.surface_ids().first() {
            focus.focus_surface(first_surface);
            return vec![ActionEffect::Redraw];
        }
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use veil_core::focus::FocusManager;
    use veil_core::keyboard::KeyAction;
    use veil_core::state::{AppState, SidebarTab};
    use veil_core::workspace::SplitDirection;

    // ================================================================
    // Helpers
    // ================================================================

    const W: u32 = 1280;
    const H: u32 = 800;

    /// Set up an `AppState` with one workspace and one pane, focus on the root surface.
    fn setup_single_pane() -> (AppState, FocusManager, WorkspaceId) {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws_id = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp"));
        let surface_id = state.workspace(ws_id).unwrap().layout.surface_ids()[0];
        focus.focus_surface(surface_id);
        (state, focus, ws_id)
    }

    /// Set up an `AppState` with one workspace and two panes (horizontal split).
    /// Focus is on the first pane.
    fn setup_two_panes() -> (AppState, FocusManager, WorkspaceId) {
        let (mut state, mut focus, ws_id) = setup_single_pane();
        let pane_id = state.workspace(ws_id).unwrap().pane_ids()[0];
        state.split_pane(ws_id, pane_id, SplitDirection::Horizontal).expect("split should succeed");
        let first_surface = state.workspace(ws_id).unwrap().layout.surface_ids()[0];
        focus.focus_surface(first_surface);
        (state, focus, ws_id)
    }

    /// Set up an `AppState` with one workspace and three panes.
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

    // ================================================================
    // Test 1: SplitHorizontal creates new pane and returns SpawnPty + Redraw
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

        let effects = dispatch_action(&KeyAction::SplitHorizontal, &mut state, &mut focus, W, H);
        assert!(effects.is_empty(), "split with no focus should return no effects");
    }

    // ================================================================
    // Test 4: Split with no active workspace returns empty
    // ================================================================

    #[test]
    fn split_with_no_active_workspace_returns_empty() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();

        let effects = dispatch_action(&KeyAction::SplitHorizontal, &mut state, &mut focus, W, H);
        assert!(effects.is_empty(), "split with no active workspace should return empty effects");

        let effects = dispatch_action(&KeyAction::SplitVertical, &mut state, &mut focus, W, H);
        assert!(
            effects.is_empty(),
            "split vertical with no active workspace should return empty effects"
        );
    }

    // ================================================================
    // Test 5: ClosePane removes pane and returns ClosePty + Redraw
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
    // Test 7: ClosePane on last pane of only workspace is no-op
    // ================================================================

    #[test]
    fn close_pane_last_pane_only_workspace_is_noop() {
        let (mut state, mut focus, ws_id) = setup_single_pane();
        let pane_count_before = state.workspace(ws_id).unwrap().layout.pane_count();
        assert_eq!(pane_count_before, 1);

        let effects = dispatch_action(&KeyAction::ClosePane, &mut state, &mut focus, W, H);

        let pane_count_after = state.workspace(ws_id).unwrap().layout.pane_count();
        assert_eq!(pane_count_after, 1, "should not close the last pane");
        assert!(effects.is_empty(), "effects should be empty or None");
    }

    // ================================================================
    // Test 8: ClosePane on last pane with other workspaces closes workspace
    // ================================================================

    #[test]
    fn close_pane_last_pane_with_other_workspaces_closes_workspace() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let _ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));
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
    // Test 9: FocusNextPane cycles forward
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
    // Test 10: FocusNextPane wraps around
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
    // Test 11: FocusPreviousPane wraps around
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

        dispatch_action(&KeyAction::ZoomPane, &mut state, &mut focus, W, H);
        assert!(
            state.workspace(ws_id).unwrap().zoomed_pane.is_none(),
            "pane should be unzoomed after second dispatch"
        );
    }

    // ================================================================
    // Test 14: ZoomPane with no focus is no-op
    // ================================================================

    #[test]
    fn zoom_pane_with_no_focus_is_noop() {
        let (mut state, mut focus, _ws_id) = setup_single_pane();
        focus.clear();

        let effects = dispatch_action(&KeyAction::ZoomPane, &mut state, &mut focus, W, H);

        assert!(effects.is_empty(), "ZoomPane with no focus should produce no effects");
    }

    // ================================================================
    // Test 15: CreateWorkspace adds workspace and spawns PTY
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
    // Test 16: SwitchWorkspace changes active
    // ================================================================

    #[test]
    fn switch_workspace_changes_active() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));
        let _ws3 = state.create_workspace("ws3".to_string(), PathBuf::from("/tmp/3"));
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
    // Test 17: SwitchWorkspace out of range is no-op
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
        assert!(effects.is_empty(), "out-of-range switch should produce no meaningful effects");
    }

    // ================================================================
    // Test 18: CloseWorkspace removes workspace and returns ClosePty
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
    // Test 18b: CloseWorkspace returns ClosePty for EACH surface
    // ================================================================

    #[test]
    fn close_workspace_returns_close_pty_for_each_surface() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let _ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));
        let ws1_pane = state.workspace(ws1).unwrap().pane_ids()[0];
        state.split_pane(ws1, ws1_pane, SplitDirection::Horizontal).expect("split should succeed");
        let ws1_surface_count = state.workspace(ws1).unwrap().layout.surface_ids().len();
        assert_eq!(ws1_surface_count, 2, "ws1 should have 2 surfaces after split");

        let ws1_surface = state.workspace(ws1).unwrap().layout.surface_ids()[0];
        focus.focus_surface(ws1_surface);
        state.set_active_workspace(ws1).unwrap();

        let effects = dispatch_action(&KeyAction::CloseWorkspace, &mut state, &mut focus, W, H);

        let close_pty_count =
            effects.iter().filter(|e| matches!(e, ActionEffect::ClosePty { .. })).count();
        assert_eq!(
            close_pty_count, ws1_surface_count,
            "should return one ClosePty per surface in the closed workspace"
        );
    }

    // ================================================================
    // Test 19: SwitchToWorkspacesTab changes tab
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
    // Test 20: SwitchToConversationsTab changes tab
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
    // Test 21: FocusSidebar changes focus target
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
    // Test 22: FocusTerminal returns focus to surface
    // ================================================================

    #[test]
    fn focus_terminal_returns_focus_to_surface() {
        let (mut state, mut focus, ws_id) = setup_single_pane();
        focus.focus_sidebar();
        assert!(!focus.is_surface_focused());

        let effects = dispatch_action(&KeyAction::FocusTerminal, &mut state, &mut focus, W, H);

        assert!(focus.is_surface_focused(), "after FocusTerminal, a surface should be focused");
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
    // Test 23: All actions with no active workspace return empty
    // ================================================================

    #[test]
    fn dispatch_with_no_workspace_returns_empty() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();

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
                "with no workspace, {action:?} should return empty effects",
            );
        }
    }

    // ================================================================
    // All actions exhaustive no-panic check
    // ================================================================

    #[test]
    fn all_actions_no_workspace_no_panic() {
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
    // Additional edge case tests
    // ================================================================

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
        assert!(effects.is_empty(), "RenameWorkspace should produce no meaningful effects");
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
}
