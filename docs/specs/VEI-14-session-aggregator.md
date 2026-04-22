# VEI-14: Session Aggregator — SQLite Cache + Adapter Trait

## Context

The session aggregator is the subsystem that discovers, indexes, and caches AI agent conversation data for the Conversations tab. It reads session files from agent harnesses (Claude Code, Codex, OpenCode, etc.), extracts metadata, stores it in a SQLite database, and exposes a search/query API for the UI layer.

This task establishes the foundational types, database schema, adapter trait, and session store. It does NOT include the file watcher (VEI-25), AppState push mechanism (VEI-56), or progressive metadata enrichment (VEI-26). Those subsystems build on the foundation created here.

The architecture follows the system design doc: the session aggregator is a background actor that owns a SQLite connection, provides CRUD operations for session metadata, and supports full-text search via FTS5. Adapters are the pluggable boundary where agent-specific file parsing lives.

### Key design decisions

- **Shared types in veil-core**: `SessionId`, `SessionEntry`, `SessionPreview`, `SessionStatus`, and `AgentKind` live in veil-core so other crates (veil-ui, veil-socket) can reference them without depending on veil-aggregator.
- **AgentAdapter trait in veil-aggregator**: The trait and its implementations are internal to the aggregator crate.
- **SQLite via rusqlite**: Direct rusqlite usage (not an ORM). WAL mode for concurrent reads. FTS5 for search.
- **Error handling via thiserror**: All aggregator errors are structured, no silent failures.
- **Title generation**: Heuristic-based, implemented as a pure function on session metadata. Agent-provided titles are preferred; gibberish/UUID titles fall back to first-message extraction.

## Implementation Units

### Unit 1: Core session types (veil-core)

Define the shared types that represent session data throughout the application.

**Types:**

```rust
/// Opaque session identifier — wraps a String (agent-specific ID format).
pub struct SessionId(String);

/// Which agent harness produced this session.
pub enum AgentKind {
    ClaudeCode,
    Codex,
    OpenCode,
    Aider,
    Unknown(String),
}

/// Lifecycle state of a session.
pub enum SessionStatus {
    Active,
    Completed,
    Errored,
    Unknown,
}

/// Metadata record for one agent conversation session.
/// This is the primary data structure stored in SQLite and
/// passed to the UI layer for rendering conversation list entries.
pub struct SessionEntry {
    pub id: SessionId,
    pub agent: AgentKind,
    pub title: String,
    pub working_dir: PathBuf,
    pub branch: Option<String>,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    pub plan_content: Option<String>,
    pub status: SessionStatus,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub indexed_at: DateTime<Utc>,
}

/// Lightweight preview content for a specific session.
/// Loaded lazily when user selects a conversation.
pub struct SessionPreview {
    pub id: SessionId,
    pub first_user_message: Option<String>,
    pub first_assistant_message: Option<String>,
    pub message_count: usize,
    pub tool_call_count: usize,
}

/// Search result entry returned by FTS5 queries.
pub struct SessionSearchResult {
    pub entry: SessionEntry,
    pub relevance: f64,
    pub snippet: Option<String>,
}
```

**Files:**
- `crates/veil-core/src/session.rs` — All session types
- `crates/veil-core/src/lib.rs` — Re-export `pub mod session;`

**Dependencies added to veil-core Cargo.toml:**
- `chrono = { version = "0.4", features = ["serde"] }` (for DateTime<Utc>)

**Tests:**
- `SessionId` construction and equality: IDs with same inner string are equal; different strings are not
- `SessionId` Display implementation shows inner value
- `AgentKind` Display: each variant produces expected human-readable name
- `AgentKind::Unknown` wraps arbitrary string
- `SessionStatus` default is `Unknown`
- `SessionEntry` can be constructed with all fields populated
- `SessionEntry` can be constructed with optional fields as `None`
- `AgentKind` round-trip: convert to string and parse back

### Unit 2: AgentAdapter trait (veil-aggregator)

Define the trait that all agent harness adapters must implement. This is the pluggable boundary for reading session data from different agent tools.

**Types:**

