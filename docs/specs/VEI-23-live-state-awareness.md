# VEI-23: Live State Awareness -- Git/PR Status Polling for Conversation Metadata

## Context

Conversation metadata in the Conversations tab is historical: branch names and PR numbers are captured at the time the agent session occurs. But branches get deleted after merge, PRs get merged or closed, and working directories get moved. If Veil displays stale metadata without any indication, users will see ghost branches and unknown PR states, eroding trust in the navigation pane.

This task implements the **live state cross-referencing** layer described in the system design document ("Live State Awareness" section). It checks whether cached branch names still exist, whether PRs have been merged/closed, and whether working directories still exist on disk. The results are cached in SQLite with a configurable TTL (30-60s) to avoid hammering git and the GitHub API.

### What this task covers

- `LiveState` enum and `LiveStatus` types representing the current state of branches, PRs, and directories (in `veil-core`)
- `GitChecker` -- a struct that shells out to `git` via `std::process::Command` to check branch existence, batching multiple branches per repo
- `PrChecker` -- a struct that shells out to `gh` to check PR state, with rate-limiting awareness and aggressive caching
- `DirChecker` -- a trivial `Path::exists()` check for working directories
- `LiveStateCache` -- SQLite table and query methods in `veil-aggregator` for caching check results with TTL-based expiration
- `LiveStateResolver` -- the coordinator that, given a list of `SessionEntry` records, resolves their live state by consulting cache first and checking only stale/missing entries
- Integration of `LiveState` into `ConversationEntryData` in `veil-ui` so the UI can render dimmed branches, PR badges, and directory warnings

### What is out of scope (deferred)

- The background polling actor/loop that periodically triggers resolution (VEI-56 or similar)
- Wiring into the app event loop or `StateUpdate` messages
- Actual egui rendering changes for dimmed text, colored badges, warning icons (VEI-24 or similar)
- GitHub API token management or OAuth flows
- Checking branches for workspaces (the Workspaces tab) -- this task is Conversations tab only

### Why now

The Conversations tab (VEI-13) and Claude Code adapter (VEI-15) are implemented. Sessions have branch and PR metadata. Without live state awareness, deleted branches and merged PRs display identically to active ones, making the metadata misleading rather than helpful. This is a prerequisite for the progressive metadata enrichment story to feel trustworthy.

## Implementation Units

### Unit 1: Live state types (`veil-core/src/live_state.rs`)

Define the core types that represent live state for branches, PRs, and directories. These live in `veil-core` because they're consumed by both `veil-aggregator` (cache) and `veil-ui` (rendering).

**Types:**

```rust
// crates/veil-core/src/live_state.rs

use std::fmt;
use std::path::PathBuf;

/// Live state of a git branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchState {
    /// Branch exists in the repository.
    Exists,
    /// Branch has been deleted from the repository.
    Deleted,
    /// Could not determine branch state (git unavailable, repo missing, etc.).
    Unknown,
}

/// Live state of a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrState {
    /// PR is open and accepting reviews/changes.
    Open,
    /// PR has been merged.
    Merged,
    /// PR was closed without merging.
    Closed,
    /// Could not determine PR state (gh unavailable, rate limited, etc.).
    Unknown,
}

/// Live state of a working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirState {
    /// Directory exists on disk.
    Exists,
    /// Directory no longer exists.
    Missing,
}

/// Aggregated live state for a single conversation/session.
///
/// Each field is `Option` because a session may not have a branch,
/// PR, or working directory to check.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveStatus {
    /// Live state of the associated branch, if any.
    pub branch: Option<BranchState>,
    /// Live state of the associated PR, if any.
    pub pr: Option<PrState>,
    /// Live state of the working directory.
    pub dir: Option<DirState>,
}
```

Also add `Display` impls for `BranchState` and `PrState` (useful for badge labels and logging), and register the module in `veil-core/src/lib.rs`.

