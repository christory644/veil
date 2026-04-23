//! Central application state — single source of truth for UI rendering.
//!
//! The UI thread reads `AppState` each frame to render. Background actors push
//! `StateUpdate` messages to `AppState` via tokio mpsc channels.

use chrono::Utc;
use std::path::PathBuf;

use crate::error::{ErrorId, ErrorReport};
use crate::notification::{Notification, NotificationId, NotificationSource, NotificationStore};
use crate::session::SessionEntry;
use crate::workspace::{PaneId, SplitDirection, SurfaceId, Workspace, WorkspaceError, WorkspaceId};

/// An error tracked in `AppState`, with an assigned ID.
#[derive(Debug, Clone)]
pub struct TrackedError {
    /// Unique identifier for this error.
    pub id: ErrorId,
    /// The error report.
    pub report: ErrorReport,
}

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
    pub notifications: NotificationStore,
    /// Sidebar display state.
    pub sidebar: SidebarState,
    /// Active errors being displayed to the user.
    pub errors: Vec<TrackedError>,
    next_id: u64,
}

impl AppState {
    /// Create a new `AppState` with defaults: no workspaces, sidebar visible, Workspaces tab.
    pub fn new() -> Self {
        Self {
            workspaces: Vec::new(),
            active_workspace_id: None,
            conversations: ConversationIndex::default(),
            notifications: NotificationStore::new(),
            sidebar: SidebarState {
                visible: true,
                active_tab: SidebarTab::Workspaces,
                width_px: 250,
            },
            errors: Vec::new(),
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
        let notif = Notification {
            id: NotificationId::new(id),
            source: NotificationSource::Internal,
            message,
            title: None,
            workspace_id,
            surface_id: None,
            created_at: Utc::now(),
            read: false,
        };
        self.notifications.add(notif);
    }

    /// Mark a notification as acknowledged.
    pub fn acknowledge_notification(&mut self, id: u64) {
        self.notifications.mark_read(NotificationId::new(id));
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

    /// Add a notification with full source and metadata via the notification store.
    pub fn add_notification_with_source(
        &mut self,
        workspace_id: WorkspaceId,
        surface_id: Option<SurfaceId>,
        message: String,
        title: Option<String>,
        source: NotificationSource,
    ) -> NotificationId {
        let id = self.next_id();
        let notif = Notification {
            id: NotificationId::new(id),
            source,
            message,
            title,
            workspace_id,
            surface_id,
            created_at: Utc::now(),
            read: false,
        };
        self.notifications.add(notif)
    }

    /// Dismiss (remove) a notification by its `NotificationId`.
    pub fn dismiss_notification(&mut self, id: NotificationId) -> bool {
        self.notifications.dismiss(id)
    }

    /// Clear all notifications for a workspace.
    pub fn clear_workspace_notifications(&mut self, workspace_id: WorkspaceId) {
        self.notifications.clear_workspace(workspace_id);
    }

    /// Clear all notifications.
    pub fn clear_all_notifications(&mut self) {
        self.notifications.clear_all();
    }

    /// Get unread notification count for a workspace.
    pub fn unread_notification_count(&self, workspace_id: WorkspaceId) -> usize {
        self.notifications.unread_count(workspace_id)
    }

    /// Get the latest notification for a workspace.
    pub fn latest_notification(&self, workspace_id: WorkspaceId) -> Option<&Notification> {
        self.notifications.latest_for_workspace(workspace_id)
    }

    /// Track a new error. Returns the assigned `ErrorId`.
    pub fn add_error(&mut self, report: ErrorReport) -> ErrorId {
        let id = ErrorId::new(self.next_id());
        self.errors.push(TrackedError { id, report });
        id
    }

    /// Dismiss (remove) an error by its ID. Returns true if found.
    pub fn dismiss_error(&mut self, id: ErrorId) -> bool {
        let len_before = self.errors.len();
        self.errors.retain(|e| e.id != id);
        self.errors.len() < len_before
    }

    /// Get all active errors.
    pub fn active_errors(&self) -> &[TrackedError] {
        &self.errors
    }

    /// Get errors associated with a specific pane.
    pub fn errors_for_pane(&self, pane_id: PaneId) -> Vec<&TrackedError> {
        self.errors.iter().filter(|e| e.report.pane_id == Some(pane_id)).collect()
    }

    /// Get errors not associated with any pane (global errors).
    pub fn global_errors(&self) -> Vec<&TrackedError> {
        self.errors.iter().filter(|e| e.report.pane_id.is_none()).collect()
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
        assert_eq!(state.notifications.all()[0].message, "hello");
        assert!(!state.notifications.all()[0].read);
    }

    #[test]
    fn acknowledge_notification_marks_it() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        state.add_notification(ws_id, "hello".to_string());
        let notif_id = state.notifications.all()[0].id.as_u64();
        state.acknowledge_notification(notif_id);
        assert!(state.notifications.all()[0].read);
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

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Operation enum for driving arbitrary state machine sequences.
    #[derive(Debug, Clone)]
    enum StateOp {
        Create,
        Close(usize),
        SetActive(usize),
    }

    fn arb_state_op() -> impl Strategy<Value = StateOp> {
        prop_oneof![
            Just(StateOp::Create),
            (0..10usize).prop_map(StateOp::Close),
            (0..10usize).prop_map(StateOp::SetActive),
        ]
    }

    proptest! {
        /// After any sequence of create/close/set_active operations,
        /// active_workspace_id is always either None or refers to an
        /// existing workspace.
        #[test]
        fn active_workspace_always_valid_or_none(
            ops in proptest::collection::vec(arb_state_op(), 0..50)
        ) {
            let mut state = AppState::new();

            for op in &ops {
                match op {
                    StateOp::Create => {
                        state.create_workspace(
                            "ws".to_string(),
                            PathBuf::from("/tmp"),
                        );
                    }
                    StateOp::Close(idx) => {
                        if !state.workspaces.is_empty() {
                            let ws_idx = idx % state.workspaces.len();
                            let ws_id = state.workspaces[ws_idx].id;
                            let _ = state.close_workspace(ws_id);
                        }
                    }
                    StateOp::SetActive(idx) => {
                        if !state.workspaces.is_empty() {
                            let ws_idx = idx % state.workspaces.len();
                            let ws_id = state.workspaces[ws_idx].id;
                            let _ = state.set_active_workspace(ws_id);
                        }
                    }
                }

                // Invariant: active_workspace_id is None or points to existing workspace
                if let Some(active_id) = state.active_workspace_id {
                    prop_assert!(
                        state.workspace(active_id).is_some(),
                        "active_workspace_id {:?} does not refer to an existing workspace",
                        active_id,
                    );
                }
            }
        }

        /// After any sequence of split/close pane operations, the pane tree
        /// always has at least 1 leaf.
        #[test]
        fn pane_tree_always_has_at_least_one_leaf(
            split_count in 0..20usize,
            close_indices in proptest::collection::vec(0..20usize, 0..15),
        ) {
            let mut state = AppState::new();
            let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));

            // Perform splits
            for _ in 0..split_count {
                let pane_ids = state.workspace(ws_id).unwrap().pane_ids();
                let target = pane_ids[0]; // Always split the first pane
                let _ = state.split_pane(ws_id, target, SplitDirection::Horizontal);
            }

            // Perform closes (skipping errors like LastPane)
            for idx in &close_indices {
                let pane_ids = state.workspace(ws_id).unwrap().pane_ids();
                if pane_ids.len() > 1 {
                    let target_idx = idx % pane_ids.len();
                    let target = pane_ids[target_idx];
                    let _ = state.close_pane(ws_id, target);
                }
            }

            // Invariant: pane tree always has at least 1 leaf
            let ws = state.workspace(ws_id).unwrap();
            prop_assert!(
                ws.layout.pane_count() >= 1,
                "pane tree should have at least 1 leaf, got {}",
                ws.layout.pane_count(),
            );
        }

        /// next_id is strictly monotonically increasing across any number of calls.
        #[test]
        fn next_id_strictly_monotonic(call_count in 1..100usize) {
            let mut state = AppState::new();
            let mut prev = state.next_id();
            for _ in 1..call_count {
                let current = state.next_id();
                prop_assert!(
                    current > prev,
                    "next_id must be strictly increasing: {} <= {}",
                    current, prev,
                );
                prev = current;
            }
        }
    }
}

