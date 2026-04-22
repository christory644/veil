//! Sidebar event wiring — connects `SidebarResponse` to `AppState` mutations.
//!
//! This module contains the logic that interprets a `SidebarResponse` from the
//! UI layer and applies the corresponding state changes. Extracted from `main.rs`
//! to enable unit testing without a GPU or window.

use veil_core::focus::FocusManager;
use veil_core::state::AppState;
use veil_ui::sidebar::SidebarResponse;

/// Apply a `SidebarResponse` to `AppState` and `FocusManager`.
///
/// Handles tab switching, workspace switching, and focus updates.
/// Returns `Ok(())` on success, or a descriptive error string if
/// the workspace switch failed (e.g., stale workspace ID).
#[allow(clippy::unnecessary_wraps)]
pub fn apply_sidebar_response(
    _response: &SidebarResponse,
    _app_state: &mut AppState,
    _focus: &mut FocusManager,
) -> Result<(), String> {
    // Stub: does nothing so tests fail.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use veil_core::focus::FocusManager;
    use veil_core::state::{AppState, SidebarTab};
    use veil_core::workspace::WorkspaceId;
    use veil_ui::sidebar::SidebarResponse;

    // ================================================================
    // Helpers
    // ================================================================

    fn state_with_two_workspaces() -> (AppState, WorkspaceId, WorkspaceId) {
        let mut state = AppState::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/ws1"));
        let ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/ws2"));
        (state, ws1, ws2)
    }

    // ================================================================
    // Unit 4: Switch workspace via SidebarResponse
    // ================================================================

    #[test]
    fn switch_workspace_updates_active_workspace_id() {
        let (mut state, ws1, ws2) = state_with_two_workspaces();
        assert_eq!(state.active_workspace_id, Some(ws1));

        let mut focus = FocusManager::new();
        let response = SidebarResponse { switch_to_workspace: Some(ws2), switch_tab: None };

        apply_sidebar_response(&response, &mut state, &mut focus).expect("should succeed");
        assert_eq!(state.active_workspace_id, Some(ws2), "active workspace should change to ws2");
    }

    #[test]
    fn switch_workspace_updates_focus_to_new_workspace_surface() {
        let (mut state, _ws1, ws2) = state_with_two_workspaces();
        let mut focus = FocusManager::new();

        // Focus on ws1's surface initially
        let ws1_surface = state.workspaces[0].layout.surface_ids()[0];
        focus.focus_surface(ws1_surface);

        let response = SidebarResponse { switch_to_workspace: Some(ws2), switch_tab: None };

        apply_sidebar_response(&response, &mut state, &mut focus).expect("should succeed");

        // Focus should now be on ws2's first surface
        let ws2_surface = state.workspaces[1].layout.surface_ids()[0];
        assert_eq!(
            focus.focused_surface(),
            Some(ws2_surface),
            "focus should move to new workspace's first surface"
        );
    }

    #[test]
    fn switch_to_nonexistent_workspace_returns_error() {
        let (mut state, _ws1, _ws2) = state_with_two_workspaces();
        let mut focus = FocusManager::new();

        let stale_id = WorkspaceId::new(999);
        let response = SidebarResponse { switch_to_workspace: Some(stale_id), switch_tab: None };

        let result = apply_sidebar_response(&response, &mut state, &mut focus);
        assert!(result.is_err(), "switching to nonexistent workspace should return an error");
    }

    #[test]
    fn switch_to_nonexistent_workspace_preserves_current_active() {
        let (mut state, ws1, _ws2) = state_with_two_workspaces();
        let mut focus = FocusManager::new();

        let stale_id = WorkspaceId::new(999);
        let response = SidebarResponse { switch_to_workspace: Some(stale_id), switch_tab: None };

        let _ = apply_sidebar_response(&response, &mut state, &mut focus);
        assert_eq!(
            state.active_workspace_id,
            Some(ws1),
            "active workspace should not change on error"
        );
    }

    // ================================================================
    // Unit 4: Switch tab via SidebarResponse
    // ================================================================

    #[test]
    fn switch_tab_to_conversations_updates_sidebar_tab() {
        let mut state = AppState::new();
        let mut focus = FocusManager::new();
        assert_eq!(state.sidebar.active_tab, SidebarTab::Workspaces);

        let response = SidebarResponse {
            switch_to_workspace: None,
            switch_tab: Some(SidebarTab::Conversations),
        };

        apply_sidebar_response(&response, &mut state, &mut focus).expect("should succeed");
        assert_eq!(
            state.sidebar.active_tab,
            SidebarTab::Conversations,
            "sidebar tab should change to Conversations"
        );
    }

    #[test]
    fn switch_tab_to_workspaces_updates_sidebar_tab() {
        let mut state = AppState::new();
        state.set_sidebar_tab(SidebarTab::Conversations);
        let mut focus = FocusManager::new();

        let response =
            SidebarResponse { switch_to_workspace: None, switch_tab: Some(SidebarTab::Workspaces) };

        apply_sidebar_response(&response, &mut state, &mut focus).expect("should succeed");
        assert_eq!(
            state.sidebar.active_tab,
            SidebarTab::Workspaces,
            "sidebar tab should change back to Workspaces"
        );
    }

    #[test]
    fn switch_tab_when_sidebar_hidden_still_updates_tab() {
        let mut state = AppState::new();
        state.toggle_sidebar(); // hide sidebar
        assert!(!state.sidebar.visible);
        let mut focus = FocusManager::new();

        let response = SidebarResponse {
            switch_to_workspace: None,
            switch_tab: Some(SidebarTab::Conversations),
        };

        apply_sidebar_response(&response, &mut state, &mut focus).expect("should succeed");
        assert_eq!(
            state.sidebar.active_tab,
            SidebarTab::Conversations,
            "tab should change even when sidebar is hidden"
        );
        assert!(!state.sidebar.visible, "sidebar should remain hidden");
    }

    // ================================================================
    // Unit 4: No-op response
    // ================================================================

    #[test]
    fn empty_response_changes_nothing() {
        let (mut state, ws1, _ws2) = state_with_two_workspaces();
        let mut focus = FocusManager::new();
        let ws1_surface = state.workspaces[0].layout.surface_ids()[0];
        focus.focus_surface(ws1_surface);

        let response = SidebarResponse::default();

        apply_sidebar_response(&response, &mut state, &mut focus).expect("should succeed");
        assert_eq!(state.active_workspace_id, Some(ws1));
        assert_eq!(state.sidebar.active_tab, SidebarTab::Workspaces);
        assert_eq!(focus.focused_surface(), Some(ws1_surface));
    }

    // ================================================================
    // Unit 4: Toggle sidebar
    // ================================================================

    #[test]
    fn toggle_sidebar_from_visible_to_hidden() {
        let mut state = AppState::new();
        assert!(state.sidebar.visible);
        state.toggle_sidebar();
        assert!(!state.sidebar.visible, "sidebar should be hidden after toggle");
    }

    #[test]
    fn toggle_sidebar_from_hidden_to_visible() {
        let mut state = AppState::new();
        state.toggle_sidebar(); // hide
        assert!(!state.sidebar.visible);
        state.toggle_sidebar(); // show again
        assert!(state.sidebar.visible, "sidebar should be visible after second toggle");
    }

    // ================================================================
    // Unit 4: Both tab switch and workspace switch in same response
    // ================================================================

    #[test]
    fn response_with_both_tab_and_workspace_switch() {
        let (mut state, _ws1, ws2) = state_with_two_workspaces();
        let mut focus = FocusManager::new();

        let response = SidebarResponse {
            switch_to_workspace: Some(ws2),
            switch_tab: Some(SidebarTab::Conversations),
        };

        apply_sidebar_response(&response, &mut state, &mut focus).expect("should succeed");
        assert_eq!(state.active_workspace_id, Some(ws2), "workspace should switch");
        assert_eq!(state.sidebar.active_tab, SidebarTab::Conversations, "tab should also switch");
    }

    // ================================================================
    // Unit 4: Switch workspace to the already-active one
    // ================================================================

    #[test]
    fn switch_to_already_active_workspace_is_harmless() {
        let (mut state, ws1, _ws2) = state_with_two_workspaces();
        let mut focus = FocusManager::new();
        let ws1_surface = state.workspaces[0].layout.surface_ids()[0];
        focus.focus_surface(ws1_surface);

        let response = SidebarResponse {
            switch_to_workspace: Some(ws1), // already active
            switch_tab: None,
        };

        apply_sidebar_response(&response, &mut state, &mut focus).expect("should succeed");
        assert_eq!(state.active_workspace_id, Some(ws1));
    }
}
