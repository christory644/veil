# VEI-15: Claude Code Adapter — Session Discovery and Parsing

## Context

This is the first concrete agent adapter for the session aggregator (VEI-14). It implements `AgentAdapter` for Claude Code, enabling Veil's Conversations tab to discover, parse, and display Claude Code session history.

Claude Code stores conversation data in `~/.claude/projects/` as JSONL files. Each project gets a directory named with an encoded version of the filesystem path (e.g., `-Users-christopherstory-repos-veil` for `/Users/christopherstory/repos/veil`). Inside each project directory, session files are named `<session-uuid>.jsonl` and contain newline-delimited JSON records representing conversation turns.

### Why this matters

The Claude Code adapter is the most important adapter because Claude Code is the primary target user's agent harness. Getting the JSONL parsing right — and making it resilient to format changes — sets the pattern for all future adapters.

### Scope boundaries

**In scope (VEI-15):**
- `ClaudeCodeAdapter` implementing `AgentAdapter` trait
- Session discovery: scanning `~/.claude/projects/` for JSONL files
- JSONL parsing: extracting messages, tool calls, timestamps
- Title extraction from session data
- Working directory extraction from JSONL `cwd` field
- Watch paths for file watcher integration
- Unit tests with sample JSONL fixtures

**Out of scope (separate issues):**
- Tool call metadata extraction — branch detection, PR extraction, plan association (VEI-27)
- Property-based tests for JSONL parsing edge cases (VEI-28)
- File watcher integration (VEI-25)
- AppState push mechanism (VEI-56)

## Claude Code JSONL Format (Observed)

Examined actual session data at `~/.claude/projects/` on this machine. Key findings:

### Directory structure

```
~/.claude/
  projects/
    <project-hash>/                      # One per project, path encoded as dash-separated
      <session-uuid>.jsonl               # Main session conversation log
      <session-uuid>/
        subagents/
          agent-<agent-id>.jsonl         # Subagent conversation logs
  history.jsonl                          # User-side input history (display, timestamp, project, sessionId)
```

### Project hash encoding

Directory names encode the project path by replacing `/` with `-`. Example:
- `/Users/christopherstory/repos/veil` -> `-Users-christopherstory-repos-veil`
- `/Users/christopherstory/repos/core-data-access-layer` -> `-Users-christopherstory-repos-core-data-access-layer`

**Important:** This encoding is lossy — hyphens in directory names become indistinguishable from path separators. The actual working directory MUST be read from the `cwd` field inside the JSONL, not decoded from the directory name.

### Message types (top-level `type` field)

| Type | Purpose | Key fields |
|------|---------|------------|
| `queue-operation` | Message queue bookkeeping | `operation` (enqueue/dequeue), `content`, `sessionId` |
| `user` | User message (text or tool results) | `message.role`, `message.content`, `cwd`, `gitBranch`, `version`, `uuid`, `parentUuid` |
| `assistant` | Assistant response (text, thinking, tool_use) | `message.role`, `message.content` (array of blocks), `message.model`, `message.usage` |
| `system` | System events (hooks, compaction boundaries) | `subtype` (stop_hook_summary, compact_boundary, local_command) |
| `progress` | Hook progress indicators | `data.type`, `data.hookEvent` |
| `last-prompt` | Internal bookkeeping | `lastPrompt` |
| `pr-link` | PR association event | `prNumber`, `prUrl`, `prRepository` |
| `file-history-snapshot` | File state snapshot | (internal bookkeeping) |

### User message structure

```json
{
  "parentUuid": null,
  "isSidechain": false,
  "promptId": "...",
  "type": "user",
  "message": {
    "role": "user",
    "content": "the user's message text"
  },
  "uuid": "267215e5-...",
  "timestamp": "2026-03-24T04:52:44.890Z",
  "permissionMode": "default",
  "userType": "external",
  "entrypoint": "claude-desktop",
  "cwd": "/Users/christopherstory/repos/second_brain",
  "sessionId": "4c2b273b-...",
  "version": "2.1.78",
  "gitBranch": "main"
}
```

User messages may also carry tool results:

