//! Notification data model and store.
//!
//! Defines the enriched notification types that replace the legacy
//! `NotificationEntry` in `state.rs`, plus a `NotificationStore` that
//! manages notification lifecycle (create, read, dismiss, clear, query).

use std::fmt;

use chrono::{DateTime, Utc};

use crate::workspace::{SurfaceId, WorkspaceId};

/// Maximum number of notifications retained in the store.
/// Oldest notifications are evicted when this limit is exceeded.
pub const MAX_NOTIFICATIONS: usize = 500;

/// Where the notification originated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationSource {
    /// OSC escape sequence from terminal output.
    Osc {
        /// Which OSC sequence type produced this notification.
        sequence_type: OscSequenceType,
    },
    /// Socket API `notification.create` call.
    SocketApi,
    /// Internal Veil event (e.g., process exit, error).
    Internal,
}

impl fmt::Display for NotificationSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

/// Which OSC sequence produced the notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscSequenceType {
    /// OSC 9 -- iTerm2/ConEmu notification.
    Osc9,
    /// OSC 99 -- kitty notification protocol.
    Osc99,
    /// OSC 777 -- rxvt-unicode notification.
    Osc777,
}

/// Unique notification identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NotificationId(u64);

impl NotificationId {
    /// Create a new `NotificationId`.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the underlying `u64` value.
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// A notification in the system.
#[derive(Debug, Clone)]
pub struct Notification {
    /// Unique identifier.
    pub id: NotificationId,
    /// Where this notification came from.
    pub source: NotificationSource,
    /// Notification message body.
    pub message: String,
    /// Optional title (OSC 99 supports separate title/body).
    pub title: Option<String>,
    /// Which workspace this notification belongs to.
    pub workspace_id: WorkspaceId,
    /// Which pane (surface) generated this notification, if known.
    pub surface_id: Option<SurfaceId>,
    /// When the notification was created.
    pub created_at: DateTime<Utc>,
    /// Whether the notification has been read.
    pub read: bool,
}

/// Manages notification lifecycle: create, read, dismiss, clear, query.
#[derive(Debug, Clone)]
pub struct NotificationStore {
    notifications: Vec<Notification>,
}

impl NotificationStore {
    /// Create a new empty `NotificationStore`.
    pub fn new() -> Self {
        todo!()
    }

    /// Add a notification to the store. If the store exceeds
    /// `MAX_NOTIFICATIONS`, the oldest notification is evicted.
    /// Returns the ID of the new notification.
    pub fn add(&mut self, notification: Notification) -> NotificationId {
        todo!()
    }

    /// Mark a notification as read. Returns `true` if the notification
    /// was found, `false` otherwise.
    pub fn mark_read(&mut self, id: NotificationId) -> bool {
        todo!()
    }

    /// Mark all notifications for a workspace as read.
    pub fn mark_all_read(&mut self, workspace_id: WorkspaceId) {
        todo!()
    }

    /// Remove a single notification (dismiss). Returns `true` if the
    /// notification was found and removed, `false` otherwise.
    pub fn dismiss(&mut self, id: NotificationId) -> bool {
        todo!()
    }

    /// Remove all notifications for a workspace.
    pub fn clear_workspace(&mut self, workspace_id: WorkspaceId) {
        todo!()
    }

    /// Remove all notifications.
    pub fn clear_all(&mut self) {
        todo!()
    }

    /// Count unread notifications for a workspace.
    pub fn unread_count(&self, workspace_id: WorkspaceId) -> usize {
        todo!()
    }

    /// Get the most recent notification for a workspace (for subtitle display).
    pub fn latest_for_workspace(&self, workspace_id: WorkspaceId) -> Option<&Notification> {
        todo!()
    }

    /// Get all notifications for a workspace, most recent first.
    pub fn for_workspace(&self, workspace_id: WorkspaceId) -> Vec<&Notification> {
        todo!()
    }

    /// Get all notifications, most recent first.
    pub fn all(&self) -> &[Notification] {
        todo!()
    }

    /// Total count of notifications in the store.
    pub fn len(&self) -> usize {
        todo!()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        todo!()
    }
}

impl Default for NotificationStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{SurfaceId, WorkspaceId};
    use chrono::Utc;
    use proptest::prelude::*;

    // ================================================================
    // Helpers
    // ================================================================

    fn make_notification(id: u64, workspace_id: u64, message: &str) -> Notification {
        Notification {
            id: NotificationId::new(id),
            source: NotificationSource::Internal,
            message: message.to_string(),
            title: None,
            workspace_id: WorkspaceId::new(workspace_id),
            surface_id: None,
            created_at: Utc::now(),
            read: false,
        }
    }

