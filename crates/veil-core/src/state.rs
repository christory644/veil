//! Central application state — single source of truth for UI rendering.
//!
//! The UI thread reads `AppState` each frame to render. Background actors push
//! `StateUpdate` messages to `AppState` via tokio mpsc channels.

use chrono::{DateTime, Utc};
use std::path::PathBuf;

use crate::session::SessionEntry;
use crate::workspace::{PaneId, SplitDirection, SurfaceId, Workspace, WorkspaceError, WorkspaceId};

/// Which tab is active in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTab {
    /// Show workspace list.
    #[default]
    Workspaces,
    /// Show conversation history.
    Conversations,
}

/// Sidebar display state.
#[derive(Debug, Clone)]
pub struct SidebarState {
    /// Whether the sidebar is visible.
    pub visible: bool,
    /// Currently active tab.
    pub active_tab: SidebarTab,
    /// Width in pixels.
    pub width_px: u32,
}

/// A notification displayed in the UI.
#[derive(Debug, Clone)]
pub struct NotificationEntry {
    /// Unique notification identifier.
    pub id: u64,
    /// Which workspace this notification belongs to.
    pub workspace_id: WorkspaceId,
    /// Notification message.
    pub message: String,
    /// When the notification was created.
    pub timestamp: DateTime<Utc>,
    /// Whether the user has acknowledged this notification.
    pub acknowledged: bool,
}

/// Indexed conversation data for the Conversations tab.
#[derive(Debug, Clone, Default)]
pub struct ConversationIndex {
    /// Session entries ready for UI rendering.
    pub sessions: Vec<SessionEntry>,
}

/// Errors that can occur during state operations.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// The specified workspace was not found.
    #[error("workspace {0:?} not found")]
    WorkspaceNotFound(WorkspaceId),
    /// No workspace is currently active.
    #[error("no active workspace")]
    NoActiveWorkspace,
    /// Workspace-level error.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
}

/// Central application state — single source of truth for UI rendering.
///
/// Focus management lives in the separate `FocusManager` (focus module),
/// not in `AppState`. The event loop owns both and coordinates between them.
#[derive(Debug)]
pub struct AppState {
    /// All workspaces.
    pub workspaces: Vec<Workspace>,
    /// Currently active workspace.
    pub active_workspace_id: Option<WorkspaceId>,
    /// Conversation index for the sidebar.
    pub conversations: ConversationIndex,
    /// Notification list.
    pub notifications: Vec<NotificationEntry>,
    /// Sidebar display state.
    pub sidebar: SidebarState,
    next_id: u64,
}

impl AppState {
    /// Create a new `AppState` with defaults: no workspaces, sidebar visible, Workspaces tab.
    pub fn new() -> Self {
        Self {
            workspaces: Vec::new(),
            active_workspace_id: None,
            conversations: ConversationIndex::default(),
            notifications: Vec::new(),
            sidebar: SidebarState {
                visible: true,
                active_tab: SidebarTab::Workspaces,
                width_px: 250,
            },
            next_id: 1,
        }
    }

    /// Generate the next unique ID.
    pub fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Create a new workspace. Sets it active if no workspace exists yet.
    pub fn create_workspace(&mut self, name: String, working_directory: PathBuf) -> WorkspaceId {
        let ws_id = WorkspaceId::new(self.next_id());
        let pane_id = PaneId::new(self.next_id());
        let surface_id = SurfaceId::new(self.next_id());
        let ws = Workspace::new(ws_id, name, working_directory, pane_id, surface_id);
        let is_first = self.workspaces.is_empty();
        self.workspaces.push(ws);
        if is_first {
            self.active_workspace_id = Some(ws_id);
        }
        ws_id
    }

    /// Close a workspace. Returns surface IDs to clean up.
    /// Activates adjacent workspace if closing the active one.
    pub fn close_workspace(&mut self, id: WorkspaceId) -> Result<Vec<SurfaceId>, StateError> {
        let idx = self.workspace_index(id)?;

        let surface_ids = self.workspaces[idx].layout.surface_ids();
        self.workspaces.remove(idx);

        if self.active_workspace_id == Some(id) {
            if self.workspaces.is_empty() {
                self.active_workspace_id = None;
            } else {
                // Activate the workspace at the same index, or the last one if we removed the tail.
                let new_idx =
                    if idx < self.workspaces.len() { idx } else { self.workspaces.len() - 1 };
                self.active_workspace_id = Some(self.workspaces[new_idx].id);
            }
        }

        Ok(surface_ids)
    }