#[cfg(test)]
mod notification_integration_tests {
    use super::*;
    use crate::notification::{NotificationId, NotificationSource};

    // --- add_notification_with_source ---

    #[test]
    fn add_notification_with_source_returns_valid_id() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let notif_id = state.add_notification_with_source(
            ws_id,
            None,
            "hello".to_string(),
            None,
            NotificationSource::Internal,
        );
        // The ID should be valid (non-zero or specific value depends on impl)
        let _ = notif_id.as_u64();
    }

    #[test]
    fn add_notification_with_source_creates_entry_in_store() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        state.add_notification_with_source(
            ws_id,
            None,
            "test message".to_string(),
            None,
            NotificationSource::Internal,
        );
        let latest = state.latest_notification(ws_id);
        assert!(latest.is_some(), "notification should exist in store after adding");
        assert_eq!(latest.unwrap().message, "test message");
    }

    #[test]
    fn add_notification_with_osc_source() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let source =
            NotificationSource::Osc { sequence_type: crate::notification::OscSequenceType::Osc9 };
        state.add_notification_with_source(
            ws_id,
            None,
            "osc notification".to_string(),
            None,
            source,
        );
        let latest = state.latest_notification(ws_id);
        assert!(latest.is_some());
        assert!(
            matches!(latest.unwrap().source, NotificationSource::Osc { .. }),
            "source should be Osc"
        );
    }

    #[test]
    fn add_notification_with_title_and_surface_id() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let surface_id = state.workspace(ws_id).unwrap().layout.surface_ids()[0];
        state.add_notification_with_source(
            ws_id,
            Some(surface_id),
            "body".to_string(),
            Some("Title".to_string()),
            NotificationSource::SocketApi,
        );
        let latest = state.latest_notification(ws_id);
        assert!(latest.is_some());
        let notif = latest.unwrap();
        assert_eq!(notif.title.as_deref(), Some("Title"));
        assert_eq!(notif.surface_id, Some(surface_id));
    }

    // --- dismiss_notification ---

    #[test]
    fn dismiss_notification_removes_entry() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let notif_id = state.add_notification_with_source(
            ws_id,
            None,
            "to dismiss".to_string(),
            None,
            NotificationSource::Internal,
        );
        let result = state.dismiss_notification(notif_id);
        assert!(result, "dismiss should return true");
        assert!(
            state.latest_notification(ws_id).is_none(),
            "notification should be gone after dismiss"
        );
    }

    #[test]
    fn dismiss_notification_nonexistent_returns_false() {
        let mut state = AppState::new();
        let result = state.dismiss_notification(NotificationId::new(999));
        assert!(!result);
    }

    // --- clear_workspace_notifications ---

    #[test]
    fn clear_workspace_notifications_removes_all_for_workspace() {
        let mut state = AppState::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));
        state.add_notification_with_source(
            ws1,
            None,
            "ws1 notif".to_string(),
            None,
            NotificationSource::Internal,
        );
        state.add_notification_with_source(
            ws2,
            None,
            "ws2 notif".to_string(),
            None,
            NotificationSource::Internal,
        );

        state.clear_workspace_notifications(ws1);

        assert!(state.latest_notification(ws1).is_none(), "ws1 notifications should be cleared");
        assert!(state.latest_notification(ws2).is_some(), "ws2 notifications should remain");
    }

    // --- clear_all_notifications ---

    #[test]
    fn clear_all_notifications_empties_everything() {
        let mut state = AppState::new();
        let ws1 = state.create_workspace("ws1".to_string(), PathBuf::from("/tmp/1"));
        let ws2 = state.create_workspace("ws2".to_string(), PathBuf::from("/tmp/2"));
        state.add_notification_with_source(
            ws1,
            None,
            "notif 1".to_string(),
            None,
            NotificationSource::Internal,
        );
        state.add_notification_with_source(
            ws2,
            None,
            "notif 2".to_string(),
            None,
            NotificationSource::Internal,
        );

        state.clear_all_notifications();

        assert!(state.latest_notification(ws1).is_none());
        assert!(state.latest_notification(ws2).is_none());
    }

    // --- unread_notification_count ---

    #[test]
    fn unread_count_delegates_to_store() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        state.add_notification_with_source(
            ws_id,
            None,
            "notif 1".to_string(),
            None,
            NotificationSource::Internal,
        );
        state.add_notification_with_source(
            ws_id,
            None,
            "notif 2".to_string(),
            None,
            NotificationSource::Internal,
        );

        assert_eq!(state.unread_notification_count(ws_id), 2);
    }

    #[test]
    fn unread_count_zero_for_empty_workspace() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        assert_eq!(state.unread_notification_count(ws_id), 0);
    }

    // --- latest_notification ---

    #[test]
    fn latest_notification_returns_most_recent() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        state.add_notification_with_source(
            ws_id,
            None,
            "first".to_string(),
            None,
            NotificationSource::Internal,
        );
        state.add_notification_with_source(
            ws_id,
            None,
            "second".to_string(),
            None,
            NotificationSource::Internal,
        );

        let latest = state.latest_notification(ws_id);
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().message, "second");
    }

    #[test]
    fn latest_notification_none_for_no_notifications() {
        let mut state = AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        assert!(state.latest_notification(ws_id).is_none());
    }
}