**Files:**
- New: `crates/veil-core/src/live_state.rs`
- Modified: `crates/veil-core/src/lib.rs` (add `pub mod live_state;`)

### Unit 2: Git branch checker (`veil-core/src/git_checker.rs`)

A struct that checks whether branches exist in git repositories by shelling out to `git`. Batches multiple branch checks for the same repo into a single `git branch --list` call.

**Design:**

```rust
// crates/veil-core/src/git_checker.rs

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::live_state::BranchState;

/// Checks git branch existence by shelling out to `git`.
pub struct GitChecker;

/// A request to check one branch in one repo.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchCheckRequest {
    /// Path to the git repository (working directory).
    pub repo_path: PathBuf,
    /// Branch name to check.
    pub branch_name: String,
}

/// Result of checking one branch.
#[derive(Debug, Clone)]
pub struct BranchCheckResult {
    /// The original request.
    pub request: BranchCheckRequest,
    /// The determined state.
    pub state: BranchState,
}

impl GitChecker {
    /// Check a single branch in a single repo.
    ///
    /// Runs `git -C <repo_path> branch --list <branch_name>` and checks
    /// whether the output is non-empty.
    pub fn check_branch(repo_path: &Path, branch_name: &str) -> BranchState { ... }

    /// Check multiple branches, batching by repo.
    ///
    /// For each unique repo_path, runs a single `git -C <repo_path> branch --list`
    /// (with all branch names for that repo) and parses the output.
    /// Returns results in the same order as the input requests.
    pub fn check_branches(requests: &[BranchCheckRequest]) -> Vec<BranchCheckResult> { ... }

    /// Check whether `git` is available on the system PATH.
    pub fn is_available() -> bool { ... }
}
```

**Key behaviors:**
- `check_branch` runs `git -C <repo_path> branch --list <branch_name>` and returns `Exists` if stdout is non-empty, `Deleted` if empty, `Unknown` if the command fails.
- `check_branches` groups requests by `repo_path`, runs one `git branch --list name1 name2 name3` per repo, parses the output lines (stripping leading `*` and whitespace), and maps each requested branch to `Exists` or `Deleted`.
- If `git` is not found or the directory is not a git repo, returns `Unknown` for those entries.
- Timeout: each `git` invocation gets a 5-second timeout to avoid hanging on network-mounted repos.

**Files:**
- New: `crates/veil-core/src/git_checker.rs`
- Modified: `crates/veil-core/src/lib.rs` (add `pub mod git_checker;`)

### Unit 3: PR state checker (`veil-core/src/pr_checker.rs`)

A struct that checks PR state by shelling out to `gh`. Includes rate-limiting awareness.

**Design:**

```rust
// crates/veil-core/src/pr_checker.rs

use std::path::Path;
use std::process::Command;

use crate::live_state::PrState;

/// Checks PR state by shelling out to `gh`.
pub struct PrChecker;

/// A request to check one PR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrCheckRequest {
    /// Path to the git repository (used for gh context).
    pub repo_path: std::path::PathBuf,
    /// PR number to check.
    pub pr_number: u64,
}

/// Result of checking one PR.
#[derive(Debug, Clone)]
pub struct PrCheckResult {
    /// The original request.
    pub request: PrCheckRequest,
    /// The determined state.
    pub state: PrState,
}

impl PrChecker {
    /// Check a single PR's state.
    ///
    /// Runs `gh pr view <number> --json state` in the repo directory
    /// and parses the JSON output.
    pub fn check_pr(repo_path: &Path, pr_number: u64) -> PrState { ... }

    /// Check multiple PRs. Does NOT batch (each PR is a separate gh call)
    /// but respects a simple rate limit: if any call returns a rate-limit
    /// error, remaining checks return `Unknown`.
    pub fn check_prs(requests: &[PrCheckRequest]) -> Vec<PrCheckResult> { ... }

    /// Check whether `gh` is available and authenticated.
    pub fn is_available() -> bool { ... }
}
```

