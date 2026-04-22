//! Workspace list rendering for the Workspaces tab.
//!
//! Renders each workspace entry as a clickable row with active indicator,
//! name, git branch, abbreviated working directory, and notification badge.

use std::path::Path;

use veil_core::state::AppState;
use veil_core::workspace::WorkspaceId;

/// Data needed to render a single workspace entry.
/// This is a view-model extracted from `AppState` to keep rendering
/// decoupled from the full state.
pub struct WorkspaceEntryData<'a> {
    /// Workspace identifier.
    pub id: WorkspaceId,
    /// Display name.
    pub name: &'a str,
    /// Working directory.
    pub working_directory: &'a Path,
    /// Git branch, if any.
    pub branch: Option<&'a str>,
    /// Whether this workspace is the currently active one.
    pub is_active: bool,
    /// Number of unacknowledged notifications for this workspace.
    pub notification_count: usize,
}

/// Render the workspace list inside a `ScrollArea`.
///
/// Returns the ID of a workspace the user clicked to switch to, if any.
pub fn render_workspaces_tab(
    ui: &mut egui::Ui,
    entries: &[WorkspaceEntryData],
) -> Option<WorkspaceId> {
    let mut clicked_id = None;
    for entry in entries {
        if render_workspace_entry(ui, entry) {
            clicked_id = Some(entry.id);
        }
    }
    clicked_id
}

/// Render a single workspace entry row.
///
/// Returns `true` if the user clicked this entry.
fn render_workspace_entry(ui: &mut egui::Ui, entry: &WorkspaceEntryData) -> bool {
    let response = ui.vertical(|ui| {
        // First line: active indicator + name (+ optional notification badge)
        ui.horizontal(|ui| {
            let indicator = if entry.is_active { "● " } else { "○ " };
            ui.label(indicator);

            let name_text = egui::RichText::new(entry.name);
            let name_text = if entry.is_active { name_text.strong() } else { name_text };
            ui.label(name_text);

            if entry.notification_count > 0 {
                ui.label(format!("({})", entry.notification_count));
            }
        });

        // Branch line (only if present)
        if let Some(branch) = entry.branch {
            ui.label(egui::RichText::new(format!("  {branch}")).weak());
        }

        // Working directory line
        let abbreviated = abbreviate_path(entry.working_directory);
        ui.label(egui::RichText::new(format!("  {abbreviated}")).weak());
    });

    response.response.interact(egui::Sense::click()).clicked()
}

/// Extract workspace entry view data from `AppState`.
///
/// Counts unacknowledged notifications per workspace from `state.notifications`.
pub fn extract_workspace_entries(state: &AppState) -> Vec<WorkspaceEntryData<'_>> {
    let active_id = state.active_workspace_id.unwrap_or(WorkspaceId::new(0));

    state
        .workspaces
        .iter()
        .map(|ws| {
            let notification_count = state
                .notifications
                .iter()
                .filter(|n| n.workspace_id == ws.id && !n.acknowledged)
                .count();

            WorkspaceEntryData {
                id: ws.id,
                name: &ws.name,
                working_directory: &ws.working_directory,
                branch: ws.branch.as_deref(),
                is_active: ws.id == active_id,
                notification_count,
            }
        })
        .collect()
}