// ============================================================
// VEI-20: Error tracking in AppState
// ============================================================

#[cfg(test)]
mod error_tracking_tests {
    use super::*;
    use crate::error::{ErrorComponent, ErrorId, ErrorReport, ErrorSeverity, RecoveryAction};

    fn make_error_report(msg: &str) -> ErrorReport {
        ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, msg)
    }

    fn make_pane_error(msg: &str, pane_id: PaneId) -> ErrorReport {
        ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Terminal, msg).with_pane_id(pane_id)
    }

    fn make_global_error(msg: &str) -> ErrorReport {
        ErrorReport::new(ErrorSeverity::Warning, ErrorComponent::System, msg)
    }

    // --- new state ---

    #[test]
    fn new_state_has_no_errors() {
        let state = AppState::new();
        assert!(state.errors.is_empty());
        assert!(state.active_errors().is_empty());
    }

    // --- add_error ---

    #[test]
    fn add_error_returns_unique_id() {
        let mut state = AppState::new();
        let id1 = state.add_error(make_error_report("error 1"));
        let id2 = state.add_error(make_error_report("error 2"));
        assert_ne!(id1, id2, "each add_error should return a unique ID");
    }

    #[test]
    fn add_error_increases_error_count() {
        let mut state = AppState::new();
        assert_eq!(state.active_errors().len(), 0);
        state.add_error(make_error_report("error 1"));
        assert_eq!(state.active_errors().len(), 1);
        state.add_error(make_error_report("error 2"));
        assert_eq!(state.active_errors().len(), 2);
    }

    #[test]
    fn add_error_preserves_report_fields() {
        let mut state = AppState::new();
        let report = ErrorReport::new(ErrorSeverity::Fatal, ErrorComponent::Pty, "process crashed")
            .with_detail("segfault")
            .with_suggestion("restart")
            .with_recovery_actions(vec![RecoveryAction::Close]);

        let id = state.add_error(report);
        let tracked = state.active_errors().iter().find(|e| e.id == id);
        assert!(tracked.is_some(), "error should be in active_errors");
        let tracked = tracked.unwrap();
        assert_eq!(tracked.report.severity, ErrorSeverity::Fatal);
        assert_eq!(tracked.report.component, ErrorComponent::Pty);
        assert_eq!(tracked.report.message, "process crashed");
        assert_eq!(tracked.report.detail.as_deref(), Some("segfault"));
        assert_eq!(tracked.report.suggestion.as_deref(), Some("restart"));
        assert_eq!(tracked.report.recovery_actions, vec![RecoveryAction::Close]);
    }

    // --- dismiss_error ---

    #[test]
    fn dismiss_error_removes_it() {
        let mut state = AppState::new();
        let id = state.add_error(make_error_report("to dismiss"));
        state.dismiss_error(id);
        assert!(
            state.active_errors().iter().all(|e| e.id != id),
            "dismissed error should be gone from active_errors"
        );
    }

    #[test]
    fn dismiss_error_returns_true() {
        let mut state = AppState::new();
        let id = state.add_error(make_error_report("to dismiss"));
        let result = state.dismiss_error(id);
        assert!(result, "dismiss_error should return true when error exists");
    }

    #[test]
    fn dismiss_nonexistent_error_returns_false() {
        let mut state = AppState::new();
        let result = state.dismiss_error(ErrorId::new(999));
        assert!(!result, "dismiss_error should return false for unknown ID");
    }

    #[test]
    fn dismiss_error_does_not_affect_others() {
        let mut state = AppState::new();
        let id1 = state.add_error(make_error_report("keep this"));
        let id2 = state.add_error(make_error_report("dismiss this"));
        let id3 = state.add_error(make_error_report("keep this too"));
        state.dismiss_error(id2);
        assert_eq!(state.active_errors().len(), 2);
        assert!(state.active_errors().iter().any(|e| e.id == id1));
        assert!(state.active_errors().iter().any(|e| e.id == id3));
    }

    // --- errors_for_pane ---

    #[test]
    fn errors_for_pane_filters_correctly() {
        let mut state = AppState::new();
        let pane_a = PaneId::new(10);
        let pane_b = PaneId::new(20);
        state.add_error(make_pane_error("error A1", pane_a));
        state.add_error(make_pane_error("error A2", pane_a));
        state.add_error(make_pane_error("error B1", pane_b));
        state.add_error(make_global_error("global error"));

        let pane_a_errors = state.errors_for_pane(pane_a);
        assert_eq!(pane_a_errors.len(), 2, "should have 2 errors for pane_a");
        for e in &pane_a_errors {
            assert_eq!(e.report.pane_id, Some(pane_a));
        }
    }

    #[test]
    fn errors_for_pane_empty_when_no_match() {
        let mut state = AppState::new();
        state.add_error(make_global_error("global only"));
        let result = state.errors_for_pane(PaneId::new(999));
        assert!(result.is_empty(), "should return empty vec for pane with no errors");
    }

    // --- global_errors ---

    #[test]
    fn global_errors_filters_correctly() {
        let mut state = AppState::new();
        let pane = PaneId::new(10);
        state.add_error(make_global_error("global 1"));
        state.add_error(make_global_error("global 2"));
        state.add_error(make_pane_error("pane error", pane));

        let globals = state.global_errors();
        assert_eq!(globals.len(), 2, "should have 2 global errors");
        for e in &globals {
            assert!(e.report.pane_id.is_none());
        }
    }

    #[test]
    fn global_errors_excludes_pane_errors() {
        let mut state = AppState::new();
        let pane = PaneId::new(10);
        state.add_error(make_pane_error("pane error 1", pane));
        state.add_error(make_pane_error("pane error 2", pane));

        let globals = state.global_errors();
        assert!(globals.is_empty(), "global_errors should exclude all pane-associated errors");
    }

    // --- error IDs share sequence with other IDs ---

    #[test]
    fn error_ids_share_sequence_with_other_ids() {
        let mut state = AppState::new();
        // Create a workspace (consumes some IDs from next_id)
        let _ws_id = state.create_workspace("ws".to_string(), PathBuf::from("/tmp"));
        let next_before = state.next_id;
        let error_id = state.add_error(make_error_report("test error"));
        // The error ID should come from the same next_id() sequence.
        // If next_id was N before add_error, the error_id should be N.
        assert_eq!(
            error_id.as_u64(),
            next_before,
            "error ID should come from the global next_id sequence"
        );
    }
}