**Key behaviors:**
- `check_pr` runs `gh pr view <number> --json state -R <owner/repo>` (or relies on git remote context via the `repo_path` working directory). Parses `{"state":"OPEN"|"MERGED"|"CLOSED"}`.
- If `gh` is not installed or not authenticated, returns `Unknown`.
- If the response contains rate-limit headers indicating exhaustion, `check_prs` short-circuits remaining requests and returns `Unknown` for all of them.
- Timeout: each `gh` invocation gets a 10-second timeout (network call).
- The JSON parsing uses `serde_json` (already a dependency of `veil-aggregator`; will be added to `veil-core` dev-dependencies or used inline). Since `gh pr view --json state` returns minimal JSON, a lightweight manual parse (find `"state":"..."`) is acceptable to avoid adding `serde_json` as a runtime dependency to `veil-core`. Decision: use a small manual parse to keep `veil-core` dependency-light.

**Files:**
- New: `crates/veil-core/src/pr_checker.rs`
- Modified: `crates/veil-core/src/lib.rs` (add `pub mod pr_checker;`)

### Unit 4: Directory checker (`veil-core/src/dir_checker.rs`)

Trivial module -- checks `Path::exists()`. Separated for consistency and testability.

```rust
// crates/veil-core/src/dir_checker.rs

use std::path::Path;
use crate::live_state::DirState;

/// Checks whether a working directory still exists on disk.
pub struct DirChecker;

impl DirChecker {
    /// Check if a directory exists.
    pub fn check(path: &Path) -> DirState {
        if path.exists() { DirState::Exists } else { DirState::Missing }
    }
}
```

**Files:**
- New: `crates/veil-core/src/dir_checker.rs`
- Modified: `crates/veil-core/src/lib.rs` (add `pub mod dir_checker;`)

### Unit 5: Live state cache (`veil-aggregator/src/live_state_cache.rs`)

SQLite table and operations for caching live state check results with TTL-based expiration. Extends the existing `SessionStore` or lives alongside it using the same database connection.

**Schema:**

```sql
CREATE TABLE IF NOT EXISTS live_state_cache (
    -- Composite key: what we checked
    check_type  TEXT NOT NULL,    -- 'branch', 'pr', 'dir'
    check_key   TEXT NOT NULL,    -- e.g., '/repo/path::branch_name', '/repo/path::42', '/working/dir'
    -- Result
    state       TEXT NOT NULL,    -- 'exists', 'deleted', 'open', 'merged', 'closed', 'missing', 'unknown'
    -- TTL
    checked_at  TEXT NOT NULL,    -- RFC 3339 timestamp
    PRIMARY KEY (check_type, check_key)
);
```

**Design:**

```rust
// crates/veil-aggregator/src/live_state_cache.rs

use chrono::{DateTime, Duration, Utc};
use veil_core::live_state::{BranchState, DirState, PrState};

use crate::store::StoreError;

/// Cache key types for live state checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckType {
    Branch,
    Pr,
    Dir,
}

/// A cached check result.
#[derive(Debug, Clone)]
pub struct CachedCheck {
    pub check_type: CheckType,
    pub check_key: String,
    pub state: String,
    pub checked_at: DateTime<Utc>,
}

/// Operations for the live_state_cache table.
///
/// Takes a reference to a `rusqlite::Connection` (shared with SessionStore).
pub struct LiveStateCache<'a> {
    conn: &'a rusqlite::Connection,
}

impl<'a> LiveStateCache<'a> {
    /// Create a new cache handle over an existing connection.
    pub fn new(conn: &'a rusqlite::Connection) -> Self { ... }

    /// Run migrations (CREATE TABLE IF NOT EXISTS).
    pub fn run_migrations(&self) -> Result<(), StoreError> { ... }

    /// Look up a cached result. Returns None if not found or if the
    /// entry is older than `max_age`.
    pub fn get(
        &self,
        check_type: CheckType,
        key: &str,
        max_age: Duration,
    ) -> Result<Option<CachedCheck>, StoreError> { ... }

    /// Store a check result (upsert).
    pub fn put(
        &self,
        check_type: CheckType,
        key: &str,
        state: &str,
        checked_at: DateTime<Utc>,
    ) -> Result<(), StoreError> { ... }

    /// Remove expired entries older than `max_age`.
    pub fn evict_expired(&self, max_age: Duration) -> Result<usize, StoreError> { ... }

    /// Clear all cached state (for testing or full refresh).
    pub fn clear_all(&self) -> Result<(), StoreError> { ... }
}
```