/// Abbreviate a path by replacing the home directory prefix with "~".
pub fn abbreviate_path(path: &Path) -> String {
    let path_str = path.as_os_str();
    if path_str.is_empty() {
        return String::new();
    }

    if let Some(home) = dirs::home_dir() {
        if path == home {
            return "~".to_string();
        }
        if let Ok(suffix) = path.strip_prefix(&home) {
            return format!("~/{}", suffix.display());
        }
    }

    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use veil_core::workspace::WorkspaceId;

    // ================================================================
    // Helper: create workspace entry test data
    // ================================================================

    /// Owned data that backs `WorkspaceEntryData` borrows.
    struct OwnedEntry {
        id: WorkspaceId,
        name: String,
        dir: PathBuf,
        branch: Option<String>,
        is_active: bool,
        notification_count: usize,
    }

    impl OwnedEntry {
        fn new(
            id: u64,
            name: &str,
            dir: &str,
            branch: Option<&str>,
            is_active: bool,
            notification_count: usize,
        ) -> Self {
            Self {
                id: WorkspaceId::new(id),
                name: name.to_string(),
                dir: PathBuf::from(dir),
                branch: branch.map(String::from),
                is_active,
                notification_count,
            }
        }

        fn as_entry_data(&self) -> WorkspaceEntryData<'_> {
            WorkspaceEntryData {
                id: self.id,
                name: &self.name,
                working_directory: &self.dir,
                branch: self.branch.as_deref(),
                is_active: self.is_active,
                notification_count: self.notification_count,
            }
        }
    }

    // ================================================================
    // Helper: run an egui frame with workspace list rendering
    // ================================================================

    fn screen_input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(1280.0, 800.0),
            )),
            ..Default::default()
        }
    }

    fn run_workspaces_tab_frame(entries: &[WorkspaceEntryData]) -> Option<WorkspaceId> {
        let ctx = egui::Context::default();
        let mut result = None;
        let _ = ctx.run_ui(screen_input(), |ctx| {
            egui::CentralPanel::default().show_inside(ctx, |ui| {
                result = render_workspaces_tab(ui, entries);
            });
        });
        result
    }

    /// Run a frame and return the cursor advancement height from inside the UI closure.
    fn measure_render_height(entries: &[WorkspaceEntryData]) -> f32 {
        let ctx = egui::Context::default();
        let mut height = 0.0;
        let _ = ctx.run_ui(screen_input(), |ctx| {
            egui::CentralPanel::default().show_inside(ctx, |ui| {
                let before_cursor = ui.cursor().top();
                render_workspaces_tab(ui, entries);
                let after_cursor = ui.cursor().top();
                height = after_cursor - before_cursor;
            });
        });
        height
    }

    // ================================================================
    // Unit 3: Path abbreviation
    // ================================================================

    #[test]
    fn abbreviate_path_replaces_home_dir() {
        let home = dirs::home_dir().expect("should have a home directory");
        let full_path = home.join("repos/api");
        let abbreviated = abbreviate_path(&full_path);
        assert_eq!(abbreviated, "~/repos/api", "home dir prefix should be replaced with ~");
    }

    #[test]
    fn abbreviate_path_non_home_path_unchanged() {
        let path = Path::new("/tmp/test");
        let abbreviated = abbreviate_path(path);
        assert_eq!(abbreviated, "/tmp/test", "non-home paths should remain unchanged");
    }

    #[test]
    fn abbreviate_path_home_dir_itself() {
        let home = dirs::home_dir().expect("should have a home directory");
        let abbreviated = abbreviate_path(&home);
        assert_eq!(abbreviated, "~", "home dir itself should become just ~");
    }

    #[test]
    fn abbreviate_path_empty_path() {
        let path = Path::new("");
        let abbreviated = abbreviate_path(path);
        assert_eq!(abbreviated, "", "empty path should remain empty");
    }

    #[test]
    fn abbreviate_path_root() {
        let path = Path::new("/");
        let abbreviated = abbreviate_path(path);
        assert_eq!(abbreviated, "/", "root should remain /");
    }

    // ================================================================
    // Unit 3: Workspace list rendering -- happy path
    // ================================================================

    #[test]
    fn render_three_workspaces_returns_none_without_click() {
        let data = [
            OwnedEntry::new(1, "api-server", "/tmp/api", Some("main"), true, 0),
            OwnedEntry::new(2, "frontend", "/tmp/fe", Some("develop"), false, 0),
            OwnedEntry::new(3, "docs", "/tmp/docs", None, false, 0),
        ];
        let entries: Vec<_> = data.iter().map(OwnedEntry::as_entry_data).collect();
        let result = run_workspaces_tab_frame(&entries);
        assert!(result.is_none(), "no click should mean no workspace switch");
    }

    #[test]
    fn render_empty_workspace_list_returns_none() {
        let entries: Vec<WorkspaceEntryData> = Vec::new();
        let result = run_workspaces_tab_frame(&entries);
        assert!(result.is_none(), "empty list should return None");
    }

    #[test]
    fn render_single_workspace_does_not_panic() {
        let data = OwnedEntry::new(1, "only-ws", "/tmp/only", None, true, 0);
        let entries = vec![data.as_entry_data()];
        let _result = run_workspaces_tab_frame(&entries);
    }

    // ================================================================
    // Unit 3: Entries produce visible content
    // ================================================================

    #[test]
    fn render_workspace_entries_produces_content() {
        // Workspace entries should consume vertical space when rendered.
        let data = [
            OwnedEntry::new(1, "api-server", "/tmp/api", Some("main"), true, 0),
            OwnedEntry::new(2, "frontend", "/tmp/fe", None, false, 0),
        ];
        let entries: Vec<_> = data.iter().map(OwnedEntry::as_entry_data).collect();
        let height = measure_render_height(&entries);
        assert!(
            height > 10.0,
            "rendering 2 workspace entries should consume vertical space, got height={height}"
        );
    }

    #[test]
    fn render_three_entries_taller_than_one() {
        let one = [OwnedEntry::new(1, "ws1", "/tmp", None, true, 0)];
        let three = [
            OwnedEntry::new(1, "ws1", "/tmp", None, true, 0),
            OwnedEntry::new(2, "ws2", "/tmp", None, false, 0),
            OwnedEntry::new(3, "ws3", "/tmp", None, false, 0),
        ];

        let entries_one: Vec<_> = one.iter().map(OwnedEntry::as_entry_data).collect();
        let entries_three: Vec<_> = three.iter().map(OwnedEntry::as_entry_data).collect();

        let height_one = measure_render_height(&entries_one);
        let height_three = measure_render_height(&entries_three);

        assert!(
            height_three > height_one,
            "3 entries ({height_three}px) should be taller than 1 entry ({height_one}px)"
        );
    }

    // ================================================================
    // Unit 3: Branch rendering affects height
    // ================================================================

    #[test]
    fn workspace_with_branch_taller_than_without() {
        let with_branch = [OwnedEntry::new(1, "ws", "/tmp", Some("feature/login"), true, 0)];
        let without_branch = [OwnedEntry::new(2, "ws", "/tmp", None, true, 0)];

        let entries_with: Vec<_> = with_branch.iter().map(OwnedEntry::as_entry_data).collect();
        let entries_without: Vec<_> =
            without_branch.iter().map(OwnedEntry::as_entry_data).collect();

        let height_with = measure_render_height(&entries_with);
        let height_without = measure_render_height(&entries_without);

        // Entry with branch should have one more line, making it taller.
        assert!(
            height_with > height_without,
            "entry with branch ({height_with}px) should be taller than without ({height_without}px)"
        );
    }

    // ================================================================
    // Unit 3: Notification badges affect width/content
    // ================================================================

    #[test]
    fn workspace_with_notifications_produces_content() {
        let data = [OwnedEntry::new(1, "ws", "/tmp", None, false, 3)];
        let entries: Vec<_> = data.iter().map(OwnedEntry::as_entry_data).collect();

        let height = measure_render_height(&entries);
        assert!(
            height > 10.0,
            "workspace with 3 notifications should render content, got height={height}"
        );
    }

    // ================================================================
    // Unit 3: Edge cases -- long name, empty path
    // ================================================================

    #[test]
    fn very_long_workspace_name_does_not_panic() {
        let long_name = "a".repeat(500);
        let data = OwnedEntry {
            id: WorkspaceId::new(1),
            name: long_name,
            dir: PathBuf::from("/tmp"),
            branch: None,
            is_active: true,
            notification_count: 0,
        };
        let entries = vec![data.as_entry_data()];
        // Should not panic
        let _result = run_workspaces_tab_frame(&entries);
    }

    #[test]
    fn workspace_with_empty_path_does_not_panic() {
        let data = OwnedEntry::new(1, "ws", "", None, true, 0);
        let entries = vec![data.as_entry_data()];
        // Should not panic
        let _result = run_workspaces_tab_frame(&entries);
    }

    // ================================================================
    // extract_workspace_entries
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