```rust
/// Errors that can occur during adapter operations.
pub enum AdapterError {
    /// Session data directory not found or inaccessible.
    DataDirNotFound(PathBuf),
    /// Failed to parse session file.
    ParseError { path: PathBuf, source: Box<dyn std::error::Error + Send + Sync> },
    /// I/O error reading session data.
    IoError(std::io::Error),
}

/// Trait implemented by each agent harness adapter.
///
/// Adapters discover sessions on the filesystem, parse their metadata,
/// and provide preview content. They are expected to be stateless —
/// the session store (SQLite) owns the persistent state.
pub trait AgentAdapter: Send + Sync {
    /// Human-readable name for this harness (e.g., "Claude Code").
    fn name(&self) -> &str;

    /// Which AgentKind this adapter handles.
    fn agent_kind(&self) -> AgentKind;

    /// Filesystem paths this adapter monitors for session data.
    /// Used by the file watcher (VEI-25) to know what to watch.
    fn watch_paths(&self) -> Vec<PathBuf>;

    /// Discover all sessions this adapter can find.
    /// Returns entries with as much metadata as the adapter can extract.
    /// Must not panic — returns errors per-session via Result in the Vec.
    fn discover_sessions(&self) -> Vec<Result<SessionEntry, AdapterError>>;

    /// Load preview content for a specific session.
    /// Returns None if the session ID is not recognized or data is unavailable.
    fn session_preview(&self, id: &SessionId) -> Result<Option<SessionPreview>, AdapterError>;
}
```

**Files:**
- `crates/veil-aggregator/src/adapter.rs` — Trait definition + AdapterError
- `crates/veil-aggregator/src/lib.rs` — Re-export `pub mod adapter;`

**Tests:**
- Compile-time: trait is object-safe (can be used as `Box<dyn AgentAdapter>`)
- A mock adapter can be constructed and called through the trait interface
- `discover_sessions` returning mixed Ok/Err entries: verify both are accessible
- `session_preview` returning None for unknown ID
- `AdapterError` Display implementations produce meaningful messages
- Adapter is `Send + Sync` (compile-time check via trait bound)

### Unit 3: Title generation (veil-aggregator)

Pure functions for generating meaningful conversation titles from session metadata.

**Functions:**

```rust
/// Generate a display title for a session.
/// Priority: agent-provided title (if not gibberish) > heuristic from first message > fallback.
pub fn generate_title(
    agent_title: Option<&str>,
    first_user_message: Option<&str>,
) -> String;

/// Returns true if the string looks like a UUID, hash, or other non-meaningful identifier.
fn is_gibberish_title(title: &str) -> bool;

/// Extract a topic phrase from the first user message.
/// Truncates to reasonable length, strips common prefixes ("please", "can you", etc.).
fn extract_topic_from_message(message: &str) -> String;
```

**Files:**
- `crates/veil-aggregator/src/title.rs`
- Re-export from `lib.rs`

**Tests:**
- Agent-provided meaningful title is used as-is: `"Fix auth middleware"` -> `"Fix auth middleware"`
- UUID-style title falls through to heuristic: `"a1b2c3d4-e5f6-7890-abcd-ef1234567890"` -> extracted from message
- Hex hash title falls through: `"abc123def456"` -> extracted from message
- Purely numeric title falls through: `"1234567890"` -> extracted from message
- No agent title + has message: extracts topic from first message
- No agent title + no message: returns generic fallback like "Untitled session"
- Long first message is truncated to reasonable length (~80 chars)
- Common prefixes stripped: "Please help me fix the auth bug" -> "fix the auth bug" (or similar)
- Empty string agent title treated as missing
- Whitespace-only agent title treated as missing
- Message with only whitespace falls through to fallback
- Mixed case, punctuation in titles preserved

### Unit 4: SQLite session store (veil-aggregator)

The persistence layer: owns the SQLite connection, manages schema, provides CRUD operations and FTS5 search.

**Schema:**