**Key format conventions:**
- Branch: `"{repo_path}::{branch_name}"`
- PR: `"{repo_path}::{pr_number}"`
- Dir: `"{dir_path}"`

To share the SQLite connection with `SessionStore`, the `SessionStore` will expose a `conn()` accessor (or `LiveStateCache` will be constructed with a reference to the raw connection). This avoids opening a second database file.

**Files:**
- New: `crates/veil-aggregator/src/live_state_cache.rs`
- Modified: `crates/veil-aggregator/src/lib.rs` (add `pub mod live_state_cache;`)
- Modified: `crates/veil-aggregator/src/store.rs` (add `pub fn conn(&self) -> &rusqlite::Connection` accessor, and call `LiveStateCache::run_migrations` from `SessionStore::run_migrations`)

### Unit 6: Live state resolver (`veil-aggregator/src/live_state_resolver.rs`)

The coordinator that resolves live state for a set of sessions. Consults the cache first, then checks only stale/missing entries using the checkers from Units 2-4.

**Design:**

```rust
// crates/veil-aggregator/src/live_state_resolver.rs

use std::collections::HashMap;

use chrono::Duration;
use veil_core::live_state::LiveStatus;
use veil_core::session::SessionId;

use crate::live_state_cache::LiveStateCache;

/// Configuration for the resolver.
#[derive(Debug, Clone)]
pub struct ResolverConfig {
    /// Maximum age of a cached branch check before re-checking.
    pub branch_ttl: Duration,
    /// Maximum age of a cached PR check before re-checking.
    pub pr_ttl: Duration,
    /// Maximum age of a cached directory check before re-checking.
    pub dir_ttl: Duration,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            branch_ttl: Duration::seconds(60),
            pr_ttl: Duration::seconds(120),  // PRs change less frequently; cache longer
            dir_ttl: Duration::seconds(300), // Directories rarely disappear; cache longest
        }
    }
}

/// Input to the resolver: one session's metadata that might need live checks.
#[derive(Debug, Clone)]
pub struct SessionCheckInput {
    pub session_id: SessionId,
    pub repo_path: Option<std::path::PathBuf>,
    pub branch_name: Option<String>,
    pub pr_number: Option<u64>,
    pub working_dir: std::path::PathBuf,
}

/// Resolves live state for sessions by checking cache, then shelling out
/// for stale/missing entries.
pub struct LiveStateResolver<'a> {
    cache: LiveStateCache<'a>,
    config: ResolverConfig,
}

impl<'a> LiveStateResolver<'a> {
    pub fn new(cache: LiveStateCache<'a>, config: ResolverConfig) -> Self { ... }

    /// Resolve live state for a batch of sessions.
    ///
    /// Returns a map from SessionId to LiveStatus. Sessions that have no
    /// branch/PR/dir metadata will have an empty (default) LiveStatus.
    ///
    /// Algorithm:
    /// 1. For each session, build cache keys for branch/PR/dir.
    /// 2. Look up each key in the cache. If fresh, use cached value.
    /// 3. Collect stale/missing keys into check batches.
    /// 4. Run GitChecker::check_branches for all stale branch checks.
    /// 5. Run PrChecker::check_prs for all stale PR checks.
    /// 6. Run DirChecker::check for all stale dir checks.
    /// 7. Write fresh results back to cache.
    /// 8. Assemble and return the LiveStatus map.
    pub fn resolve(
        &self,
        inputs: &[SessionCheckInput],
    ) -> Result<HashMap<SessionId, LiveStatus>, crate::store::StoreError> { ... }
}
```