```json
{
  "type": "user",
  "message": {
    "role": "user",
    "content": [
      {
        "tool_use_id": "toolu_011onhCNo8n2Xcmsh999dTrG",
        "type": "tool_result",
        "content": "output text...",
        "is_error": false
      }
    ]
  },
  "toolUseResult": { "stdout": "..." }
}
```

### Assistant message structure

```json
{
  "parentUuid": "...",
  "isSidechain": false,
  "message": {
    "model": "claude-opus-4-6",
    "id": "msg_...",
    "type": "message",
    "role": "assistant",
    "content": [
      { "type": "thinking", "thinking": "...", "signature": "..." },
      { "type": "text", "text": "The assistant's response..." },
      { "type": "tool_use", "id": "toolu_...", "name": "Bash", "input": { "command": "..." } }
    ],
    "stop_reason": "end_turn",
    "usage": { "input_tokens": 3, "output_tokens": 679, ... }
  },
  "type": "assistant",
  "uuid": "...",
  "timestamp": "2026-03-24T04:53:20.907Z",
  "slug": "majestic-stargazing-planet",
  "cwd": "...",
  "sessionId": "...",
  "version": "2.1.78",
  "gitBranch": "main"
}
```

### Content block types in `message.content` (array)

| Block type | Fields | Notes |
|------------|--------|-------|
| `text` | `text` | Assistant's text response |
| `thinking` | `thinking`, `signature` | Extended thinking content |
| `tool_use` | `id`, `name`, `input` | Tool invocation (Bash, Read, Edit, Write, Grep, etc.) |
| `tool_result` | `tool_use_id`, `content`, `is_error` | In user messages — tool execution result |

### Special fields

- **`slug`**: Auto-generated human-readable name (e.g., "majestic-stargazing-planet"). Not meaningful as a title — treat as gibberish.
- **`isSidechain`**: Boolean, true for subagent/branched conversations.
- **`isCompactSummary`**: Boolean, marks context-compaction summary messages (not real user messages).
- **`isVisibleInTranscriptOnly`**: Boolean, marks synthetic messages not part of original conversation.
- **`entrypoint`**: Where the session was launched from (e.g., "claude-desktop").
- **`version`**: Claude Code version string (e.g., "2.1.87").
- **`gitBranch`**: Git branch at message time.
- **`cwd`**: Working directory at message time.
- **`pr-link` type**: Standalone record with `prNumber`, `prUrl`, `prRepository`.

### Subagent JSONL files

Located at `<session-uuid>/subagents/agent-<id>.jsonl`. Same message format as main session but include an `agentId` field. These are child conversations spawned by the main session.

For VEI-15, we discover and count subagent files but do NOT parse them as separate sessions. They belong to their parent session.

## Implementation Units

### Unit 1: JSONL message types (deserialization structs)

Define the serde structs for deserializing Claude Code JSONL records. These are internal to the adapter and not exposed in the public API.

**File:** `crates/veil-aggregator/src/claude_code/jsonl.rs`

**Types:**

```rust
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// A single JSONL record from a Claude Code session file.
/// Uses an untagged-like approach: we deserialize common fields first,
/// then dispatch on the `type` field.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum JournalRecord {
    #[serde(rename = "user")]
    User(UserRecord),
    #[serde(rename = "assistant")]
    Assistant(AssistantRecord),
    #[serde(rename = "system")]
    System(SystemRecord),
    #[serde(rename = "queue-operation")]
    QueueOperation(QueueOperationRecord),
    #[serde(rename = "pr-link")]
    PrLink(PrLinkRecord),
    #[serde(rename = "progress")]
    Progress(ProgressRecord),
    #[serde(rename = "last-prompt")]
    LastPrompt(LastPromptRecord),
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot(FileHistorySnapshotRecord),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRecord {
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub version: Option<String>,
    pub message: Option<MessagePayload>,
    #[serde(default)]
    pub is_sidechain: bool,
    #[serde(default)]
    pub is_compact_summary: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRecord {
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub version: Option<String>,
    pub slug: Option<String>,
    pub message: Option<AssistantMessagePayload>,
    #[serde(default)]
    pub is_sidechain: bool,
}

#[derive(Debug, Deserialize)]
pub struct MessagePayload {
    pub role: Option<String>,
    pub content: MessageContent,
}

/// Message content is either a plain string or an array of content blocks.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Deserialize)]
pub struct AssistantMessagePayload {
    pub role: Option<String>,
    pub model: Option<String>,
    pub content: Option<Vec<ContentBlock>>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: Option<String>,
        name: Option<String>,
        input: Option<serde_json::Value>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: Option<String>,
        content: Option<serde_json::Value>,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemRecord {
    pub timestamp: Option<DateTime<Utc>>,
    pub session_id: Option<String>,
    pub subtype: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueOperationRecord {
    pub operation: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub session_id: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrLinkRecord {
    pub session_id: Option<String>,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    pub pr_repository: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressRecord {
    pub timestamp: Option<DateTime<Utc>>,
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastPromptRecord {
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistorySnapshotRecord {
    pub session_id: Option<String>,
}
```

