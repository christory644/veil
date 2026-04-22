# VEI-18: Notification System — Core Infrastructure

## Context

Veil needs a notification system so that AI agents and external tools can surface alerts to the user. Notifications appear as badge counts on workspace entries in the sidebar, and the latest notification text is shown as a subtitle on each workspace entry.

There are two notification sources:

1. **OSC escape sequences** (OSC 9, OSC 99, OSC 777) — terminal-standard sequences that agents can emit naturally through PTY output. These are the primary mechanism for agent-generated alerts.
2. **Socket API** (`notification.create` method) — programmatic creation via the JSON-RPC socket, used by extensions and external tools.

### What this task covers

VEI-18 focuses on the **core notification infrastructure** that the rest of the system builds on:

- **Notification data model** with source tracking, workspace/pane association, read/unread state, and timestamps
- **Notification store** with lifecycle operations (create, read, dismiss, clear) and query methods
- **OSC sequence parser** that extracts notification payloads from PTY output byte streams
- **AppState and StateUpdate integration** — wiring the new types into the existing message channel architecture

### What is out of scope (covered by follow-up issues)

- **VEI-58**: OSC parsing integration with actual libghosty terminal callback registration
- **VEI-59**: Desktop notifications (macOS UserNotifications, Linux D-Bus, Windows Toast)
- **VEI-60**: Pane border visual highlight for notifications
- **VEI-61**: Per-workspace notification mute
- **VEI-50**: Socket API `notification.create`/`list`/`clear` methods

### Why now

The existing `NotificationEntry` in `state.rs` is minimal — it has an ID, workspace, message, timestamp, and acknowledged flag. It lacks source tracking (OSC vs. socket vs. internal), pane association, notification severity/urgency, and a proper store with lifecycle operations. The existing `add_notification` and `acknowledge_notification` methods on `AppState` are thin wrappers with no notification limits, no dismissal, and no filtering. The OSC parsing does not exist at all.

Building the core data model, store, and parser now unblocks all follow-up work (desktop notifications, pane highlights, socket API methods, muting) without coupling to unfinished systems like the full libghosty callback pipeline.

## Implementation Units

### Unit 1: Notification data model (`veil-core/src/notification.rs`)

Define the enriched notification types that replace the current `NotificationEntry` in `state.rs`.

**Types:**

```rust
// crates/veil-core/src/notification.rs

/// Where the notification originated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationSource {
    /// OSC escape sequence from terminal output.
    Osc { sequence_type: OscSequenceType },
    /// Socket API `notification.create` call.
    SocketApi,
    /// Internal Veil event (e.g., process exit, error).
    Internal,
}

/// Which OSC sequence produced the notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscSequenceType {
    /// OSC 9 — iTerm2/ConEmu notification.
    Osc9,
    /// OSC 99 — kitty notification protocol.
    Osc99,
    /// OSC 777 — rxvt-unicode notification.
    Osc777,
}

/// Unique notification identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NotificationId(u64);

impl NotificationId {
    pub fn new(id: u64) -> Self { Self(id) }
    pub fn as_u64(self) -> u64 { self.0 }
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
```

**What changes in existing code:**

- The existing `NotificationEntry` struct in `state.rs` will be replaced by `Notification` from the new module. The `AppState.notifications` field type changes from `Vec<NotificationEntry>` to `NotificationStore` (see Unit 2).
- The existing `add_notification` and `acknowledge_notification` methods on `AppState` will delegate to `NotificationStore` methods.
- All existing test code referencing `NotificationEntry` will be updated to use `Notification`.

**Test strategy:**

| Test | Type | What it verifies |
|------|------|-----------------|
| `notification_id_equality` | Unit | Two `NotificationId` values with same inner are equal |
| `notification_id_inequality` | Unit | Different inner values are not equal |
| `notification_source_display` | Unit | Each `NotificationSource` variant has a human-readable display |
| `osc_sequence_type_variants` | Unit | All three OSC types are distinct |
| `notification_default_unread` | Unit | Newly constructed `Notification` has `read: false` |
| `notification_with_title` | Unit | `title` field can hold a value and `None` |
| `notification_with_surface_id` | Unit | `surface_id` field can hold a value and `None` |

### Unit 2: Notification store (`veil-core/src/notification.rs`)

A dedicated store that manages the notification lifecycle and provides query methods. This replaces the raw `Vec<NotificationEntry>` in `AppState`.

**Types and methods:**