**Graceful degradation:**
- If `GitChecker::is_available()` returns false, skip all branch checks and return `BranchState::Unknown` for everything.
- If `PrChecker::is_available()` returns false, skip all PR checks and return `PrState::Unknown`.
- Directory checks always work (just `Path::exists()`).
- All checker failures result in `Unknown` state, never panics or errors that propagate.

**Files:**
- New: `crates/veil-aggregator/src/live_state_resolver.rs`
- Modified: `crates/veil-aggregator/src/lib.rs` (add `pub mod live_state_resolver;`)

### Unit 7: UI integration -- LiveStatus in ConversationEntryData (`veil-ui`)

Extend `ConversationEntryData` to include live state information so the rendering layer can display badges, dimmed text, and warnings.

**Changes to `veil-ui/src/conversation_list.rs`:**

```rust
// Add to ConversationEntryData:
pub struct ConversationEntryData {
    // ... existing fields ...

    /// Live state of the associated branch, if any.
    pub branch_state: Option<BranchState>,
    /// Live state of the associated PR, if any.
    pub pr_state: Option<PrState>,
    /// Live state of the working directory.
    pub dir_state: Option<DirState>,
}
```

**Changes to extraction logic:**

`extract_conversation_groups` will accept an optional `&HashMap<SessionId, LiveStatus>` parameter. If provided, each session's live state is looked up and populated into the entry data. If not provided (or if a session has no live state entry), the fields remain `None`.

The rendering function (`render_conversation_entry`) will be updated in a future task (VEI-24) to use these fields for visual display. For now, the fields are present but unused in rendering -- this unit ensures the data pipeline is complete.

**Files:**
- Modified: `crates/veil-ui/src/conversation_list.rs` (add fields to `ConversationEntryData`, update `extract_conversation_groups` signature and logic)

## Test Strategy Per Unit

### Unit 1: Live state types

- **Happy path:** Construct each variant of `BranchState`, `PrState`, `DirState`, `LiveStatus`; verify equality and Display output.
- **Default:** `LiveStatus::default()` has all `None` fields.
- **Edge:** `LiveStatus` with mixed Some/None fields.

### Unit 2: Git branch checker

- **Happy path (integration):** Create a temp git repo, create branches, verify `check_branch` returns `Exists`. Delete a branch, verify `Deleted`.
- **Batch:** Multiple branches in same repo resolved with single `check_branches` call; verify correct mapping.
- **Cross-repo batch:** Branches from different repos batched correctly.
- **Error cases:** Non-existent directory returns `Unknown`. Non-git directory returns `Unknown`. Invalid branch name returns `Unknown` (not panic).
- **`is_available`:** Verify returns `true` in test environment (git is expected).
- **Timeout:** Verify that a hung git command doesn't block indefinitely (use a mock or skip in unit tests, cover in integration).

### Unit 3: PR state checker

- **Happy path (unit):** Mock `gh` output parsing -- `{"state":"OPEN"}` maps to `PrState::Open`, `{"state":"MERGED"}` maps to `Merged`, `{"state":"CLOSED"}` maps to `Closed`.
- **Parse logic:** Test the JSON state extraction function directly with valid/invalid input.
- **Error cases:** `gh` not found returns `Unknown`. Malformed JSON returns `Unknown`. Rate-limit response causes remaining checks to short-circuit.
- **`is_available`:** Test independently.
- **Note:** Full integration tests (actual GitHub API calls) are not run in CI. Unit tests cover the parsing and error handling with canned command output.

### Unit 4: Directory checker