**Design rationale:**
- All fields are `Option<T>` (except where serde defaults apply) because the format has evolved across versions and not all fields are present in all records. This makes the parser resilient to missing fields.
- The `JournalRecord` enum uses `#[serde(tag = "type")]` for internal tagging, matching the JSONL format where `"type": "user"` determines the variant.
- `ContentBlock` also uses internal tagging on the `"type"` field.
- `serde_json::Value` is used for tool inputs and tool result content because these have unbounded structure — we don't need to fully parse them for VEI-15.

**Tests:**

- Deserialize a user message with text content -> `UserRecord` with correct fields
- Deserialize a user message with tool_result array content -> `UserRecord` with `Blocks` content
- Deserialize an assistant message with text block -> `AssistantRecord` with text content block
- Deserialize an assistant message with tool_use block -> `AssistantRecord` with tool_use content block
- Deserialize an assistant message with thinking block -> `AssistantRecord` with thinking block
- Deserialize a system message -> `SystemRecord`
- Deserialize a queue-operation message -> `QueueOperationRecord`
- Deserialize a pr-link message -> `PrLinkRecord` with prNumber, prUrl, prRepository
- Deserialize a progress message -> `ProgressRecord`
- Deserialize a last-prompt message -> `LastPromptRecord`
- Unknown type field -> serde error (not panic)
- Missing optional fields -> successful deserialization with None values
- Malformed JSON line -> serde error (not panic)
- Empty string line -> serde error (not panic)
- Line with extra unknown fields -> successful deserialization (serde ignores unknown fields by default, but we should add `#[serde(deny_unknown_fields)]` ONLY where safe, or rely on the default lenient behavior for forward compatibility)

### Unit 2: JSONL file parser

Reads a session JSONL file line by line and produces a parsed session summary. Handles I/O errors and malformed lines gracefully — a single bad line should not prevent parsing the rest of the file.

**File:** `crates/veil-aggregator/src/claude_code/parser.rs`

**Types and functions:**

```rust
use std::path::Path;
use crate::adapter::AdapterError;
use super::jsonl::{JournalRecord, ContentBlock, MessageContent};

/// Summary of a parsed Claude Code session JSONL file.
/// Contains the extracted metadata needed to build a SessionEntry.
#[derive(Debug)]
pub struct ParsedSession {
    /// Session UUID (from sessionId field in records).
    pub session_id: String,
    /// Working directory (from first cwd field encountered).
    pub cwd: Option<String>,
    /// Git branch (from first gitBranch field encountered).
    pub git_branch: Option<String>,
    /// Claude Code version string.
    pub version: Option<String>,
    /// Slug (auto-generated name — treated as gibberish for title purposes).
    pub slug: Option<String>,
    /// Timestamp of the first record in the session.
    pub started_at: Option<DateTime<Utc>>,
    /// Timestamp of the last record in the session.
    pub ended_at: Option<DateTime<Utc>>,
    /// First user message text (excluding compact summaries and tool results).
    pub first_user_message: Option<String>,
    /// First assistant text response (excluding thinking blocks and tool calls).
    pub first_assistant_message: Option<String>,
    /// Total count of user messages (with role="user" and text content).
    pub user_message_count: usize,
    /// Total count of assistant messages.
    pub assistant_message_count: usize,
    /// Total count of tool_use blocks across all assistant messages.
    pub tool_use_count: usize,
    /// Number of lines that failed to parse.
    pub parse_error_count: usize,
}

/// Parse a single JSONL file into a ParsedSession.
///
/// Reads line by line, accumulating metadata. Malformed lines are counted
/// in `parse_error_count` but do not prevent parsing the rest of the file.
pub fn parse_session_file(path: &Path) -> Result<ParsedSession, AdapterError>;

/// Parse a single JSONL line into a JournalRecord.
/// Returns None for lines that fail to parse (with a tracing::debug log).
pub fn parse_line(line: &str) -> Option<JournalRecord>;

/// Extract the first plain-text user message from a UserRecord,
/// skipping compact summaries and tool_result messages.
fn extract_user_text(record: &UserRecord) -> Option<&str>;

/// Extract the first text block from an assistant message's content array.
fn extract_assistant_text(content: &[ContentBlock]) -> Option<&str>;

/// Count tool_use blocks in an assistant message's content array.
fn count_tool_uses(content: &[ContentBlock]) -> usize;
```

