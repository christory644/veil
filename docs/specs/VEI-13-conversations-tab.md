# VEI-13: Navigation Pane -- Conversations Tab (Agent Space)

## Context

The navigation pane sidebar has two tabs: Workspaces (userspace) and Conversations (agent space). VEI-12 delivered the Workspaces tab with workspace list rendering, click-to-switch, and the tab header bar. The Conversations tab currently shows a "Coming soon" placeholder. This task replaces that placeholder with the core conversations tab -- grouped agent session history with entry rendering, tab switching, and click-to-select.

The Conversations tab is a P0 feature (PRD item 3) and the primary differentiator between Veil and other terminal multiplexers. It presents all AI agent session history grouped by harness (Claude Code, Codex, etc.), sorted by most recent activity, with meaningful metadata per entry.

### What already exists

- **`veil-core::session::SessionEntry`** -- Full session metadata: `id`, `agent` (AgentKind), `title`, `working_dir`, `branch`, `pr_number`, `status` (SessionStatus: Active/Completed/Errored/Unknown), `started_at`, `ended_at`, `plan_content`, `indexed_at`.
- **`veil-core::session::AgentKind`** -- Enum: `ClaudeCode`, `Codex`, `OpenCode`, `Aider`, `Unknown(String)`. Has `Display` impl.
- **`veil-core::session::SessionStatus`** -- Enum: `Active`, `Completed`, `Errored`, `Unknown`. Has `Display` impl.
- **`veil-core::state::AppState`** -- Has `conversations: ConversationIndex` with `sessions: Vec<SessionEntry>`. Method: `update_conversations(sessions)` replaces the session list.
- **`veil-core::state::SidebarTab`** -- Enum with `Workspaces` and `Conversations` variants.
- **`veil-core::message::StateUpdate::ConversationsUpdated`** -- Message variant for pushing session data from the aggregator actor to AppState.
- **`veil-ui::sidebar::render_sidebar`** -- Already dispatches on `SidebarTab`. The `Conversations` match arm currently renders `ui.label("Coming soon")`.
- **`veil-ui::sidebar::SidebarResponse`** -- Has `switch_to_workspace` and `switch_tab` fields. Needs extension for conversation interactions.
- **`veil-ui::workspace_list`** -- Established pattern: view-model struct (`WorkspaceEntryData`), extraction function (`extract_workspace_entries`), rendering function (`render_workspaces_tab`), helper (`abbreviate_path`).
- **`veil-aggregator::store::SessionStore`** -- SQLite store with `list_sessions()`, `list_sessions_by_agent()`. Sessions ordered by `started_at` DESC.

### What this task delivers

1. Conversations tab content: agent harness group headers (collapsible) with session count, and per-conversation entry rendering (active indicator, title, branch, relative timestamp).
2. Tab switching between Workspaces and Conversations (already works via the existing tab bar -- this task adds content to render when Conversations is active).
3. Click interaction to select a conversation, reported back via `SidebarResponse`.
4. View-model extraction from `AppState` to egui rendering, following the pattern established by `workspace_list.rs`.
5. Relative timestamp formatting ("2h ago", "yesterday", "3 days ago").

### What is explicitly out of scope (tracked as follow-up issues)

- **VEI-48**: [+] button to start new agent sessions
- **VEI-47**: Conversation details panel for historical sessions
- **VEI-46**: Lazy loading and scroll pagination
- **VEI-45**: Live state awareness (branch deleted, PR merged/closed badges, directory gone)
- Search/filter (/ to focus, FTS5) -- separate follow-up
- Keyboard navigation within conversation list (j/k) -- separate follow-up
- PR number with live status badge rendering -- VEI-45
- Plan indicator icon -- VEI-47 (details panel)

### Key design decisions

**Follow the workspace_list.rs pattern.** The conversations tab uses the same architecture as the workspaces tab: a view-model struct (`ConversationEntryData`), an extraction function that transforms `AppState` into view data, and a rendering function that takes the view data and returns click events. This keeps rendering pure and testable.

**Group-then-entry hierarchy.** Sessions are grouped by `AgentKind` for rendering. Each group has a collapsible header showing the agent name and session count. Within each group, sessions are sorted by `started_at` descending (most recent first). The grouping is computed during view-model extraction, not stored in AppState.

**Collapse state lives in egui.** Collapsible group headers use `egui::CollapsingHeader`, which stores its open/closed state in egui's per-frame memory. This means collapse state persists across frames (egui handles this internally) but does not persist across app restarts. This is acceptable -- groups default to expanded.

**Relative timestamps are computed at render time.** The view-model extraction function computes relative timestamps by diffing `started_at` against `Utc::now()`. This is a pure function on `DateTime<Utc>` and is independently testable without egui.