```rust
/// Maximum number of notifications retained in the store.
/// Oldest notifications are evicted when this limit is exceeded.
const MAX_NOTIFICATIONS: usize = 500;

/// Manages notification lifecycle: create, read, dismiss, clear, query.
#[derive(Debug, Clone)]
pub struct NotificationStore {
    notifications: Vec<Notification>,
}

impl NotificationStore {
    pub fn new() -> Self;

    /// Add a notification to the store. If the store exceeds
    /// MAX_NOTIFICATIONS, the oldest notification is evicted.
    /// Returns the ID of the new notification.
    pub fn add(&mut self, notification: Notification) -> NotificationId;

    /// Mark a notification as read.
    pub fn mark_read(&mut self, id: NotificationId) -> bool;

    /// Mark all notifications for a workspace as read.
    pub fn mark_all_read(&mut self, workspace_id: WorkspaceId);

    /// Remove a single notification (dismiss).
    pub fn dismiss(&mut self, id: NotificationId) -> bool;

    /// Remove all notifications for a workspace.
    pub fn clear_workspace(&mut self, workspace_id: WorkspaceId);

    /// Remove all notifications.
    pub fn clear_all(&mut self);

    /// Count unread notifications for a workspace.
    pub fn unread_count(&self, workspace_id: WorkspaceId) -> usize;

    /// Get the most recent notification for a workspace (for subtitle display).
    pub fn latest_for_workspace(&self, workspace_id: WorkspaceId) -> Option<&Notification>;

    /// Get all notifications for a workspace, most recent first.
    pub fn for_workspace(&self, workspace_id: WorkspaceId) -> Vec<&Notification>;

    /// Get all notifications, most recent first.
    pub fn all(&self) -> &[Notification];

    /// Total count of notifications in the store.
    pub fn len(&self) -> usize;

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool;
}
```

**Test strategy:**

| Test | Type | What it verifies |
|------|------|-----------------|
| `new_store_is_empty` | Unit | Fresh store has len 0 |
| `add_notification_increases_len` | Unit | Adding a notification increments len |
| `add_returns_correct_id` | Unit | Returned ID matches the notification's ID |
| `mark_read_sets_read_flag` | Unit | After `mark_read`, notification has `read: true` |
| `mark_read_nonexistent_returns_false` | Unit | Returns false for unknown ID |
| `mark_all_read_for_workspace` | Unit | All notifications for a workspace become read |
| `mark_all_read_does_not_affect_other_workspaces` | Unit | Notifications from other workspaces stay unread |
| `dismiss_removes_notification` | Unit | Notification is gone after dismiss |
| `dismiss_nonexistent_returns_false` | Unit | Returns false for unknown ID |
| `clear_workspace_removes_all_for_workspace` | Unit | All workspace notifications gone, others remain |
| `clear_all_empties_store` | Unit | Store is empty after clear_all |
| `unread_count_counts_only_unread` | Unit | Read notifications not counted |
| `unread_count_counts_only_target_workspace` | Unit | Other workspace notifications not counted |
| `latest_for_workspace_returns_most_recent` | Unit | Most recently created notification is returned |
| `latest_for_workspace_returns_none_when_empty` | Unit | Returns None when workspace has no notifications |
| `for_workspace_returns_most_recent_first` | Unit | Ordering is by created_at descending |
| `for_workspace_filters_to_workspace` | Unit | Only target workspace notifications returned |
| `eviction_at_max_capacity` | Unit | Adding beyond MAX_NOTIFICATIONS evicts oldest |
| `eviction_preserves_newest` | Unit | After eviction, most recent notifications survive |
| `proptest_store_invariants` | Property | After random add/dismiss/clear sequences: len <= MAX_NOTIFICATIONS, no duplicate IDs, unread_count <= len |

### Unit 3: OSC notification parser (`veil-core/src/osc_parse.rs`)

Parse OSC 9, OSC 99, and OSC 777 escape sequences from raw byte streams. This is a pure parser with no FFI dependency -- it operates on byte slices.

**Background on OSC notification sequences:**

- **OSC 9** (iTerm2/ConEmu): `\x1b]9;<message>\x07` or `\x1b]9;<message>\x1b\\` — simple text notification
- **OSC 99** (kitty): `\x1b]99;i=<id>;<payload>\x07` or with `\x1b\\` terminator — supports title (`t=0` or `t=1`), body (`b=0` or `b=1`), urgency (`u=<n>`), identifier for replacement
- **OSC 777** (rxvt-unicode): `\x1b]777;notify;<title>;<body>\x07` or with `\x1b\\` terminator