**Design rationale:**
- The parser does a single pass through the file, accumulating metadata. It does not load the entire file into memory — session files can be large (12K+ lines observed).
- `BufReader` with line-by-line iteration for memory efficiency.
- Parse errors on individual lines are logged at `debug` level and counted, not propagated. This ensures resilience against format evolution — new record types or field changes don't break existing parsing.
- The session_id is extracted from the filename (UUID part of `<uuid>.jsonl`), but also verified against `sessionId` fields in the JSONL records.

**Tests:**

- Parse a well-formed JSONL file with mixed message types -> correct counts, timestamps, first messages
- Parse a file where first user message is a compact summary -> skip it, use second real user message
- Parse a file with tool_result user messages interleaved -> only count text user messages for first_user_message
- Parse a file with only queue-operation and system records (no user/assistant) -> ParsedSession with None for messages, zero counts
- Parse a file with a single malformed line among valid lines -> parse_error_count is 1, other records parsed correctly
- Parse a completely empty file -> ParsedSession with all None/zero values
- Parse a file with only one user message and one assistant response -> correct first messages
- Extract user text from string content -> Some(text)
- Extract user text from tool_result array content -> None (not a text message)
- Extract user text from compact summary -> None
- Extract assistant text from content with text block -> Some(text)
- Extract assistant text from content with only tool_use blocks -> None
- Extract assistant text from content with thinking + text -> text (not thinking)
- Count tool uses with zero tool_use blocks -> 0
- Count tool uses with multiple tool_use blocks -> correct count
- Timestamps: started_at is earliest, ended_at is latest

### Unit 3: Session discovery

Scans `~/.claude/projects/` to find all session JSONL files and determine which project directory they belong to.

**File:** `crates/veil-aggregator/src/claude_code/discovery.rs`

**Types and functions:**

```rust
use std::path::{Path, PathBuf};

/// A discovered session file on disk, not yet parsed.
#[derive(Debug)]
pub struct DiscoveredSession {
    /// Path to the session JSONL file.
    pub jsonl_path: PathBuf,
    /// Session UUID extracted from the filename.
    pub session_id: String,
    /// Project directory (parent of the JSONL file).
    pub project_dir: PathBuf,
    /// Encoded project name (directory name, e.g., "-Users-user-repos-foo").
    pub project_hash: String,
}

/// Scan a base directory for Claude Code session JSONL files.
///
/// Looks for `<base>/<project-hash>/<uuid>.jsonl` files.
/// Skips subagent files (those are inside `<uuid>/subagents/`).
/// Returns one DiscoveredSession per JSONL file found.
pub fn discover_sessions(base_dir: &Path) -> Vec<DiscoveredSession>;

/// Resolve the Claude Code projects directory.
/// Returns `~/.claude/projects/` with home directory expansion.
/// Returns None if the directory does not exist.
pub fn resolve_projects_dir() -> Option<PathBuf>;

/// Extract a session UUID from a JSONL filename.
/// Returns None if the filename doesn't match the UUID pattern.
fn extract_session_id(filename: &str) -> Option<String>;
```

**Design rationale:**
- Discovery is separate from parsing — we first find all session files (fast directory scan), then parse them on demand or in batch.
- Subagent files are explicitly skipped: they live at `<uuid>/subagents/agent-<id>.jsonl` and are NOT top-level sessions.
- The UUID extraction validates the filename pattern to avoid picking up non-session files.
- `resolve_projects_dir()` uses `dirs::home_dir()` (from the `dirs` crate) for cross-platform home directory resolution.