```sql
CREATE TABLE IF NOT EXISTS sessions (
    id              TEXT PRIMARY KEY,
    agent           TEXT NOT NULL,
    title           TEXT NOT NULL,
    working_dir     TEXT NOT NULL,
    branch          TEXT,
    pr_number       INTEGER,
    pr_url          TEXT,
    plan_content    TEXT,
    status          TEXT NOT NULL DEFAULT 'unknown',
    started_at      TEXT NOT NULL,  -- ISO 8601
    ended_at        TEXT,           -- ISO 8601
    indexed_at      TEXT NOT NULL   -- ISO 8601
);

CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
    title,
    first_message,
    content='',  -- contentless FTS table (data stored separately)
    tokenize='porter unicode61'
);
```

**Types:**

```rust
/// Wraps a rusqlite::Connection with session-specific operations.
pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    /// Open or create the database at the given path.
    /// Runs migrations on first open.
    pub fn open(path: &Path) -> Result<Self, StoreError>;

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, StoreError>;

    /// Insert or update a session entry.
    /// Uses INSERT OR REPLACE — the session ID is the natural key.
    pub fn upsert_session(&self, entry: &SessionEntry) -> Result<(), StoreError>;

    /// Batch upsert multiple sessions in a single transaction.
    pub fn upsert_sessions(&self, entries: &[SessionEntry]) -> Result<usize, StoreError>;

    /// Retrieve a session by ID.
    pub fn get_session(&self, id: &SessionId) -> Result<Option<SessionEntry>, StoreError>;

    /// List all sessions, ordered by started_at descending.
    pub fn list_sessions(&self) -> Result<Vec<SessionEntry>, StoreError>;

    /// List sessions filtered by agent kind.
    pub fn list_sessions_by_agent(&self, agent: &AgentKind) -> Result<Vec<SessionEntry>, StoreError>;

    /// Full-text search across session titles and first messages.
    pub fn search_sessions(&self, query: &str) -> Result<Vec<SessionSearchResult>, StoreError>;

    /// Delete a session by ID.
    pub fn delete_session(&self, id: &SessionId) -> Result<bool, StoreError>;

    /// Update the FTS index for a session (title + first message text).
    pub fn update_fts(&self, id: &SessionId, title: &str, first_message: Option<&str>) -> Result<(), StoreError>;

    /// Count total sessions, optionally filtered by agent.
    pub fn count_sessions(&self, agent: Option<&AgentKind>) -> Result<usize, StoreError>;

    /// Delete all sessions (used for cache rebuild scenarios).
    pub fn clear_all(&self) -> Result<(), StoreError>;
}

pub enum StoreError {
    /// SQLite error.
    Sqlite(rusqlite::Error),
    /// Database file I/O error.
    Io(std::io::Error),
    /// Data conversion/serialization error.
    DataError(String),
    /// Schema migration failed.
    MigrationError(String),
}
```

**Files:**
- `crates/veil-aggregator/src/store.rs` — SessionStore + StoreError
- `crates/veil-aggregator/src/store/migrations.rs` — Schema creation/migration logic (or inline in store.rs if small)

**Dependencies added to veil-aggregator Cargo.toml:**
- `rusqlite = { version = "0.35", features = ["bundled"] }` (bundled for cross-platform, includes FTS5)
- `chrono = { version = "0.4", features = ["serde"] }` (for DateTime conversions)

**Tests (all use `open_in_memory()`):**

*Schema:*
- `open_in_memory` succeeds and creates tables
- Opening twice on same file does not fail (migrations are idempotent)
- `sessions` table has expected columns
- `sessions_fts` virtual table exists

*CRUD:*
- `upsert_session` + `get_session` round-trip: all fields preserved
- `upsert_session` with same ID updates existing record
- `get_session` with nonexistent ID returns None
- `list_sessions` returns entries ordered by `started_at` descending
- `list_sessions` on empty database returns empty Vec
- `list_sessions_by_agent` filters correctly
- `delete_session` with existing ID returns true and removes it
- `delete_session` with nonexistent ID returns false
- `count_sessions` with no filter returns total
- `count_sessions` with agent filter returns filtered count
- `clear_all` removes all sessions

*Batch:*
- `upsert_sessions` inserts multiple entries in one transaction
- `upsert_sessions` with empty slice returns Ok(0)
- `upsert_sessions` with mix of new and existing entries: new ones inserted, existing ones updated
- `upsert_sessions` is atomic: if any entry fails, none are committed