**SidebarResponse is extended with conversation selection.** A new field `selected_conversation` carries the `SessionId` of a clicked conversation entry. The caller (the `veil` binary) decides what to do with it -- for active sessions, navigate to the workspace; for historical, show details (VEI-47). For now, the binary just logs the selection; the actual navigation is wired in follow-up tasks.

**SessionId needs Clone.** `SessionId` already derives `Clone`, so it can be included in `SidebarResponse` directly.

## Implementation Units

### Unit 1: Relative Timestamp Formatting (`veil-ui` crate)

A pure function that converts a `DateTime<Utc>` into a human-readable relative string. This has no egui dependency and is independently testable.

**File:** `crates/veil-ui/src/time_fmt.rs`

**Functions:**

```rust
/// Format a timestamp as a relative string ("just now", "5m ago", "2h ago",
/// "yesterday", "3 days ago", "2 weeks ago", "Jan 15").
///
/// Uses `now` as the reference time to enable deterministic testing.
pub fn format_relative(timestamp: DateTime<Utc>, now: DateTime<Utc>) -> String
```

**Formatting rules:**

| Duration since timestamp | Output |
|---|---|
| < 60 seconds | "just now" |
| 1-59 minutes | "{n}m ago" |
| 1-23 hours | "{n}h ago" |
| 1 day (24-47h) | "yesterday" |
| 2-13 days | "{n} days ago" |
| 14-59 days | "{n} weeks ago" |
| >= 60 days | "Mon DD" (e.g. "Jan 15") or "Mon DD, YYYY" if different year |

When `timestamp` is in the future relative to `now`, return "just now" (defensive -- clock skew or test artifact).

**Test strategy:**

Happy path:
- Timestamp 30 seconds ago -> "just now"
- Timestamp 5 minutes ago -> "5m ago"
- Timestamp 3 hours ago -> "3h ago"
- Timestamp 26 hours ago -> "yesterday"
- Timestamp 5 days ago -> "5 days ago"
- Timestamp 3 weeks ago -> "3 weeks ago"
- Timestamp 90 days ago (same year) -> "Jan 22" (or appropriate date)
- Timestamp from previous year -> "Dec 15, 2025"

Edge cases:
- Exactly 60 seconds -> "1m ago" (boundary)
- Exactly 24 hours -> "yesterday" (boundary)
- Exactly 48 hours -> "2 days ago" (boundary)
- Exactly 14 days -> "2 weeks ago" (boundary)
- Future timestamp -> "just now"
- Same timestamp (now == timestamp) -> "just now"

### Unit 2: Conversation View-Model and Extraction (`veil-ui` crate)

The view-model types and the function that transforms `AppState.conversations.sessions` into grouped, sorted, renderable data. No egui dependency -- this is pure data transformation.

**File:** `crates/veil-ui/src/conversation_list.rs`

**Types:**

```rust
/// Data for rendering a single conversation entry.
/// View-model extracted from SessionEntry to keep rendering decoupled.
pub struct ConversationEntryData {
    /// Session identifier (for click reporting).
    pub id: SessionId,
    /// Display title.
    pub title: String,
    /// Whether this session is currently active (running).
    pub is_active: bool,
    /// Git branch, if known.
    pub branch: Option<String>,
    /// Relative timestamp string (e.g., "2h ago").
    pub relative_time: String,
    /// Whether a finalized plan exists for this session.
    pub has_plan: bool,
}

/// A group of conversations for one agent harness.
pub struct ConversationGroup {
    /// Agent harness display name (e.g., "Claude Code").
    pub agent_name: String,
    /// Agent kind (for identification).
    pub agent_kind: AgentKind,
    /// Total session count for this agent (used in group header).
    pub session_count: usize,
    /// Conversation entries, sorted by most recent first.
    pub entries: Vec<ConversationEntryData>,
}
```

**Functions:**

```rust
/// Extract grouped conversation data from AppState.
///
/// Groups sessions by AgentKind, sorts each group by started_at descending
/// (most recent first), and formats relative timestamps using `now`.
/// Groups themselves are sorted by the most recent session in each group.
///
/// `now` is passed explicitly for deterministic testing.
pub fn extract_conversation_groups(
    state: &AppState,
    now: DateTime<Utc>,
) -> Vec<ConversationGroup>

/// Convert a single SessionEntry into a ConversationEntryData.
///
/// `now` is used for relative timestamp formatting.
fn session_to_entry_data(
    session: &SessionEntry,
    now: DateTime<Utc>,
) -> ConversationEntryData
```

**Extraction logic:**