**Tests:**

- Discover sessions in a temp directory with valid JSONL files -> correct count and paths
- Discover sessions skips non-JSONL files (e.g., `.json`, `.txt`)
- Discover sessions skips subagent JSONL files (`<uuid>/subagents/agent-*.jsonl`)
- Discover sessions with empty projects directory -> empty Vec
- Discover sessions with nonexistent directory -> empty Vec (not an error)
- Extract session ID from valid UUID filename -> Some(uuid)
- Extract session ID from non-UUID filename -> None
- Extract session ID from filename with extra characters -> None
- Discovered sessions include correct project_hash from parent directory name
- Multiple sessions in one project directory are each discovered separately

### Unit 4: ClaudeCodeAdapter (trait implementation)

The main adapter struct that implements `AgentAdapter`. Wires together discovery, parsing, and title generation.

**File:** `crates/veil-aggregator/src/claude_code/adapter.rs`

**Types:**

```rust
use std::path::PathBuf;
use crate::adapter::{AdapterError, AgentAdapter};
use veil_core::session::{AgentKind, SessionEntry, SessionId, SessionPreview};

/// Claude Code adapter for the session aggregator.
///
/// Discovers and parses session data from `~/.claude/projects/`.
pub struct ClaudeCodeAdapter {
    /// Base directory for Claude Code projects.
    /// Defaults to `~/.claude/projects/` but injectable for testing.
    projects_dir: PathBuf,
}

impl ClaudeCodeAdapter {
    /// Create an adapter with the default projects directory.
    pub fn new() -> Option<Self>;

    /// Create an adapter with a custom projects directory (for testing).
    pub fn with_projects_dir(dir: PathBuf) -> Self;
}

impl AgentAdapter for ClaudeCodeAdapter {
    fn name(&self) -> &str { "Claude Code" }

    fn agent_kind(&self) -> AgentKind { AgentKind::ClaudeCode }

    fn watch_paths(&self) -> Vec<PathBuf> {
        // Returns [self.projects_dir]
    }

    fn discover_sessions(&self) -> Vec<Result<SessionEntry, AdapterError>> {
        // 1. Call discovery::discover_sessions(self.projects_dir)
        // 2. For each DiscoveredSession, call parser::parse_session_file
        // 3. Convert ParsedSession into SessionEntry using title::generate_title
        // 4. Wrap parse errors in AdapterError::ParseError
    }

    fn session_preview(&self, id: &SessionId) -> Result<Option<SessionPreview>, AdapterError> {
        // 1. Find the JSONL file for this session ID
        // 2. Parse it (or use cached ParsedSession if available)
        // 3. Build SessionPreview from parsed data
    }
}
```

**Design rationale:**
- The adapter is injectable: `with_projects_dir` enables testing with temp directories containing fixture JSONL files.
- `new()` returns `Option<Self>` — `None` when `~/.claude/projects/` doesn't exist (graceful no-op).
- `discover_sessions` does full parsing because SessionEntry needs the extracted title, working directory, and timestamps. This is acceptable for startup/re-index — incremental updates via file watcher (VEI-25) will be more targeted.
- `session_preview` re-reads and parses the JSONL file. This is the lazy-loading pattern: previews are only loaded when the user clicks on a session in the UI.

**Title generation strategy:**
1. The `slug` field (e.g., "majestic-stargazing-planet") is auto-generated gibberish — treat it the same as a UUID.
2. Claude Code does not provide a meaningful session title in its JSONL format.
3. Use `title::generate_title(None, first_user_message)` — always fall through to heuristic extraction from the first user message.
4. If the first user message starts with task-notification XML or is a compact summary, skip it and use the next real user message.

**Tests:**