#[cfg(test)]
mod error_channel_tests {
    use crate::error::{ErrorComponent, ErrorId, ErrorReport, ErrorSeverity};
    use crate::message::{Channels, StateUpdate};

    #[tokio::test]
    async fn state_update_error_occurred_round_trip() {
        let channels = Channels::new(16);
        let Channels { state_tx, mut state_rx, .. } = channels;

        let report = ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, "test error");

        state_tx.send(StateUpdate::ErrorOccurred(report)).await.expect("send should succeed");

        let msg = state_rx.recv().await.expect("should receive message");
        match msg {
            StateUpdate::ErrorOccurred(r) => {
                assert_eq!(r.component, ErrorComponent::Config);
            }
            other => panic!("expected ErrorOccurred, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn state_update_error_dismissed_round_trip() {
        let channels = Channels::new(16);
        let Channels { state_tx, mut state_rx, .. } = channels;

        state_tx
            .send(StateUpdate::ErrorDismissed { error_id: ErrorId::new(42) })
            .await
            .expect("send should succeed");

        let msg = state_rx.recv().await.expect("should receive message");
        match msg {
            StateUpdate::ErrorDismissed { error_id } => {
                assert_eq!(error_id, ErrorId::new(42));
            }
            other => panic!("expected ErrorDismissed, got: {other:?}"),
        }
    }
}

#[cfg(test)]
mod error_tracking_proptests {
    use super::*;
    use crate::error::{ErrorComponent, ErrorId, ErrorReport, ErrorSeverity};
    use proptest::prelude::*;