1. Collect all sessions from `state.conversations.sessions`.
2. Group by `session.agent` (using `AgentKind` as the grouping key).
3. Within each group, sort by `started_at` descending (most recent first).
4. Sort groups by the `started_at` of their most recent entry (groups with newer sessions appear first).
5. Convert each `SessionEntry` to `ConversationEntryData`, computing relative timestamps and `has_plan` from `plan_content.is_some()`.
6. Map `SessionStatus::Active` to `is_active = true`, all others to `false`.

**Test strategy:**

Happy path:
- Empty sessions list -> empty groups vec.
- 3 sessions from one agent -> one group with 3 entries.
- Sessions from 2 agents -> 2 groups, each with correct entries.
- Sessions within a group are sorted by started_at descending.
- Groups are sorted by most recent session.
- Active session has `is_active = true`.
- Completed session has `is_active = false`.
- Session with branch shows branch.
- Session without branch has `branch = None`.
- Session with plan_content has `has_plan = true`.
- Session without plan_content has `has_plan = false`.
- Relative time string is formatted correctly (delegates to format_relative).

Edge cases:
- Single session -> one group with one entry.
- All sessions from same agent -> one group.
- Sessions from `AgentKind::Unknown("Custom")` -> group named "Custom".
- Multiple sessions with identical timestamps -> stable order (no panic).
- Session with `SessionStatus::Errored` -> `is_active = false`.
- Session with `SessionStatus::Unknown` -> `is_active = false`.

### Unit 3: Conversation List Rendering (`veil-ui` crate)

Render the grouped conversation entries inside the Conversations tab scroll area using egui. Follows the same pattern as `render_workspaces_tab` but with group headers and different entry layout.

**File:** `crates/veil-ui/src/conversation_list.rs` (same file as Unit 2)

**Functions:**

```rust
/// Render the conversations tab content.
///
/// Displays collapsible agent group headers with session counts, and
/// per-conversation entry rows within each group. Returns the SessionId
/// of a conversation the user clicked, if any.
pub fn render_conversations_tab(
    ui: &mut egui::Ui,
    groups: &[ConversationGroup],
) -> Option<SessionId>

/// Render a single conversation entry row.
///
/// Returns true if the user clicked this entry.
fn render_conversation_entry(
    ui: &mut egui::Ui,
    entry: &ConversationEntryData,
) -> bool
```

**Group header layout:**

```
▼ Claude Code (5)
```

- Uses `egui::CollapsingHeader` with the agent name and session count.
- Default state: expanded (collapsed only if user clicks the header).
- The header text uses the agent's `Display` name.

**Entry layout (per conversation):**

```
┌────────────────────────┐
│ ● "Fix auth            │   <- active indicator + title
│   middleware"           │
│   feat/auth    2h ago  │   <- branch + relative time (on same line)
└────────────────────────┘
```

- **Active indicator:** `●` (green/bold) for active sessions (`is_active == true`), `○` (dimmed) for historical.
- **Title:** Primary label. Quoted in the wireframe for clarity but rendered without quotes.
- **Branch:** Dimmed text on the second line. Omitted if `branch` is `None`.
- **Relative time:** Dimmed text, right-aligned on the branch line. If no branch, time appears alone on the second line.
- **Plan indicator:** If `has_plan` is true, show a small icon/text indicator (e.g., a document emoji or "[plan]" text) next to the title. Keep it minimal -- no custom icons in MVP.
- **Hover state:** egui's built-in response highlighting.
- **Click interaction:** Entire entry is clickable. Click returns the entry's `SessionId`.

**Test strategy:**

Happy path:
- Render 2 groups with entries: verify content is rendered (height > 0).
- Render empty groups list: no panic, nothing rendered.
- Single group with 3 entries produces visible content.
- Click on entry returns `Some(session_id)`.
- No click returns `None`.
- Group with 0 entries: header renders but no entry content.

Edge cases:
- Very long title: no panic, text wraps or truncates.
- Very long branch name: no panic.
- Empty title string: renders without panic.
- Many groups (10+): no panic, scrollable.
- Group with many entries (50+): no panic, scrollable within the collapsing header.

Rendering verification:
- Rendering 3 entries is taller than rendering 1 entry.
- Entry with branch is taller than entry without branch (extra line).
- Entry with `has_plan = true` contains plan indicator text.

### Unit 4: Sidebar Integration and SidebarResponse Extension (`veil-ui` crate)

Wire the conversation list rendering into the existing sidebar and extend `SidebarResponse` to report conversation clicks.

**File:** `crates/veil-ui/src/sidebar.rs` (modification)

**Changes to `SidebarResponse`:**

```rust
#[derive(Debug, Default)]
pub struct SidebarResponse {
    /// User clicked a workspace to switch to it.
    pub switch_to_workspace: Option<WorkspaceId>,
    /// User clicked a tab to switch to it.
    pub switch_tab: Option<SidebarTab>,
    /// User clicked a conversation entry.
    pub selected_conversation: Option<SessionId>,
}
```

