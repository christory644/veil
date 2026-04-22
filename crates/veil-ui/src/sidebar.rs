//! Sidebar container layout, tab header, and `SidebarResponse`.
//!
//! This module contains the top-level sidebar rendering function that
//! draws the sidebar panel with tab headers and delegates to the active
//! tab's content renderer.

use veil_core::session::SessionId;
use veil_core::state::{AppState, SidebarTab};
use veil_core::workspace::WorkspaceId;

/// Response from the sidebar UI, describing user interactions.
#[derive(Debug, Default)]
pub struct SidebarResponse {
    /// User clicked a workspace to switch to it.
    pub switch_to_workspace: Option<WorkspaceId>,
    /// User clicked a tab to switch to it.
    pub switch_tab: Option<SidebarTab>,
    /// User clicked a conversation entry.
    pub selected_conversation: Option<SessionId>,
}

/// Render the sidebar container into the given top-level UI.
///
/// Returns a `SidebarResponse` describing any user interactions.
/// The caller (the `veil` binary) interprets the response and
/// mutates `AppState` accordingly.
pub fn render_sidebar(ui: &mut egui::Ui, state: &AppState) -> SidebarResponse {
    let mut response = SidebarResponse::default();

    #[allow(clippy::cast_precision_loss)]
    let width = state.sidebar.width_px as f32;

    egui::Panel::left("veil_sidebar").exact_size(width).show_inside(ui, |ui| {
        // Tab header bar
        ui.horizontal(|ui| {
            if ui.button("Workspaces").clicked()
                && state.sidebar.active_tab != SidebarTab::Workspaces
            {
                response.switch_tab = Some(SidebarTab::Workspaces);
            }
            if ui.button("Conversations").clicked()
                && state.sidebar.active_tab != SidebarTab::Conversations
            {
                response.switch_tab = Some(SidebarTab::Conversations);
            }
        });

        ui.separator();

        // Tab content
        egui::ScrollArea::vertical().show(ui, |ui| match state.sidebar.active_tab {
            SidebarTab::Workspaces => {
                let entries = crate::workspace_list::extract_workspace_entries(state);
                if let Some(ws_id) = crate::workspace_list::render_workspaces_tab(ui, &entries) {
                    response.switch_to_workspace = Some(ws_id);
                }
            }
            SidebarTab::Conversations => {
                ui.label("Coming soon");
            }
        });
    });

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use veil_core::session::{AgentKind, SessionEntry, SessionId, SessionStatus};
    use veil_core::state::{AppState, SidebarTab};

    // ================================================================
    // Helper: run an egui frame headlessly and call `render_sidebar`
    // ================================================================

    fn run_sidebar_frame(state: &AppState) -> SidebarResponse {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput::default();
        let mut response = SidebarResponse::default();
        let _ = ctx.run_ui(raw_input, |ui| {
            response = render_sidebar(ui, state);
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
        let _ = ctx.run_ui(raw_input, |ui| {
            let _response = render_sidebar(ui, &state);
            // After render_sidebar, check if a left side panel was actually registered.
            // A properly implemented render_sidebar creates an egui::Panel::left
            // which consumes horizontal space. We verify by checking that the
            // available_width (remaining space after panels) is narrower than full window.
            let remaining_width = ui.available_width();
            // If sidebar rendered correctly, remaining width < 1280 (panel consumed space).
            panel_allocated = remaining_width < 1279.0;
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
    // Unit 4: Sidebar Integration — helpers
    // ================================================================

    fn make_session(id: &str, title: &str, agent: AgentKind) -> SessionEntry {
        use chrono::Utc;
        use std::path::PathBuf;
        SessionEntry {
            id: SessionId::new(id),
            agent,
            title: title.to_string(),
            working_dir: PathBuf::from("/tmp/project"),
            branch: Some("main".to_string()),
            pr_number: None,
            pr_url: None,
            plan_content: None,
            status: SessionStatus::Active,
            started_at: Utc::now(),
            ended_at: None,
            indexed_at: Utc::now(),
        }
    }

    // ================================================================
    // Unit 4: Sidebar Integration — happy path
    // ================================================================

    #[test]
    fn sidebar_conversations_tab_with_sessions_renders_without_panic() {
        let mut state = AppState::new();
        state.set_sidebar_tab(SidebarTab::Conversations);
        state.update_conversations(vec![
            make_session("s1", "Fix auth", AgentKind::ClaudeCode),
            make_session("s2", "Add tests", AgentKind::Codex),
        ]);
        let _response = run_sidebar_frame(&state);
    }

    #[test]
    fn sidebar_conversations_tab_empty_sessions_renders_without_panic() {
        let mut state = AppState::new();
        state.set_sidebar_tab(SidebarTab::Conversations);
        // No sessions
        let _response = run_sidebar_frame(&state);
    }

    #[test]
    fn sidebar_response_default_has_no_selected_conversation() {
        let response = SidebarResponse::default();
        assert!(
            response.selected_conversation.is_none(),
            "default SidebarResponse should have no selected_conversation"
        );
    }

    #[test]
    fn sidebar_response_all_fields_none_when_no_interaction() {
        let mut state = AppState::new();
        state.set_sidebar_tab(SidebarTab::Conversations);
        state.update_conversations(vec![make_session("s1", "Fix auth", AgentKind::ClaudeCode)]);
        let response = run_sidebar_frame(&state);

        assert!(
            response.switch_to_workspace.is_none(),
            "no interaction should leave switch_to_workspace as None"
        );
        assert!(response.switch_tab.is_none(), "no interaction should leave switch_tab as None");
        assert!(
            response.selected_conversation.is_none(),
            "no interaction should leave selected_conversation as None"
        );
    }

    #[test]
    fn sidebar_response_selected_conversation_field_is_accessible() {
        // Verify the field exists and can be set/read.
        let mut response = SidebarResponse::default();
        response.selected_conversation = Some(SessionId::new("test-session"));
        assert_eq!(
            response.selected_conversation,
            Some(SessionId::new("test-session")),
            "selected_conversation field should be settable and readable"
        );
    }

    #[test]
    fn sidebar_conversations_tab_width_zero_no_panic() {
        let mut state = AppState::new();
        state.set_sidebar_tab(SidebarTab::Conversations);
        state.sidebar.width_px = 0;
        state.update_conversations(vec![make_session("s1", "Fix auth", AgentKind::ClaudeCode)]);
        let _response = run_sidebar_frame(&state);
    }

    #[test]
    fn sidebar_switches_between_tabs_rendering_both() {
        let mut state = AppState::new();
        state.update_conversations(vec![make_session("s1", "Fix auth", AgentKind::ClaudeCode)]);

        // Render with Workspaces tab (default)
        let _response = run_sidebar_frame(&state);

        // Switch to Conversations tab
        state.set_sidebar_tab(SidebarTab::Conversations);
        let _response = run_sidebar_frame(&state);

        // Switch back to Workspaces
        state.set_sidebar_tab(SidebarTab::Workspaces);
        let _response = run_sidebar_frame(&state);
    }
}