    /// Operation enum for driving arbitrary add/dismiss sequences.
    #[derive(Debug, Clone)]
    enum ErrorOp {
        Add,
        Dismiss(usize),
    }

    fn arb_error_op() -> impl Strategy<Value = ErrorOp> {
        prop_oneof![Just(ErrorOp::Add), (0..20usize).prop_map(ErrorOp::Dismiss),]
    }

    proptest! {
        /// After random add/dismiss sequences: no duplicate IDs in active_errors,
        /// and active_errors count matches additions minus successful dismissals.
        #[test]
        fn add_dismiss_invariants(
            ops in proptest::collection::vec(arb_error_op(), 0..50)
        ) {
            let mut state = AppState::new();
            let mut added_ids: Vec<ErrorId> = Vec::new();
            let mut successful_dismissals = 0usize;

            for op in &ops {
                match op {
                    ErrorOp::Add => {
                        let report = ErrorReport::new(
                            ErrorSeverity::Error,
                            ErrorComponent::Config,
                            "test",
                        );
                        let id = state.add_error(report);
                        added_ids.push(id);
                    }
                    ErrorOp::Dismiss(idx) => {
                        if !added_ids.is_empty() {
                            let target_idx = idx % added_ids.len();
                            let target_id = added_ids[target_idx];
                            if state.dismiss_error(target_id) {
                                successful_dismissals += 1;
                            }
                        }
                    }
                }
            }

            // Invariant 1: no duplicate IDs in active_errors
            let active = state.active_errors();
            let mut seen_ids = std::collections::HashSet::new();
            for tracked in active {
                prop_assert!(
                    seen_ids.insert(tracked.id),
                    "duplicate error ID {:?} in active_errors",
                    tracked.id,
                );
            }

            // Invariant 2: count matches additions minus successful dismissals
            let expected_count = added_ids.len() - successful_dismissals;
            prop_assert_eq!(
                active.len(),
                expected_count,
                "active_errors count should be {} (added) - {} (dismissed) = {}, got {}",
                added_ids.len(),
                successful_dismissals,
                expected_count,
                active.len(),
            );
        }
    }
}
