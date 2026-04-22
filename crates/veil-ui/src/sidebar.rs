//! Sidebar container layout, tab header, and `SidebarResponse`.
//!
//! This module contains the top-level sidebar rendering function that
//! draws the sidebar panel with tab headers and delegates to the active
//! tab's content renderer.

use veil_core::state::{AppState, SidebarTab};
use veil_core::workspace::WorkspaceId;

use crate::workspace_list::WorkspaceEntryData;

/// Response from the sidebar UI, describing user interactions.
#[derive(Debug, Default)]
pub struct SidebarResponse {
    /// User clicked a workspace to switch to it.
    pub switch_to_workspace: Option<WorkspaceId>,
    /// User clicked a tab to switch to it.
    pub switch_tab: Option<SidebarTab>,
}

/// Render the sidebar container into the egui context.
///
/// Returns a `SidebarResponse` describing any user interactions.
/// The caller (the `veil` binary) interprets the response and
/// mutates `AppState` accordingly.
pub fn render_sidebar(_ctx: &egui::Context, _state: &AppState) -> SidebarResponse {
    // Stub: returns default (no interactions) so tests fail.
    SidebarResponse::default()
}

/// Extract workspace entry view data from `AppState`.
///
/// Counts unacknowledged notifications per workspace from `state.notifications`.
pub fn extract_workspace_entries(state: &AppState) -> Vec<WorkspaceEntryData<'_>> {
    // Stub: returns empty vec so tests fail.
    let _ = state;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use veil_core::state::AppState;

    // ================================================================
    // Helper: run an egui frame headlessly and call `render_sidebar`
    // ================================================================

    fn run_sidebar_frame(state: &AppState) -> SidebarResponse {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput::default();
        let mut response = SidebarResponse::default();
        let _ = ctx.run_ui(raw_input, |ctx| {
            response = render_sidebar(ctx, state);
        });
        response
    }

    // ================================================================
    // Unit 2: Sidebar Container -- happy path
    // ================================================================

    #[test]
    fn sidebar_renders_side_panel_when_visible() {
        let state = AppState::new(); // sidebar visible by default, width_px = 250
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(1280.0, 800.0),
            )),
            ..Default::default()
        };
        let mut panel_allocated = false;
        let _ = ctx.run_ui(raw_input, |ctx| {
            let _response = render_sidebar(ctx, &state);
            // After render_sidebar, check if a left side panel was actually registered.
            // A properly implemented render_sidebar creates an egui::SidePanel::left
            // which consumes horizontal space. We verify by checking that the
            // content_rect (remaining space after panels) is narrower than full window.
            let content = ctx.content_rect();
            // If sidebar rendered correctly, content width < 1280 (panel consumed space).
            panel_allocated = content.width() < 1279.0;
        });
        assert!(
            panel_allocated,
            "render_sidebar should allocate a side panel that consumes horizontal space"
        );
    }

    #[test]
    fn sidebar_with_workspaces_tab_active_returns_no_tab_switch() {
        let state = AppState::new(); // default tab is Workspaces
        let response = run_sidebar_frame(&state);
        // When we just render without clicking, no tab switch should occur.
        assert!(
            response.switch_tab.is_none(),
            "rendering without interaction should produce no tab switch"
        );
    }

    #[test]
    fn sidebar_with_workspaces_tab_active_returns_no_workspace_switch() {
        let state = AppState::new();
        let response = run_sidebar_frame(&state);
        assert!(
            response.switch_to_workspace.is_none(),
            "rendering without interaction should produce no workspace switch"
        );
    }

    // ================================================================
    // Unit 2: Sidebar Container -- empty workspace list
    // ================================================================

    #[test]
    fn sidebar_empty_workspace_list_renders_without_panic() {
        let state = AppState::new(); // no workspaces
                                     // This should not panic
        let _response = run_sidebar_frame(&state);
    }

    // ================================================================
    // Unit 2: Sidebar Container -- sidebar width 0
    // ================================================================

    #[test]
    fn sidebar_width_zero_renders_without_panic() {
        let mut state = AppState::new();
        state.sidebar.width_px = 0;
        // Should not panic even with 0 width
        let _response = run_sidebar_frame(&state);
    }

    // ================================================================
    // Unit 2: extract_workspace_entries
    // ================================================================

    #[test]
    fn extract_entries_returns_one_entry_per_workspace() {
        let mut state = AppState::new();
        state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/ws1"));
        state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/ws2"));
        state.create_workspace("ws3".to_string(), PathBuf::from("/tmp/ws3"));

        let entries = extract_workspace_entries(&state);
        assert_eq!(entries.len(), 3, "should have one entry per workspace");
    }

    #[test]
    fn extract_entries_marks_active_workspace() {
        let mut state = AppState::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/ws1"));
        let _ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/ws2"));

        let entries = extract_workspace_entries(&state);
        let active_entry = entries.iter().find(|e| e.id == ws1);
        assert!(active_entry.is_some(), "active workspace should be in entries");
        assert!(active_entry.unwrap().is_active, "first workspace should be marked active");
    }

    #[test]
    fn extract_entries_marks_inactive_workspaces() {
        let mut state = AppState::new();
        let _ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/ws1"));
        let ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/ws2"));

        let entries = extract_workspace_entries(&state);
        let inactive_entry = entries.iter().find(|e| e.id == ws2);
        assert!(inactive_entry.is_some(), "inactive workspace should be in entries");
        assert!(!inactive_entry.unwrap().is_active, "second workspace should NOT be marked active");
    }

    #[test]
    fn extract_entries_includes_notification_count() {
        let mut state = AppState::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/ws1"));

        state.add_notification(ws1, "notif 1".to_string());
        state.add_notification(ws1, "notif 2".to_string());
        // Acknowledge one
        let first_notif_id = state.notifications[0].id;
        state.acknowledge_notification(first_notif_id);

        let entries = extract_workspace_entries(&state);
        let entry = entries.iter().find(|e| e.id == ws1).expect("ws1 should be in entries");
        assert_eq!(entry.notification_count, 1, "should count only unacknowledged notifications");
    }

    #[test]
    fn extract_entries_empty_state_returns_empty() {
        let state = AppState::new();
        let entries = extract_workspace_entries(&state);
        assert!(entries.is_empty());
    }

    #[test]
    fn extract_entries_preserves_workspace_name() {
        let mut state = AppState::new();
        state.create_workspace("my-project".to_string(), PathBuf::from("/tmp"));

        let entries = extract_workspace_entries(&state);
        assert_eq!(entries.len(), 1, "should have one entry");
        assert_eq!(entries[0].name, "my-project");
    }

    #[test]
    fn extract_entries_preserves_branch() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        // Set branch on workspace
        state.workspaces[0].branch = Some("main".to_string());

        let entries = extract_workspace_entries(&state);
        assert_eq!(entries.len(), 1, "should have one entry");
        let entry = entries.iter().find(|e| e.id == ws_id).unwrap();
        assert_eq!(entry.branch, Some("main"));
    }

    #[test]
    fn extract_entries_no_branch_is_none() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));

        let entries = extract_workspace_entries(&state);
        assert_eq!(entries.len(), 1, "should have one entry");
        let entry = entries.iter().find(|e| e.id == ws_id).unwrap();
        assert_eq!(entry.branch, None);
    }
}