- Adapter name returns "Claude Code"
- Adapter agent_kind returns `AgentKind::ClaudeCode`
- Watch paths returns the projects directory
- `with_projects_dir` on a directory containing valid JSONL fixtures -> discover_sessions returns correct SessionEntries
- Session entries have: correct session ID, AgentKind::ClaudeCode, generated title from first message, working directory from cwd field, git branch from gitBranch field, correct timestamps
- Session entries have branch=None, pr_number=None, pr_url=None, plan_content=None (metadata extraction is VEI-27)
- Discover on empty directory -> empty Vec
- Discover on directory with unparseable JSONL -> returns Err in results Vec
- session_preview for valid session ID -> Some(SessionPreview) with message counts and first messages
- session_preview for unknown session ID -> Ok(None)
- new() returns None when ~/.claude/projects/ does not exist (tests use with_projects_dir instead)

### Unit 5: Module structure and integration

Wire the Claude Code adapter module into the aggregator crate.

**Files:**

```
crates/veil-aggregator/src/
  claude_code/
    mod.rs          # Module root — re-exports ClaudeCodeAdapter
    jsonl.rs        # Unit 1: JSONL deserialization types
    parser.rs       # Unit 2: File parser
    discovery.rs    # Unit 3: Session discovery
    adapter.rs      # Unit 4: ClaudeCodeAdapter
  lib.rs            # Add `pub mod claude_code;`
```

**`claude_code/mod.rs`:**
```rust
//! Claude Code agent adapter — session discovery and JSONL parsing.

mod adapter;
mod discovery;
mod jsonl;
mod parser;

pub use adapter::ClaudeCodeAdapter;
```

Only `ClaudeCodeAdapter` is public. The JSONL types, parser, and discovery are internal implementation details.

**Test fixtures:**

```
crates/veil-aggregator/src/claude_code/
  testdata/                              # Committed test fixture JSONL files
    simple_session.jsonl                 # Minimal session: 1 user msg, 1 assistant response
    multi_turn_session.jsonl             # Multiple turns with tool calls
    empty_session.jsonl                  # Empty file (zero bytes)
    compact_summary_session.jsonl        # Session starting with compact summary
    malformed_lines.jsonl                # Mix of valid and invalid JSON lines
```

Fixtures are minimal — just enough lines to exercise the parser, not copies of real session data. They are hand-crafted from the observed format documented above, with no real user data.

**Integration tests (in adapter.rs tests):**

- Create a temp directory with fixture JSONL files
- Instantiate `ClaudeCodeAdapter::with_projects_dir(temp_dir)`
- Call `discover_sessions()` and verify results match fixtures
- Call `session_preview()` for discovered session IDs
- Register the adapter in an `AdapterRegistry` and verify it works through the registry's interface

## Acceptance Criteria

1. `cargo build -p veil-aggregator` succeeds with the new `claude_code` module
2. `cargo test -p veil-aggregator` passes all new tests (JSONL, parser, discovery, adapter)
3. `cargo clippy --all-targets --all-features -- -D warnings` passes
4. `cargo fmt --check` passes
5. `ClaudeCodeAdapter` implements `AgentAdapter` and can be registered in `AdapterRegistry`
6. JSONL parser handles all observed message types without panicking
7. Malformed lines are counted and logged, not propagated as errors
8. Session titles are heuristically generated from first user messages (no raw UUIDs or slugs)
9. Working directory is extracted from the `cwd` field, not decoded from the project hash
10. Subagent JSONL files are not surfaced as separate sessions
11. Adapter gracefully returns empty results when `~/.claude/projects/` does not exist
12. Test fixtures cover: happy path, empty file, malformed lines, compact summaries, multi-turn with tool calls
13. No metadata extraction logic (branch/PR/plan) — that is VEI-27
14. No property-based tests — that is VEI-28

## Dependencies

**New crate dependencies for veil-aggregator:**

| Dependency | Version | Reason |
|-----------|---------|--------|
| `serde` | 1 (workspace) | JSONL deserialization |
| `serde_json` | 1 | JSONL line parsing, `Value` type for unstructured tool inputs |
| `dirs` | 6 | Cross-platform home directory resolution |

**Add to workspace Cargo.toml `[workspace.dependencies]`:**

```toml
serde_json = "1"
dirs = "6"
```

**Add to veil-aggregator Cargo.toml `[dependencies]`:**

```toml
serde.workspace = true
serde_json.workspace = true
dirs.workspace = true
```

`serde` is already a workspace dependency. `serde_json` and `dirs` need to be added.

**No new external tools or system dependencies required.**