The parser does NOT need to handle the general OSC parsing problem (libghosty already does that). Instead, it receives the OSC payload string (the content between `\x1b]` and the string terminator) and extracts notification data.

**Types:**

```rust
// crates/veil-core/src/osc_parse.rs

/// Parsed notification from an OSC sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OscNotification {
    /// Which OSC sequence type produced this.
    pub sequence_type: OscSequenceType,
    /// Notification title (if the sequence supports it).
    pub title: Option<String>,
    /// Notification body/message.
    pub body: String,
}

/// Errors from OSC notification parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OscParseError {
    #[error("not a notification OSC sequence")]
    NotNotification,
    #[error("malformed OSC payload: {reason}")]
    Malformed { reason: String },
    #[error("empty notification body")]
    EmptyBody,
}

/// Try to parse an OSC payload string as a notification.
///
/// The `payload` is the content between `\x1b]` and the string terminator
/// (`\x07` or `\x1b\\`). For example, for the sequence `\x1b]9;hello\x07`,
/// the payload is `"9;hello"`.
///
/// Returns `Err(OscParseError::NotNotification)` if the payload is not an
/// OSC 9/99/777 notification sequence. This is the expected "no match" case,
/// not a true error.
pub fn parse_osc_notification(payload: &str) -> Result<OscNotification, OscParseError>;
```

**Implementation notes:**

- The parser matches on the OSC number prefix: `"9;"`, `"99;"`, `"777;"`.
- OSC 9: everything after `"9;"` is the message body.
- OSC 99: Parse key-value parameters (`i=`, `p=`, `d=`) and payload data. For the initial implementation, support the `p=body` or `p=title` parameter to distinguish title vs. body payloads. The kitty notification protocol uses `d=0` (start) and `d=1` (end) for multi-part payloads; the initial implementation will handle single-part (`d=0` or absent `d`) only.
- OSC 777: Split on `;` — format is `777;notify;<title>;<body>`. The `notify` keyword is required.
- Invalid/unrecognized sequences return `OscParseError::NotNotification`.
- Empty body after parsing returns `OscParseError::EmptyBody`.

**Test strategy:**

| Test | Type | What it verifies |
|------|------|-----------------|
| **OSC 9** | | |
| `osc9_simple_message` | Unit | `"9;hello world"` produces body `"hello world"` |
| `osc9_empty_message` | Unit | `"9;"` returns `EmptyBody` |
| `osc9_special_characters` | Unit | Body with unicode, newlines, semicolons is preserved |
| **OSC 99** | | |
| `osc99_body_only` | Unit | `"99;i=1;hello"` produces body `"hello"` |
| `osc99_with_title_payload` | Unit | `"99;i=1:p=title;My Title"` produces title |
| `osc99_with_body_payload` | Unit | `"99;i=1:p=body;The body text"` produces body |
| `osc99_empty_payload` | Unit | `"99;i=1;"` returns `EmptyBody` |
| `osc99_missing_id` | Unit | `"99;hello"` (no `i=`) still works (id is optional for one-shot) |
| **OSC 777** | | |
| `osc777_with_title_and_body` | Unit | `"777;notify;Title;Body"` produces title + body |
| `osc777_body_only` | Unit | `"777;notify;;Body"` produces body with no title |
| `osc777_missing_notify_keyword` | Unit | `"777;other;Title;Body"` returns `NotNotification` |
| `osc777_empty_body` | Unit | `"777;notify;Title;"` returns `EmptyBody` |
| **Non-notification** | | |
| `non_notification_osc` | Unit | `"0;window title"` (OSC 0) returns `NotNotification` |
| `osc7_pwd` | Unit | `"7;file://..."` returns `NotNotification` |
| `empty_payload` | Unit | `""` returns `NotNotification` |
| `garbage_payload` | Unit | Random bytes return `NotNotification` or `Malformed` |
| **Property-based** | | |
| `proptest_osc9_roundtrip` | Property | Any non-empty string s: `parse("9;" + s)` produces body s |
| `proptest_no_panic_on_arbitrary_input` | Property | Arbitrary byte strings never panic |

### Unit 4: AppState + StateUpdate integration

Wire the new notification infrastructure into the existing `AppState` and `StateUpdate` message channel system.

**Changes to `state.rs`:**