    fn make_notification_with_timestamp(
        id: u64,
        workspace_id: u64,
        message: &str,
        created_at: DateTime<Utc>,
    ) -> Notification {
        Notification {
            id: NotificationId::new(id),
            source: NotificationSource::Internal,
            message: message.to_string(),
            title: None,
            workspace_id: WorkspaceId::new(workspace_id),
            surface_id: None,
            created_at,
            read: false,
        }
    }

    // ================================================================
    // Unit 1: Notification data model
    // ================================================================

    #[test]
    fn notification_id_equality() {
        let a = NotificationId::new(42);
        let b = NotificationId::new(42);
        assert_eq!(a, b);
    }

    #[test]
    fn notification_id_inequality() {
        let a = NotificationId::new(1);
        let b = NotificationId::new(2);
        assert_ne!(a, b);
    }

    #[test]
    fn notification_id_as_u64_roundtrip() {
        let id = NotificationId::new(99);
        assert_eq!(id.as_u64(), 99);
    }

    #[test]
    fn notification_source_display_osc() {
        let source = NotificationSource::Osc { sequence_type: OscSequenceType::Osc9 };
        let display = format!("{source}");
        assert!(!display.is_empty(), "Display for Osc source should produce non-empty string");
    }

    #[test]
    fn notification_source_display_socket_api() {
        let source = NotificationSource::SocketApi;
        let display = format!("{source}");
        assert!(
            !display.is_empty(),
            "Display for SocketApi source should produce non-empty string"
        );
    }

    #[test]
    fn notification_source_display_internal() {
        let source = NotificationSource::Internal;
        let display = format!("{source}");
        assert!(!display.is_empty(), "Display for Internal source should produce non-empty string");
    }

    #[test]
    fn osc_sequence_type_variants_are_distinct() {
        assert_ne!(OscSequenceType::Osc9, OscSequenceType::Osc99);
        assert_ne!(OscSequenceType::Osc9, OscSequenceType::Osc777);
        assert_ne!(OscSequenceType::Osc99, OscSequenceType::Osc777);
    }

    #[test]
    fn notification_default_unread() {
        let notif = make_notification(1, 1, "test");
        assert!(!notif.read, "newly constructed notification should have read: false");
    }

    #[test]
    fn notification_with_title() {
        let notif = Notification {
            id: NotificationId::new(1),
            source: NotificationSource::Osc { sequence_type: OscSequenceType::Osc99 },
            message: "body text".to_string(),
            title: Some("My Title".to_string()),
            workspace_id: WorkspaceId::new(1),
            surface_id: None,
            created_at: Utc::now(),
            read: false,
        };
        assert_eq!(notif.title.as_deref(), Some("My Title"));
    }

    #[test]
    fn notification_without_title() {
        let notif = make_notification(1, 1, "body only");
        assert_eq!(notif.title, None);
    }

    #[test]
    fn notification_with_surface_id() {
        let notif = Notification {
            id: NotificationId::new(1),
            source: NotificationSource::Internal,
            message: "test".to_string(),
            title: None,
            workspace_id: WorkspaceId::new(1),
            surface_id: Some(SurfaceId::new(42)),
            created_at: Utc::now(),
            read: false,
        };
        assert_eq!(notif.surface_id, Some(SurfaceId::new(42)));
    }

    #[test]
    fn notification_without_surface_id() {
        let notif = make_notification(1, 1, "test");
        assert_eq!(notif.surface_id, None);
    }

    // ================================================================
    // Unit 2: Notification store
    // ================================================================