- **Happy path:** Existing directory returns `Exists`. Non-existent path returns `Missing`.
- **Edge:** Empty string path returns `Missing`. Symlink to existing dir returns `Exists`. File (not dir) path returns `Exists` (Path::exists returns true for files too -- document this behavior).

### Unit 5: Live state cache

- **Schema:** Migrations create the table. Idempotent (running twice doesn't error).
- **CRUD:** `put` then `get` round-trips correctly. `get` with max_age filters expired entries. `put` twice overwrites (upsert).
- **TTL:** Entry at exactly max_age boundary. Entry 1 second past max_age.
- **Eviction:** `evict_expired` removes old entries and preserves fresh ones.
- **Clear:** `clear_all` empties the table.
- **Empty:** `get` on empty table returns None.

### Unit 6: Live state resolver

- **Happy path:** Sessions with branch+PR+dir all get resolved. Cached values are used when fresh. Stale values trigger re-checks.
- **Cache miss:** First call with no cached data triggers all checks; second call within TTL uses cache.
- **Graceful degradation:** If git is unavailable, branch states are `Unknown` but no error propagates. Same for gh.
- **Mixed sessions:** Some sessions have branch metadata, some don't. Only those with metadata are checked.
- **Empty input:** Empty session list returns empty map.
- **Batch efficiency:** Multiple sessions referencing the same repo/branch result in a single git call (verify via check count, not timing).

### Unit 7: UI integration

- **Happy path:** `extract_conversation_groups` with live state map populates branch_state/pr_state/dir_state on entries.
- **No live state:** When live state map is None or session not in map, fields are None.
- **Existing tests pass:** All existing conversation_list tests continue to pass with the new optional parameter.
- **Edge:** Session has branch metadata but no live state entry for it (field is None, not Unknown).

## Acceptance Criteria

1. `BranchState`, `PrState`, `DirState`, and `LiveStatus` types exist in `veil-core` and are fully tested.
2. `GitChecker::check_branch` returns correct state for existing/deleted branches in a real git repo (integration test with tempdir).
3. `GitChecker::check_branches` batches checks by repo path.
4. `PrChecker::check_pr` correctly parses all three PR states from `gh` JSON output.
5. `PrChecker` gracefully degrades when `gh` is unavailable (returns `Unknown`, no panic).
6. `DirChecker::check` returns `Exists`/`Missing` correctly.
7. `LiveStateCache` stores and retrieves results with TTL expiration in SQLite.
8. `LiveStateCache` migrations are called from `SessionStore::run_migrations` so the table exists in the shared database.
9. `LiveStateResolver` coordinates cache lookups and checker invocations, writing fresh results back to cache.
10. `LiveStateResolver` gracefully degrades when git/gh are unavailable.
11. `ConversationEntryData` includes `branch_state`, `pr_state`, `dir_state` fields.
12. `extract_conversation_groups` accepts an optional live state map and populates the new fields.
13. All existing tests continue to pass.
14. `cargo clippy --all-targets --all-features -- -D warnings` passes.
15. `cargo fmt --check` passes.

## Dependencies

### Existing (already in workspace)

- `rusqlite` -- SQLite access (veil-aggregator)
- `chrono` -- timestamps and Duration for TTL (veil-core, veil-aggregator)
- `serde_json` -- PR state JSON parsing (only in veil-aggregator or kept to manual parse in veil-core)
- `tracing` -- logging for checker operations
- `tempfile` -- temp directories for git integration tests (dev-dependency)

### New dependencies: None

All required functionality is available via `std::process::Command` (git/gh shelling), `std::path::Path::exists` (directory checks), existing `rusqlite` (cache), and existing `chrono` (TTL). No new crate dependencies are needed.

### Tool requirements (runtime, not build-time)

- `git` must be on `PATH` for branch checks to work (graceful fallback if missing)
- `gh` (GitHub CLI) must be on `PATH` and authenticated for PR checks to work (graceful fallback if missing)