1. Replace `pub notifications: Vec<NotificationEntry>` with `pub notifications: NotificationStore`.
2. Remove the `NotificationEntry` struct (replaced by `Notification` from the new module).
3. Update `add_notification` to construct a `Notification` with `NotificationSource::Internal` by default, or accept a source parameter. Add an `add_osc_notification` method for the OSC path.
4. Update `acknowledge_notification` to delegate to `NotificationStore::mark_read`.
5. Add delegation methods: `dismiss_notification`, `clear_workspace_notifications`, `clear_all_notifications`, `unread_notification_count`, `latest_notification`.

**Changes to `message.rs`:**

Update the `StateUpdate::NotificationReceived` variant to carry richer data:

```rust
StateUpdate::NotificationReceived {
    workspace_id: WorkspaceId,
    surface_id: Option<SurfaceId>,
    message: String,
    title: Option<String>,
    source: NotificationSource,
}
```

**Changes to `workspace_list.rs`:**

Update `extract_workspace_entries` to use `NotificationStore::unread_count` instead of manually filtering `Vec<NotificationEntry>`. Add a `latest_notification` field to `WorkspaceEntryData` for subtitle display.

```rust
pub struct WorkspaceEntryData<'a> {
    // ... existing fields ...
    pub notification_count: usize,
    /// Latest notification message for subtitle display.
    pub latest_notification: Option<&'a str>,
}
```

**Changes to `lib.rs`:**

Add `pub mod notification;` and `pub mod osc_parse;` to the module declarations.

**Test strategy:**

| Test | Type | What it verifies |
|------|------|-----------------|
| `add_notification_uses_store` | Unit | `AppState::add_notification` creates entry in `NotificationStore` |
| `acknowledge_delegates_to_mark_read` | Unit | `acknowledge_notification` calls `mark_read` on store |
| `dismiss_notification_removes_entry` | Unit | Notification is removed from store |
| `clear_workspace_notifications` | Unit | All notifications for workspace cleared |
| `clear_all_notifications` | Unit | All notifications cleared |
| `unread_count_delegates_to_store` | Unit | Returns correct unread count |
| `latest_notification_for_workspace` | Unit | Returns most recent notification |
| `state_update_with_osc_source` | Async unit | `StateUpdate::NotificationReceived` with OSC source round-trips through channel |
| `state_update_with_socket_source` | Async unit | Same for SocketApi source |
| `extract_workspace_entries_uses_store_unread_count` | Unit | Notification count comes from store |
| `extract_workspace_entries_includes_latest_notification` | Unit | Latest notification message populated |
| `existing_notification_tests_still_pass` | Unit | Verify backward compatibility of updated tests |

## Acceptance Criteria

1. **Notification data model** — `Notification`, `NotificationId`, `NotificationSource`, `OscSequenceType` types exist in `veil-core/src/notification.rs` with full doc comments.

2. **Notification store** — `NotificationStore` provides create, mark_read, mark_all_read, dismiss, clear_workspace, clear_all, unread_count, latest_for_workspace, for_workspace, all, len, is_empty. Enforces a MAX_NOTIFICATIONS eviction policy.

3. **OSC parser** — `parse_osc_notification` in `veil-core/src/osc_parse.rs` correctly parses OSC 9, OSC 99 (single-part), and OSC 777 notification sequences. Returns typed errors for non-notification sequences, malformed payloads, and empty bodies.

4. **AppState integration** — `AppState.notifications` field is a `NotificationStore`. Existing `add_notification` and `acknowledge_notification` methods still work. New delegation methods added for dismiss, clear, query.

5. **StateUpdate integration** — `StateUpdate::NotificationReceived` carries source, title, and surface_id fields. Existing channel tests updated.

6. **Sidebar view-model** — `WorkspaceEntryData` includes `latest_notification` field. `extract_workspace_entries` uses `NotificationStore` methods.

7. **All quality gates pass** — `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo build` all pass.

8. **No regressions** — All existing tests in `state.rs`, `message.rs`, `workspace_list.rs`, and `sidebar.rs` still pass (updated to use new types).

## Dependencies

### Existing (already in Cargo.toml)

- `thiserror` — error types for `OscParseError`
- `chrono` — timestamps on `Notification`
- `tracing` — structured logging
- `proptest` (dev) — property-based tests for OSC parser and store invariants
- `tokio` (dev, with `sync` + `macros` + `rt`) — async channel tests

### No new dependencies needed

All required functionality is covered by existing crate dependencies. The OSC parser is pure string/byte processing. The notification store is a `Vec` with lifecycle methods. No platform-specific code is needed for this task (desktop notifications are VEI-59).