**Changes to `render_sidebar`:**

Replace the `SidebarTab::Conversations` arm:

```rust
SidebarTab::Conversations => {
    let groups = crate::conversation_list::extract_conversation_groups(
        state,
        chrono::Utc::now(),
    );
    if let Some(session_id) = crate::conversation_list::render_conversations_tab(
        ui, &groups,
    ) {
        response.selected_conversation = Some(session_id);
    }
}
```

**Changes to `veil-ui/src/lib.rs`:**

Add `pub mod conversation_list;` and `pub mod time_fmt;`.

**Changes to `veil-ui/Cargo.toml`:**

Add `chrono.workspace = true` dependency (needed for `DateTime<Utc>` in the extraction function and `format_relative`).

**Test strategy:**

Happy path:
- Render sidebar with Conversations tab active and sessions in AppState: verify no panic and content is rendered.
- Render sidebar with Conversations tab active and empty sessions: no panic, renders empty.
- SidebarResponse correctly carries `selected_conversation` when a conversation is clicked.
- Switching tab to Conversations and back to Workspaces: both tabs render their content.

Edge cases:
- SidebarResponse with all fields None (no interaction).
- Conversations tab with sessions but sidebar width 0: no panic.

### Unit 5: Event Wiring in Binary Crate (`veil` binary crate)

Wire the `SidebarResponse::selected_conversation` field into the main event loop. For this task, the handler logs the selection and does nothing else -- the actual navigation (VEI-47/VEI-48) comes later.

**File:** `crates/veil/src/main.rs` (modification)

**Changes:**

After the existing `switch_to_workspace` and `switch_tab` handling, add:

```rust
if let Some(session_id) = response.selected_conversation {
    tracing::info!(session_id = %session_id, "conversation selected");
    // TODO(VEI-47): navigate to workspace for active, show details for historical
}
```

This is intentionally minimal. The actual behavior (navigate to workspace for active sessions, show details panel for historical) is wired in VEI-47.

**Test strategy:**

This unit is primarily integration-level (needs the window/renderer) and is verified by:
- Compile test: `cargo build -p veil` succeeds.
- The `SidebarResponse` field is consumed (no unused-field warning from clippy).
- Existing tests continue to pass (no regressions).

No new unit tests are needed for this unit -- the logging is trivial and the actual navigation logic is deferred.

## Acceptance Criteria

1. `cargo build` succeeds for all workspace crates.
2. `cargo test` passes all tests (existing and new).
3. `cargo clippy --all-targets --all-features -- -D warnings` passes.
4. `cargo fmt --check` passes.
5. When the Conversations tab is active and `AppState.conversations.sessions` contains session data, the sidebar renders grouped conversation entries instead of "Coming soon".
6. Sessions are grouped by agent harness with collapsible group headers showing agent name and session count.
7. Within each group, sessions are sorted by most recent first.
8. Each conversation entry shows: active indicator (filled/hollow bullet), title, branch (if available), and relative timestamp.
9. Clicking a conversation entry populates `SidebarResponse::selected_conversation` with the session's ID.
10. Group headers are collapsible (click to expand/collapse).
11. Empty session list renders without panic (no content, no "Coming soon").
12. Relative timestamps are formatted correctly ("just now", "5m ago", "2h ago", "yesterday", etc.).
13. Sessions with `SessionStatus::Active` show a filled bullet; all others show a hollow bullet.
14. Sessions with `plan_content` present show a plan indicator.

## Dependencies

### New crate dependencies

| Crate | Version | Purpose |
|---|---|---|
| `chrono` | workspace (existing) | DateTime operations for relative timestamp formatting in `veil-ui` |

`chrono` is already a workspace dependency (used by `veil-core`). `veil-ui` needs it added to its own `Cargo.toml`.

No new external crates are required. All rendering uses egui (already a dependency of `veil-ui`). All session types come from `veil-core` (already a dependency of `veil-ui`).

### New files

| File | Purpose |
|---|---|
| `crates/veil-ui/src/time_fmt.rs` | Relative timestamp formatting function |
| `crates/veil-ui/src/conversation_list.rs` | Conversation view-model types, extraction, and egui rendering |

### Modified files

| File | Changes |
|---|---|
| `crates/veil-ui/Cargo.toml` | Add `chrono.workspace = true` |
| `crates/veil-ui/src/lib.rs` | Add `pub mod conversation_list;` and `pub mod time_fmt;` |
| `crates/veil-ui/src/sidebar.rs` | Extend `SidebarResponse` with `selected_conversation`, replace "Coming soon" with conversation list rendering |
| `crates/veil/src/main.rs` | Handle `SidebarResponse::selected_conversation` (log + TODO comment) |