    #[test]
    fn new_store_is_empty() {
        let store = NotificationStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn add_notification_increases_len() {
        let mut store = NotificationStore::new();
        let notif = make_notification(1, 1, "hello");
        store.add(notif);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn add_returns_correct_id() {
        let mut store = NotificationStore::new();
        let notif = make_notification(42, 1, "hello");
        let returned_id = store.add(notif);
        assert_eq!(returned_id, NotificationId::new(42));
    }

    #[test]
    fn mark_read_sets_read_flag() {
        let mut store = NotificationStore::new();
        let notif = make_notification(1, 1, "hello");
        store.add(notif);
        let result = store.mark_read(NotificationId::new(1));
        assert!(result, "mark_read should return true for existing notification");

        let all = store.all();
        assert!(all[0].read, "notification should be marked as read");
    }

    #[test]
    fn mark_read_nonexistent_returns_false() {
        let mut store = NotificationStore::new();
        let result = store.mark_read(NotificationId::new(999));
        assert!(!result);
    }

    #[test]
    fn mark_all_read_for_workspace() {
        let mut store = NotificationStore::new();
        store.add(make_notification(1, 1, "notif 1"));
        store.add(make_notification(2, 1, "notif 2"));
        store.add(make_notification(3, 2, "other workspace"));

        store.mark_all_read(WorkspaceId::new(1));

        let ws1_notifs = store.for_workspace(WorkspaceId::new(1));
        assert!(ws1_notifs.iter().all(|n| n.read), "all ws1 notifications should be read");
    }

    #[test]
    fn mark_all_read_does_not_affect_other_workspaces() {
        let mut store = NotificationStore::new();
        store.add(make_notification(1, 1, "ws1 notif"));
        store.add(make_notification(2, 2, "ws2 notif"));

        store.mark_all_read(WorkspaceId::new(1));

        let ws2_notifs = store.for_workspace(WorkspaceId::new(2));
        assert!(ws2_notifs.iter().all(|n| !n.read), "ws2 notifications should remain unread");
    }

    #[test]
    fn dismiss_removes_notification() {
        let mut store = NotificationStore::new();
        store.add(make_notification(1, 1, "hello"));
        let result = store.dismiss(NotificationId::new(1));
        assert!(result, "dismiss should return true for existing notification");
        assert!(store.is_empty(), "store should be empty after dismissing only notification");
    }

    #[test]
    fn dismiss_nonexistent_returns_false() {
        let mut store = NotificationStore::new();
        let result = store.dismiss(NotificationId::new(999));
        assert!(!result);
    }

    #[test]
    fn clear_workspace_removes_all_for_workspace() {
        let mut store = NotificationStore::new();
        store.add(make_notification(1, 1, "ws1 notif 1"));
        store.add(make_notification(2, 1, "ws1 notif 2"));
        store.add(make_notification(3, 2, "ws2 notif"));

        store.clear_workspace(WorkspaceId::new(1));

        assert_eq!(store.len(), 1, "only ws2 notification should remain");
        let remaining = store.all();
        assert_eq!(remaining[0].workspace_id, WorkspaceId::new(2));
    }

    #[test]
    fn clear_all_empties_store() {
        let mut store = NotificationStore::new();
        store.add(make_notification(1, 1, "notif 1"));
        store.add(make_notification(2, 2, "notif 2"));

        store.clear_all();

        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn unread_count_counts_only_unread() {
        let mut store = NotificationStore::new();
        store.add(make_notification(1, 1, "notif 1"));
        store.add(make_notification(2, 1, "notif 2"));
        store.add(make_notification(3, 1, "notif 3"));

        store.mark_read(NotificationId::new(1));

        assert_eq!(store.unread_count(WorkspaceId::new(1)), 2);
    }

    #[test]
    fn unread_count_counts_only_target_workspace() {
        let mut store = NotificationStore::new();
        store.add(make_notification(1, 1, "ws1 notif"));
        store.add(make_notification(2, 2, "ws2 notif"));
        store.add(make_notification(3, 2, "ws2 notif 2"));

        assert_eq!(store.unread_count(WorkspaceId::new(1)), 1);
        assert_eq!(store.unread_count(WorkspaceId::new(2)), 2);
    }

    #[test]
    fn latest_for_workspace_returns_most_recent() {
        let mut store = NotificationStore::new();
        let t1 = Utc::now() - chrono::Duration::seconds(10);
        let t2 = Utc::now();

        store.add(make_notification_with_timestamp(1, 1, "older", t1));
        store.add(make_notification_with_timestamp(2, 1, "newer", t2));

        let latest = store.latest_for_workspace(WorkspaceId::new(1));
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().message, "newer");
    }

    #[test]
    fn latest_for_workspace_returns_none_when_empty() {
        let store = NotificationStore::new();
        let latest = store.latest_for_workspace(WorkspaceId::new(1));
        assert!(latest.is_none());
    }

    #[test]
    fn for_workspace_returns_most_recent_first() {
        let mut store = NotificationStore::new();
        let t1 = Utc::now() - chrono::Duration::seconds(20);
        let t2 = Utc::now() - chrono::Duration::seconds(10);
        let t3 = Utc::now();

        store.add(make_notification_with_timestamp(1, 1, "oldest", t1));
        store.add(make_notification_with_timestamp(2, 1, "middle", t2));
        store.add(make_notification_with_timestamp(3, 1, "newest", t3));

        let results = store.for_workspace(WorkspaceId::new(1));
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].message, "newest");
        assert_eq!(results[1].message, "middle");
        assert_eq!(results[2].message, "oldest");
    }