    /// Get the active workspace.
    pub fn active_workspace(&self) -> Option<&Workspace> {
        let id = self.active_workspace_id?;
        self.workspaces.iter().find(|ws| ws.id == id)
    }

    /// Get the active workspace mutably.
    pub fn active_workspace_mut(&mut self) -> Option<&mut Workspace> {
        let id = self.active_workspace_id?;
        self.workspaces.iter_mut().find(|ws| ws.id == id)
    }

    /// Look up a workspace by ID.
    pub fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|ws| ws.id == id)
    }

    /// Look up a workspace by ID, returning a mutable reference.
    fn workspace_mut(&mut self, id: WorkspaceId) -> Result<&mut Workspace, StateError> {
        self.workspaces.iter_mut().find(|ws| ws.id == id).ok_or(StateError::WorkspaceNotFound(id))
    }

    /// Find the index of a workspace by ID.
    fn workspace_index(&self, id: WorkspaceId) -> Result<usize, StateError> {
        self.workspaces.iter().position(|ws| ws.id == id).ok_or(StateError::WorkspaceNotFound(id))
    }

    /// Switch the active workspace.
    pub fn set_active_workspace(&mut self, id: WorkspaceId) -> Result<(), StateError> {
        // Verify the workspace exists via the immutable lookup.
        if self.workspace(id).is_none() {
            return Err(StateError::WorkspaceNotFound(id));
        }
        self.active_workspace_id = Some(id);
        Ok(())
    }

    /// Split a pane in a workspace. Returns the new pane and surface IDs.
    pub fn split_pane(
        &mut self,
        workspace_id: WorkspaceId,
        pane_id: PaneId,
        direction: SplitDirection,
    ) -> Result<(PaneId, SurfaceId), StateError> {
        let new_pane_id = PaneId::new(self.next_id());
        let new_surface_id = SurfaceId::new(self.next_id());
        self.workspace_mut(workspace_id)?.split_pane(
            pane_id,
            direction,
            new_pane_id,
            new_surface_id,
        )?;
        Ok((new_pane_id, new_surface_id))
    }

    /// Close a pane in a workspace. Returns the removed surface ID.
    pub fn close_pane(
        &mut self,
        workspace_id: WorkspaceId,
        pane_id: PaneId,
    ) -> Result<Option<SurfaceId>, StateError> {
        Ok(self.workspace_mut(workspace_id)?.close_pane(pane_id)?)
    }

    /// Toggle sidebar visibility.
    pub fn toggle_sidebar(&mut self) {
        self.sidebar.visible = !self.sidebar.visible;
    }

    /// Switch the active sidebar tab.
    pub fn set_sidebar_tab(&mut self, tab: SidebarTab) {
        self.sidebar.active_tab = tab;
    }

    /// Push a notification.
    pub fn add_notification(&mut self, workspace_id: WorkspaceId, message: String) {
        let id = self.next_id();
        self.notifications.push(NotificationEntry {
            id,
            workspace_id,
            message,
            timestamp: Utc::now(),
            acknowledged: false,
        });
    }

    /// Mark a notification as acknowledged.
    pub fn acknowledge_notification(&mut self, id: u64) {
        if let Some(notif) = self.notifications.iter_mut().find(|n| n.id == id) {
            notif.acknowledged = true;
        }
    }

    /// Replace the conversation index.
    pub fn update_conversations(&mut self, sessions: Vec<SessionEntry>) {
        self.conversations.sessions = sessions;
    }

    /// Toggle zoom on a pane in a workspace.
    pub fn toggle_zoom(
        &mut self,
        workspace_id: WorkspaceId,
        pane_id: PaneId,
    ) -> Result<Option<PaneId>, StateError> {
        Ok(self.workspace_mut(workspace_id)?.toggle_zoom(pane_id)?)
    }

    /// Rename a workspace.
    pub fn rename_workspace(
        &mut self,
        id: WorkspaceId,
        new_name: String,
    ) -> Result<(), StateError> {
        let ws = self.workspace_mut(id)?;
        ws.name = new_name;
        Ok(())
    }

    /// Move a workspace to a new position in the list.
    ///
    /// `new_index` is clamped to `0..=workspaces.len()-1`. The workspace is
    /// removed from its current position and inserted at `new_index`.
    pub fn reorder_workspace(
        &mut self,
        id: WorkspaceId,
        new_index: usize,
    ) -> Result<(), StateError> {
        let current_index = self.workspace_index(id)?;
        let clamped = new_index.min(self.workspaces.len() - 1);
        if current_index != clamped {
            let ws = self.workspaces.remove(current_index);
            self.workspaces.insert(clamped, ws);
        }
        Ok(())
    }

    /// Swap two workspaces by their IDs.
    pub fn swap_workspaces(&mut self, a: WorkspaceId, b: WorkspaceId) -> Result<(), StateError> {
        let idx_a = self.workspace_index(a)?;
        let idx_b = self.workspace_index(b)?;
        self.workspaces.swap(idx_a, idx_b);
        Ok(())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{AgentKind, SessionId, SessionStatus};
    use chrono::Utc;
    use std::path::PathBuf;

    fn make_session_entry(id: &str, title: &str) -> SessionEntry {
        SessionEntry {
            id: SessionId::new(id),
            agent: AgentKind::ClaudeCode,
            title: title.to_string(),
            working_dir: PathBuf::from("/tmp"),
            branch: None,
            pr_number: None,
            pr_url: None,
            plan_content: None,
            status: SessionStatus::Completed,
            started_at: Utc::now(),
            ended_at: None,
            indexed_at: Utc::now(),
        }
    }

    // --- new ---

    #[test]
    fn new_starts_with_empty_workspaces() {
        let state = AppState::new();
        assert!(state.workspaces.is_empty());
    }

    #[test]
    fn new_has_no_active_workspace() {
        let state = AppState::new();
        assert!(state.active_workspace_id.is_none());
    }

    #[test]
    fn new_sidebar_visible_with_workspaces_tab() {
        let state = AppState::new();
        assert!(state.sidebar.visible);
        assert_eq!(state.sidebar.active_tab, SidebarTab::Workspaces);
    }

    // --- create_workspace ---

    #[test]
    fn create_workspace_returns_valid_id() {
        let mut state = AppState::new();
        let id = state.create_workspace("test".to_string(), PathBuf::from("/tmp"));
        assert!(state.workspace(id).is_some());
    }

    #[test]
    fn first_workspace_becomes_active() {
        let mut state = AppState::new();
        let id = state.create_workspace("first".to_string(), PathBuf::from("/tmp"));
        assert_eq!(state.active_workspace_id, Some(id));
    }

    #[test]
    fn second_workspace_does_not_change_active() {
        let mut state = AppState::new();
        let first = state.create_workspace("first".to_string(), PathBuf::from("/tmp"));
        let _second = state.create_workspace("second".to_string(), PathBuf::from("/tmp"));
        assert_eq!(state.active_workspace_id, Some(first));
    }

    // --- close_workspace ---

    #[test]
    fn close_workspace_removes_and_returns_surface_ids() {
        let mut state = AppState::new();
        let id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let surfaces = state.close_workspace(id).expect("close should succeed");
        assert!(!surfaces.is_empty());
        assert!(state.workspace(id).is_none());
    }

    #[test]
    fn closing_active_workspace_activates_another() {
        let mut state = AppState::new();
        let first = state.create_workspace("first".to_string(), PathBuf::from("/tmp"));
        let second = state.create_workspace("second".to_string(), PathBuf::from("/tmp"));
        state.set_active_workspace(first).expect("set active should succeed");
        state.close_workspace(first).expect("close should succeed");
        assert_eq!(state.active_workspace_id, Some(second));
    }

    #[test]
    fn close_nonexistent_workspace_returns_error() {
        let mut state = AppState::new();
        let result = state.close_workspace(WorkspaceId::new(999));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StateError::WorkspaceNotFound(_)));
    }

    // --- set_active_workspace ---

    #[test]
    fn set_active_workspace_switches() {
        let mut state = AppState::new();
        let _first = state.create_workspace("first".to_string(), PathBuf::from("/tmp"));
        let second = state.create_workspace("second".to_string(), PathBuf::from("/tmp"));
        state.set_active_workspace(second).expect("set active should succeed");
        assert_eq!(state.active_workspace_id, Some(second));
    }

    #[test]
    fn set_active_workspace_invalid_id_returns_error() {
        let mut state = AppState::new();
        let result = state.set_active_workspace(WorkspaceId::new(999));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StateError::WorkspaceNotFound(_)));
    }

    // --- active_workspace ---

    #[test]
    fn active_workspace_returns_correct_one() {
        let mut state = AppState::new();
        let id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp/ws"));
        let ws = state.active_workspace().expect("should have active");
        assert_eq!(ws.id, id);
        assert_eq!(ws.name, "ws");
    }

    // --- split_pane ---

    #[test]
    fn split_pane_returns_new_ids() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let ws = state.workspace(ws_id).expect("workspace should exist");
        let first_pane = ws.pane_ids()[0];
        let (new_pane, new_surface) = state
            .split_pane(ws_id, first_pane, SplitDirection::Horizontal)
            .expect("split should succeed");
        let ws = state.workspace(ws_id).expect("workspace should exist");
        assert!(ws.pane_ids().contains(&new_pane));
        assert!(ws.layout.surface_ids().contains(&new_surface));
    }

    #[test]
    fn split_pane_nonexistent_workspace_returns_error() {
        let mut state = AppState::new();
        let result =
            state.split_pane(WorkspaceId::new(999), PaneId::new(1), SplitDirection::Horizontal);
        assert!(result.is_err());
    }

    #[test]
    fn split_pane_nonexistent_pane_returns_error() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let result = state.split_pane(ws_id, PaneId::new(999), SplitDirection::Horizontal);
        assert!(result.is_err());
    }

    // --- close_pane ---

    #[test]
    fn close_pane_returns_surface_id() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let ws = state.workspace(ws_id).expect("workspace should exist");
        let first_pane = ws.pane_ids()[0];
        let (_new_pane, _new_surface) = state
            .split_pane(ws_id, first_pane, SplitDirection::Horizontal)
            .expect("split should succeed");
        let ws = state.workspace(ws_id).expect("workspace should exist");
        let second_pane = ws.pane_ids()[1];
        let closed = state.close_pane(ws_id, second_pane).expect("close should succeed");
        assert!(closed.is_some());
    }

    #[test]
    fn close_last_pane_returns_workspace_error() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let ws = state.workspace(ws_id).expect("workspace should exist");
        let only_pane = ws.pane_ids()[0];
        let result = state.close_pane(ws_id, only_pane);
        assert!(result.is_err());
    }

    // --- toggle_sidebar ---

    #[test]
    fn toggle_sidebar_flips_visibility() {
        let mut state = AppState::new();
        assert!(state.sidebar.visible);
        state.toggle_sidebar();
        assert!(!state.sidebar.visible);
        state.toggle_sidebar();
        assert!(state.sidebar.visible);
    }

    // --- set_sidebar_tab ---

    #[test]
    fn set_sidebar_tab_changes_tab() {
        let mut state = AppState::new();
        assert_eq!(state.sidebar.active_tab, SidebarTab::Workspaces);
        state.set_sidebar_tab(SidebarTab::Conversations);
        assert_eq!(state.sidebar.active_tab, SidebarTab::Conversations);
    }

    // --- notifications ---

    #[test]
    fn add_notification_pushes_to_list() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        state.add_notification(ws_id, "hello".to_string());
        assert_eq!(state.notifications.len(), 1);
        assert_eq!(state.notifications[0].message, "hello");
        assert!(!state.notifications[0].acknowledged);
    }

    #[test]
    fn acknowledge_notification_marks_it() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        state.add_notification(ws_id, "hello".to_string());
        let notif_id = state.notifications[0].id;
        state.acknowledge_notification(notif_id);
        assert!(state.notifications[0].acknowledged);
    }

    // --- conversations ---

    #[test]
    fn update_conversations_replaces_session_list() {
        let mut state = AppState::new();
        assert!(state.conversations.sessions.is_empty());
        let sessions = vec![make_session_entry("s1", "Session 1")];
        state.update_conversations(sessions);
        assert_eq!(state.conversations.sessions.len(), 1);
        assert_eq!(state.conversations.sessions[0].title, "Session 1");
    }

    // --- next_id ---

    #[test]
    fn next_id_is_monotonically_increasing() {
        let mut state = AppState::new();
        let a = state.next_id();
        let b = state.next_id();
        let c = state.next_id();
        assert!(b > a);
        assert!(c > b);
    }

    // ============================================================
    // VEI-11: toggle_zoom via AppState
    // ============================================================

    #[test]
    fn toggle_zoom_delegates_to_workspace() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let pane_id = state.workspace(ws_id).unwrap().pane_ids()[0];
        let result = state.toggle_zoom(ws_id, pane_id).expect("toggle_zoom should succeed");
        assert_eq!(result, Some(pane_id));
    }

    // ============================================================
    // VEI-11: rename_workspace
    // ============================================================

    #[test]
    fn rename_workspace_changes_name() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("original".to_string(), PathBuf::from("/tmp"));
        state.rename_workspace(ws_id, "renamed".to_string()).expect("rename should succeed");
        assert_eq!(state.workspace(ws_id).unwrap().name, "renamed");
    }

    #[test]
    fn rename_workspace_preserves_other_state() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp/mydir"));
        let pane_id = state.workspace(ws_id).unwrap().pane_ids()[0];
        // Split to make the layout non-trivial
        state.split_pane(ws_id, pane_id, SplitDirection::Horizontal).expect("split should succeed");
        let pane_count_before = state.workspace(ws_id).unwrap().layout.pane_count();
        let wd_before = state.workspace(ws_id).unwrap().working_directory.clone();

        state.rename_workspace(ws_id, "new name".to_string()).expect("rename should succeed");

        let ws = state.workspace(ws_id).unwrap();
        assert_eq!(ws.layout.pane_count(), pane_count_before);
        assert_eq!(ws.working_directory, wd_before);
    }

    #[test]
    fn rename_active_workspace_does_not_change_active_id() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        assert_eq!(state.active_workspace_id, Some(ws_id));
        state.rename_workspace(ws_id, "new name".to_string()).expect("rename should succeed");
        assert_eq!(state.active_workspace_id, Some(ws_id));
    }

    #[test]
    fn rename_nonexistent_workspace_returns_error() {
        let mut state = AppState::new();
        let result = state.rename_workspace(WorkspaceId::new(999), "foo".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StateError::WorkspaceNotFound(_)));
    }

    #[test]
    fn rename_to_empty_string_succeeds() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        state.rename_workspace(ws_id, String::new()).expect("rename to empty should succeed");
        assert_eq!(state.workspace(ws_id).unwrap().name, "");
    }

    #[test]
    fn rename_to_same_name_succeeds() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        state.rename_workspace(ws_id, "ws".to_string()).expect("rename to same should succeed");
        assert_eq!(state.workspace(ws_id).unwrap().name, "ws");
    }

    // ============================================================
    // VEI-11: reorder_workspace
    // ============================================================

    fn create_n_workspaces(state: &mut AppState, n: usize) -> Vec<WorkspaceId> {
        (0..n)
            .map(|i| state.create_workspace(format!("ws{i}"), PathBuf::from(format!("/tmp/{i}"))))
            .collect()
    }

    #[test]
    fn reorder_workspace_from_first_to_last() {
        let mut state = AppState::new();
        let ids = create_n_workspaces(&mut state, 3);
        state.reorder_workspace(ids[0], 2).expect("reorder should succeed");
        assert_eq!(state.workspaces[0].id, ids[1]);
        assert_eq!(state.workspaces[1].id, ids[2]);
        assert_eq!(state.workspaces[2].id, ids[0]);
    }

    #[test]
    fn reorder_workspace_from_last_to_first() {
        let mut state = AppState::new();
        let ids = create_n_workspaces(&mut state, 3);
        state.reorder_workspace(ids[2], 0).expect("reorder should succeed");
        assert_eq!(state.workspaces[0].id, ids[2]);
        assert_eq!(state.workspaces[1].id, ids[0]);
        assert_eq!(state.workspaces[2].id, ids[1]);
    }

    #[test]
    fn reorder_active_workspace_preserves_active_id() {
        let mut state = AppState::new();
        let ids = create_n_workspaces(&mut state, 3);
        // First workspace is active by default
        assert_eq!(state.active_workspace_id, Some(ids[0]));
        state.reorder_workspace(ids[0], 2).expect("reorder should succeed");
        assert_eq!(
            state.active_workspace_id,
            Some(ids[0]),
            "active workspace ID should not change after reorder"
        );
    }

    #[test]
    fn reorder_nonexistent_workspace_returns_error() {
        let mut state = AppState::new();
        create_n_workspaces(&mut state, 2);
        let result = state.reorder_workspace(WorkspaceId::new(999), 0);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StateError::WorkspaceNotFound(_)));
    }

    #[test]
    fn reorder_to_same_position_is_noop() {
        let mut state = AppState::new();
        let ids = create_n_workspaces(&mut state, 3);
        state.reorder_workspace(ids[1], 1).expect("reorder to same position should succeed");
        assert_eq!(state.workspaces[0].id, ids[0]);
        assert_eq!(state.workspaces[1].id, ids[1]);
        assert_eq!(state.workspaces[2].id, ids[2]);
    }

    #[test]
    fn reorder_clamps_to_last_position() {
        let mut state = AppState::new();
        let ids = create_n_workspaces(&mut state, 3);
        state.reorder_workspace(ids[0], 100).expect("reorder beyond length should be clamped");
        assert_eq!(
            state.workspaces[2].id, ids[0],
            "workspace should be at the clamped last position"
        );
    }

    #[test]
    fn reorder_single_workspace_is_noop() {
        let mut state = AppState::new();
        let ids = create_n_workspaces(&mut state, 1);
        state
            .reorder_workspace(ids[0], 0)
            .expect("reorder in single-workspace list should succeed");
        assert_eq!(state.workspaces[0].id, ids[0]);
    }

    #[test]
    fn reorder_preserves_workspace_internal_state() {
        let mut state = AppState::new();
        let ids = create_n_workspaces(&mut state, 3);
        // Modify the workspace we're about to reorder
        let pane_id = state.workspace(ids[0]).unwrap().pane_ids()[0];
        state
            .split_pane(ids[0], pane_id, SplitDirection::Horizontal)
            .expect("split should succeed");
        let pane_count_before = state.workspace(ids[0]).unwrap().layout.pane_count();

        state.reorder_workspace(ids[0], 2).expect("reorder should succeed");

        let ws = state.workspace(ids[0]).unwrap();
        assert_eq!(ws.layout.pane_count(), pane_count_before);
        assert_eq!(ws.name, "ws0");
    }

    // ============================================================
    // VEI-11: swap_workspaces
    // ============================================================

    #[test]
    fn swap_workspaces_exchanges_positions() {
        let mut state = AppState::new();
        let ids = create_n_workspaces(&mut state, 3);
        state.swap_workspaces(ids[0], ids[2]).expect("swap should succeed");
        assert_eq!(state.workspaces[0].id, ids[2]);
        assert_eq!(state.workspaces[1].id, ids[1]);
        assert_eq!(state.workspaces[2].id, ids[0]);
    }

    #[test]
    fn swap_nonexistent_first_workspace_returns_error() {
        let mut state = AppState::new();
        let ids = create_n_workspaces(&mut state, 2);
        let result = state.swap_workspaces(WorkspaceId::new(999), ids[0]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StateError::WorkspaceNotFound(_)));
    }

    #[test]
    fn swap_nonexistent_second_workspace_returns_error() {
        let mut state = AppState::new();
        let ids = create_n_workspaces(&mut state, 2);
        let result = state.swap_workspaces(ids[0], WorkspaceId::new(999));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StateError::WorkspaceNotFound(_)));
    }

    #[test]
    fn swap_workspace_with_itself_is_noop() {
        let mut state = AppState::new();
        let ids = create_n_workspaces(&mut state, 3);
        state.swap_workspaces(ids[1], ids[1]).expect("swap with self should succeed");
        assert_eq!(state.workspaces[0].id, ids[0]);
        assert_eq!(state.workspaces[1].id, ids[1]);
        assert_eq!(state.workspaces[2].id, ids[2]);
    }
}