*FTS5 search:*
- `search_sessions` with matching title returns results
- `search_sessions` with matching first_message returns results
- `search_sessions` with no matches returns empty Vec
- `search_sessions` with partial word matches (porter stemmer: "running" matches "run")
- `search_sessions` results ordered by relevance
- `update_fts` + `search_sessions` round-trip
- `search_sessions` with empty query returns empty Vec
- `search_sessions` handles special characters in query without error (SQL injection safety)

*Data integrity:*
- Optional fields (branch, pr_number, pr_url, plan_content, ended_at) store and retrieve None correctly
- DateTime fields round-trip through ISO 8601 text storage without precision loss
- `AgentKind` round-trips through TEXT storage
- `SessionStatus` round-trips through TEXT storage
- `AgentKind::Unknown("custom")` round-trips correctly

*Error handling:*
- `StoreError` Display implementations produce useful messages
- Opening a database at an invalid path returns `StoreError::Io`

### Unit 5: Adapter registry (veil-aggregator)

A container that holds all registered adapters and coordinates discovery across them, with graceful failure isolation per adapter.

**Types:**

```rust
/// Manages a collection of agent adapters and coordinates operations across them.
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn AgentAdapter>>,
}

impl AdapterRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self;

    /// Register an adapter.
    pub fn register(&mut self, adapter: Box<dyn AgentAdapter>);

    /// Discover sessions from all registered adapters.
    /// Failed adapters are skipped with a warning log; others continue.
    /// Returns all successfully discovered sessions.
    pub fn discover_all(&self) -> Vec<SessionEntry>;

    /// Get preview from the appropriate adapter for a given session.
    pub fn session_preview(&self, agent: &AgentKind, id: &SessionId) -> Option<SessionPreview>;

    /// Collect all watch paths from all adapters.
    pub fn all_watch_paths(&self) -> Vec<PathBuf>;

    /// List registered adapter names.
    pub fn adapter_names(&self) -> Vec<&str>;
}
```

**Files:**
- `crates/veil-aggregator/src/registry.rs`

**Tests (using mock adapters):**
- Empty registry: `discover_all` returns empty Vec
- Single adapter: returns that adapter's sessions
- Multiple adapters: returns combined sessions from all
- Adapter that returns errors in `discover_sessions`: errors are logged, valid entries from other adapters still returned
- Adapter that panics (via mock): caught, other adapters still work (stretch goal — may use `catch_unwind`)
- `session_preview` routes to correct adapter by AgentKind
- `session_preview` for unregistered agent kind returns None
- `all_watch_paths` combines paths from all adapters, no duplicates
- `adapter_names` returns names in registration order

## Acceptance Criteria

1. `cargo build -p veil-core` succeeds with session types defined
2. `cargo build -p veil-aggregator` succeeds with all new modules
3. `cargo test -p veil-core` passes all session type tests
4. `cargo test -p veil-aggregator` passes all store, adapter, title, and registry tests
5. `cargo clippy --all-targets --all-features -- -D warnings` passes
6. `cargo fmt --check` passes
7. `SessionStore::open_in_memory()` creates a working database with correct schema
8. FTS5 search returns relevant results with porter stemming
9. `AdapterRegistry` gracefully handles adapter failures without affecting other adapters
10. All shared types (`SessionId`, `SessionEntry`, `SessionPreview`, `AgentKind`, `SessionStatus`, `SessionSearchResult`) are defined in veil-core and usable from other crates
11. Title generation produces meaningful titles and rejects gibberish
12. Batch upsert is transactional
13. No file watcher, no AppState push, no progressive enrichment logic (those are separate issues)

## Dependencies

**New crate dependencies:**

| Crate | Dependency | Version | Features | Reason |
|-------|-----------|---------|----------|--------|
| veil-core | chrono | 0.4 | serde | DateTime types for session timestamps |
| veil-aggregator | rusqlite | 0.35 | bundled | SQLite with FTS5, cross-platform via bundled |
| veil-aggregator | chrono | 0.4 | serde | DateTime conversions for store layer |

Add `chrono` and `rusqlite` to `[workspace.dependencies]` in root Cargo.toml so versions are pinned centrally.

**No new tools or external software required.** rusqlite's `bundled` feature compiles SQLite from source, so no system SQLite is needed.