    #[test]
    fn for_workspace_filters_to_workspace() {
        let mut store = NotificationStore::new();
        store.add(make_notification(1, 1, "ws1 notif"));
        store.add(make_notification(2, 2, "ws2 notif"));
        store.add(make_notification(3, 1, "ws1 notif 2"));

        let results = store.for_workspace(WorkspaceId::new(1));
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|n| n.workspace_id == WorkspaceId::new(1)));
    }

    #[test]
    fn eviction_at_max_capacity() {
        let mut store = NotificationStore::new();
        // Add MAX_NOTIFICATIONS + 1 notifications
        for i in 0..=MAX_NOTIFICATIONS {
            store.add(make_notification(i as u64, 1, &format!("notif {i}")));
        }
        assert_eq!(store.len(), MAX_NOTIFICATIONS, "store should not exceed MAX_NOTIFICATIONS");
    }

    #[test]
    fn eviction_preserves_newest() {
        let mut store = NotificationStore::new();
        // Add MAX_NOTIFICATIONS + 5 notifications
        let total = MAX_NOTIFICATIONS + 5;
        for i in 0..total {
            let t = Utc::now() + chrono::Duration::seconds(i as i64);
            store.add(make_notification_with_timestamp(i as u64, 1, &format!("notif {i}"), t));
        }

        assert_eq!(store.len(), MAX_NOTIFICATIONS);

        // The newest notification should be the one with the highest id
        let all = store.all();
        let last_id = (total - 1) as u64;
        assert!(
            all.iter().any(|n| n.id == NotificationId::new(last_id)),
            "newest notification should survive eviction"
        );
        // The oldest (id=0) should have been evicted
        assert!(
            !all.iter().any(|n| n.id == NotificationId::new(0)),
            "oldest notification should have been evicted"
        );
    }

    // ================================================================
    // Unit 2: Property-based tests
    // ================================================================

    #[derive(Debug, Clone)]
    enum StoreOp {
        Add(u64, u64),
        Dismiss(u64),
        ClearWorkspace(u64),
        ClearAll,
    }

    fn arb_store_op() -> impl Strategy<Value = StoreOp> {
        prop_oneof![
            (0..1000u64, 1..5u64).prop_map(|(id, ws)| StoreOp::Add(id, ws)),
            (0..1000u64).prop_map(StoreOp::Dismiss),
            (1..5u64).prop_map(StoreOp::ClearWorkspace),
            Just(StoreOp::ClearAll),
        ]
    }

    proptest! {
        #[test]
        fn proptest_store_invariants(
            ops in proptest::collection::vec(arb_store_op(), 0..200)
        ) {
            let mut store = NotificationStore::new();
            let mut next_id = 0u64;

            for op in &ops {
                match op {
                    StoreOp::Add(_, ws) => {
                        let notif = make_notification(next_id, *ws, "test");
                        store.add(notif);
                        next_id += 1;
                    }
                    StoreOp::Dismiss(id) => {
                        store.dismiss(NotificationId::new(*id));
                    }
                    StoreOp::ClearWorkspace(ws) => {
                        store.clear_workspace(WorkspaceId::new(*ws));
                    }
                    StoreOp::ClearAll => {
                        store.clear_all();
                    }
                }

                // Invariant 1: len never exceeds MAX_NOTIFICATIONS
                prop_assert!(
                    store.len() <= MAX_NOTIFICATIONS,
                    "store.len() = {} exceeds MAX_NOTIFICATIONS = {}",
                    store.len(),
                    MAX_NOTIFICATIONS,
                );

                // Invariant 2: no duplicate IDs
                let all = store.all();
                let ids: std::collections::HashSet<_> = all.iter().map(|n| n.id).collect();
                prop_assert_eq!(
                    ids.len(),
                    all.len(),
                    "duplicate IDs found in store"
                );

                // Invariant 3: unread_count for any workspace <= total len
                for ws_id in 1..=5u64 {
                    let unread = store.unread_count(WorkspaceId::new(ws_id));
                    prop_assert!(
                        unread <= store.len(),
                        "unread_count({}) = {} > store.len() = {}",
                        ws_id, unread, store.len(),
                    );
                }
            }
        }
    }
}
