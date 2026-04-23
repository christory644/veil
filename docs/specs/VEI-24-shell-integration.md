# VEI-24: Shell Integration -- Directory Tracking and Agent Detection

## Context

Veil needs to be aware of what is happening inside each terminal pane: what directory the shell is in, whether an AI agent is running, and what kind of environment the user is working in. This awareness powers workspace metadata in the sidebar (working directory, agent-running indicator) and seeds conversation entries when an agent session is detected.

This task implements the `veil-shell` crate, which contains the pure parsing and detection logic for shell integration. It processes PTY output (OSC 7 directory changes) and process tree snapshots (agent detection) to produce structured events that other parts of Veil can consume.

### What this task covers

- **OSC 7 parser**: Parse `OSC 7` (current working directory) payloads from PTY output into validated `PathBuf` values. OSC 7 uses the `file://hostname/path` URI format.
- **Agent process detector**: Given a snapshot of the process tree under a PTY's child PID, identify known agent processes (claude, codex, opencode, aider, etc.) and report their state.
- **Environment detector**: Given a working directory, detect environment characteristics (git repo, Node project, Python project, Rust project, etc.) by checking for marker files.
- **Shell event types**: Typed events (`ShellEvent`) that represent directory changes, agent detection, and environment changes, consumed by the event loop (wired in VEI-70).
- **Surface state tracker**: Per-surface state machine that tracks the current directory, active agent, and environment for a single terminal pane, deduplicating redundant events.

### What is out of scope (deferred)

- Wiring into the PTY I/O loop or event loop (VEI-70)
- Shell integration scripts for bash/zsh/fish (bonus/future -- the scripts that emit OSC 7 and custom sequences)
- Custom OSC sequences for command start/end timing
- Actual UI rendering changes based on this data
- Conversation entry creation in the aggregator (the shell crate produces events; the aggregator consumes them in a separate task)

### Why now

The workspace sidebar needs to show the current working directory and agent-running indicators. The conversations tab needs to know when an agent session starts and ends. Both depend on the parsing and detection logic this task provides. The PTY crate (VEI-9) and process management infrastructure are in place, providing the raw data (PTY output bytes, child PIDs) that this crate will consume.

### Relationship to existing code

- `veil-core::osc_parse` already parses OSC 9/99/777 for notifications. VEI-24 adds OSC 7 parsing in the new `veil-shell` crate (not in `veil-core`) because directory tracking is a shell integration concern, not a core state concern.
- `veil-core::session::AgentKind` defines the known agent variants (ClaudeCode, Codex, OpenCode, Aider). The agent detector in `veil-shell` maps process names to these variants.
- `veil-core::workspace::SurfaceId` identifies terminal surfaces. The surface state tracker is keyed by `SurfaceId`.
- `veil-pty::Pty::child_pid()` provides the root PID for process tree inspection.
- `veil-ghostty::Terminal::pwd()` provides OSC 7 state from libghosty. The OSC 7 parser in `veil-shell` is a standalone parser for cases where the raw PTY output stream is being processed directly (e.g., in the PTY read loop before data reaches libghosty, or in test scenarios).

## Implementation Units

### Unit 1: OSC 7 parser (`veil-shell/src/osc7.rs`)

Parse OSC 7 payloads containing `file://` URIs into working directory paths. OSC 7 is the standard mechanism terminals use to report the shell's current working directory.

**Types:**

```rust
// crates/veil-shell/src/osc7.rs

use std::path::PathBuf;

/// A parsed OSC 7 working directory report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Osc7Report {
    /// The hostname from the URI (empty string if omitted).
    pub hostname: String,
    /// The decoded filesystem path.
    pub path: PathBuf,
}

/// Errors from OSC 7 parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Osc7Error {
    /// The payload is not an OSC 7 sequence.
    #[error("not an OSC 7 sequence")]
    NotOsc7,
    /// The URI scheme is not `file://`.
    #[error("unsupported URI scheme: {scheme}")]
    UnsupportedScheme {
        /// The scheme that was found.
        scheme: String,
    },
    /// The path is empty after decoding.
    #[error("empty path in OSC 7 URI")]
    EmptyPath,
    /// The URI contains invalid percent-encoding.
    #[error("invalid percent-encoding in OSC 7 URI: {detail}")]
    InvalidEncoding {
        /// Description of the encoding error.
        detail: String,
    },
}

/// Parse an OSC 7 payload string into a directory report.
///
/// The `payload` is the content between `\x1b]` and the string terminator.
/// Expected format: `7;file://hostname/path/to/directory`
///
/// The path component is percent-decoded (e.g., `%20` becomes a space).
/// On POSIX, the path is used directly. On Windows, the path is
/// converted from `/C:/Users/...` to `C:\Users\...` format.
pub fn parse_osc7(payload: &str) -> Result<Osc7Report, Osc7Error> { ... }

/// Percent-decode a URI path component.
///
/// Decodes `%XX` sequences where `XX` is a two-digit hex value.
/// Invalid sequences (e.g., `%GG`, truncated `%X`) are returned as errors.
fn percent_decode(input: &str) -> Result<String, Osc7Error> { ... }
```

**Key behaviors:**
- Strips the `7;` prefix (matching the pattern in `veil-core::osc_parse`).
- Validates the `file://` scheme. Other schemes (http, etc.) return `UnsupportedScheme`.
- Extracts the hostname (between `://` and the next `/`). Empty hostname is valid (localhost implied).
- Percent-decodes the path: `%20` to space, `%C3%A9` to `e` with accent, etc.
- Returns `EmptyPath` if the decoded path is empty.
- Does not validate that the path exists on disk (that is the consumer's job).

**Files:**
- New: `crates/veil-shell/src/osc7.rs`

### Unit 2: Agent process detector (`veil-shell/src/agent_detector.rs`)

Detect known AI agent processes in a process tree rooted at a given PID. This is the core detection logic; process tree traversal is platform-specific.

**Types:**

```rust
// crates/veil-shell/src/agent_detector.rs

use veil_core::session::AgentKind;

/// A detected agent process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedAgent {
    /// Which agent was detected.
    pub kind: AgentKind,
    /// The process ID of the agent.
    pub pid: u32,
    /// The executable name that matched.
    pub exe_name: String,
}

/// A process entry from the process tree.
///
/// This is the platform-neutral representation. Platform-specific code
/// populates these by reading /proc (Linux), sysctl (macOS), etc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    /// Process ID.
    pub pid: u32,
    /// Parent process ID.
    pub ppid: u32,
    /// Executable name (basename, not full path).
    pub name: String,
}

/// Known agent process names mapped to their `AgentKind`.
///
/// Each entry is a (executable_name_pattern, AgentKind) pair. The pattern
/// is matched against the process name (case-sensitive exact match on the
/// basename).
const KNOWN_AGENTS: &[(&str, AgentKind)] = &[
    ("claude", AgentKind::ClaudeCode),
    ("codex", AgentKind::Codex),
    ("opencode", AgentKind::OpenCode),
    ("aider", AgentKind::Aider),
];

/// Detect agent processes in a list of process entries that are descendants
/// of the given root PID.
///
/// Walks the process tree from `root_pid` downward, checking each process
/// name against `KNOWN_AGENTS`. Returns all matches (there could be multiple
/// agents, or an agent spawning sub-agents).
///
/// The `processes` slice should contain the full system process list (or at
/// least all descendants of the root). The function filters to only
/// descendants of `root_pid`.
pub fn detect_agents(root_pid: u32, processes: &[ProcessEntry]) -> Vec<DetectedAgent> { ... }

/// Check if a single process name matches a known agent.
///
/// Returns the `AgentKind` if the name matches any known agent pattern.
pub fn identify_agent(process_name: &str) -> Option<AgentKind> { ... }

/// Build the set of all PIDs that are descendants of `root_pid`.
///
/// Performs a BFS/DFS from `root_pid` through parent-child relationships.
fn descendant_pids(root_pid: u32, processes: &[ProcessEntry]) -> Vec<u32> { ... }
```

**Key behaviors:**
- `identify_agent` does exact match on process basename. "claude" matches "claude" but not "claude-helper" or "my-claude". This is conservative; we can relax matching later.
- `detect_agents` first computes all descendant PIDs of `root_pid`, then checks each descendant against `KNOWN_AGENTS`.
- If no processes are descendants of `root_pid`, returns an empty vec.
- If `root_pid` itself matches an agent, it is included.
- Multiple agents can be detected simultaneously (edge case but valid).
- The function is pure: it takes a process list and returns results. It does not call any OS APIs -- that is done by the process lister (Unit 3).

**Files:**
- New: `crates/veil-shell/src/agent_detector.rs`

### Unit 3: Process lister (`veil-shell/src/process_list.rs`)

Platform-specific code to list running processes. Produces `ProcessEntry` values consumed by the agent detector.

**Types:**

```rust
// crates/veil-shell/src/process_list.rs

use crate::agent_detector::ProcessEntry;

/// Errors from process listing.
#[derive(Debug, thiserror::Error)]
pub enum ProcessListError {
    /// Failed to read process information from the OS.
    #[error("failed to list processes: {0}")]
    OsError(String),
}

/// List all running processes on the system.
///
/// Returns a snapshot of the process table. On macOS, uses `sysctl` with
/// `KERN_PROC_ALL`. On Linux, reads `/proc`. On Windows, uses
/// `CreateToolhelp32Snapshot`.
pub fn list_processes() -> Result<Vec<ProcessEntry>, ProcessListError> { ... }

/// List only processes that are descendants of the given PID.
///
/// Convenience function: calls `list_processes()` then filters to
/// descendants using the parent-child chain.
pub fn list_descendants(root_pid: u32) -> Result<Vec<ProcessEntry>, ProcessListError> { ... }
```

**macOS implementation (`#[cfg(target_os = "macos")]`):**
- Uses `libc::sysctl` with `CTL_KERN`, `KERN_PROC`, `KERN_PROC_ALL` to get the full process table.
- Extracts `kp_proc.p_pid`, `kp_eproc.e_ppid`, and `kp_proc.p_comm` from each `kinfo_proc`.
- `p_comm` is a fixed-size C string (16 bytes on macOS); convert with `CStr::from_ptr`.

**Linux implementation (`#[cfg(target_os = "linux")]`):**
- Reads `/proc/*/stat` files.
- Parses PID from directory name, PPID from field 4, and comm from field 2 (in parens).

**Windows implementation (`#[cfg(target_os = "windows")]`):**
- Stub that returns `ProcessListError::OsError("not yet implemented")`.

**Unsafe code:**
- The macOS `sysctl` path requires `unsafe` for the FFI call. This module will use `#![allow(unsafe_code)]` with `// SAFETY:` comments on each block, following the same pattern as `veil-pty/src/posix.rs`.
- Linux `/proc` reading is fully safe Rust.

**Files:**
- New: `crates/veil-shell/src/process_list.rs`

### Unit 4: Environment detector (`veil-shell/src/env_detector.rs`)

Detect environment characteristics of a working directory by checking for marker files and directories.

**Types:**

```rust
// crates/veil-shell/src/env_detector.rs

use std::path::Path;

/// A detected environment characteristic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EnvKind {
    /// Git repository (`.git` directory or file exists).
    Git,
    /// Node.js project (`package.json` exists).
    Node,
    /// Python project (`pyproject.toml`, `setup.py`, or `requirements.txt` exists).
    Python,
    /// Rust project (`Cargo.toml` exists).
    Rust,
    /// Go project (`go.mod` exists).
    Go,
    /// Java project (`pom.xml` or `build.gradle` exists).
    Java,
    /// Nix project (`flake.nix` or `shell.nix` exists).
    Nix,
}

/// Result of environment detection for a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvReport {
    /// The set of detected environment characteristics.
    /// A directory can have multiple (e.g., Git + Rust + Nix).
    pub kinds: Vec<EnvKind>,
}

/// Detect environment characteristics of a directory.
///
/// Checks for marker files in the given directory (not recursive -- only
/// checks the directory itself, not parents). Returns an `EnvReport`
/// with all detected characteristics.
///
/// If the directory does not exist or is not readable, returns an empty report.
pub fn detect_env(dir: &Path) -> EnvReport { ... }
```

**Marker file mapping:**

| EnvKind | Marker files/dirs checked |
|---------|--------------------------|
| Git | `.git` (dir or file) |
| Node | `package.json` |
| Python | `pyproject.toml`, `setup.py`, `requirements.txt` |
| Rust | `Cargo.toml` |
| Go | `go.mod` |
| Java | `pom.xml`, `build.gradle`, `build.gradle.kts` |
| Nix | `flake.nix`, `shell.nix`, `default.nix` |

**Key behaviors:**
- Only checks the given directory, not parents. If you cd into `src/`, you do not get Git detection unless `src/.git` exists.
- Multiple `EnvKind` values can be returned (a Rust project in a git repo with a Nix flake).
- Non-existent or unreadable directories return an empty `kinds` vec, not an error.
- Detection is synchronous and fast (just `Path::exists()` calls -- no file content reading).
- The `kinds` vec is sorted by `EnvKind` discriminant order for deterministic output.

**Files:**
- New: `crates/veil-shell/src/env_detector.rs`

### Unit 5: Shell event types (`veil-shell/src/event.rs`)

The typed events that `veil-shell` produces, consumed by the PTY I/O integration layer (VEI-70).

**Types:**

```rust
// crates/veil-shell/src/event.rs

use std::path::PathBuf;
use veil_core::session::AgentKind;
use veil_core::workspace::SurfaceId;

use crate::agent_detector::DetectedAgent;
use crate::env_detector::EnvReport;

/// Events produced by shell integration processing.
///
/// These are emitted by the `SurfaceShellState` tracker when it observes
/// meaningful state changes. The consumer (PTY I/O loop, event loop)
/// maps these to `StateUpdate` messages or direct `AppState` mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    /// The shell's working directory changed (OSC 7 received).
    DirectoryChanged {
        /// Which surface this applies to.
        surface_id: SurfaceId,
        /// The new working directory.
        path: PathBuf,
    },

    /// An AI agent process was detected in the surface's process tree.
    AgentStarted {
        /// Which surface this applies to.
        surface_id: SurfaceId,
        /// The detected agent.
        agent: DetectedAgent,
    },

    /// A previously detected AI agent process is no longer running.
    AgentStopped {
        /// Which surface this applies to.
        surface_id: SurfaceId,
        /// Which agent kind stopped.
        agent_kind: AgentKind,
        /// The PID that was previously detected.
        pid: u32,
    },

    /// The environment characteristics of the working directory changed.
    EnvironmentChanged {
        /// Which surface this applies to.
        surface_id: SurfaceId,
        /// The new environment report.
        env: EnvReport,
    },
}
```

**Files:**
- New: `crates/veil-shell/src/event.rs`

### Unit 6: Surface state tracker (`veil-shell/src/tracker.rs`)

Per-surface state machine that tracks the current directory, active agent, and environment. Deduplicates redundant events (e.g., repeated OSC 7 with the same path).

**Types:**

```rust
// crates/veil-shell/src/tracker.rs

use std::path::PathBuf;
use veil_core::workspace::SurfaceId;

use crate::agent_detector::{DetectedAgent, ProcessEntry};
use crate::env_detector::{EnvKind, EnvReport};
use crate::event::ShellEvent;
use crate::osc7::{Osc7Report, Osc7Error};

/// Tracked state for a single terminal surface.
#[derive(Debug)]
pub struct SurfaceShellState {
    /// Which surface this tracks.
    surface_id: SurfaceId,
    /// Current working directory (last OSC 7 value).
    current_dir: Option<PathBuf>,
    /// Currently detected agent (most recently detected).
    active_agent: Option<DetectedAgent>,
    /// Current environment report.
    current_env: Option<EnvReport>,
}

impl SurfaceShellState {
    /// Create a new tracker for a surface.
    pub fn new(surface_id: SurfaceId) -> Self { ... }

    /// Get the current working directory, if known.
    pub fn current_dir(&self) -> Option<&PathBuf> { ... }

    /// Get the currently active agent, if any.
    pub fn active_agent(&self) -> Option<&DetectedAgent> { ... }

    /// Get the current environment report, if any.
    pub fn current_env(&self) -> Option<&EnvReport> { ... }

    /// Process an OSC 7 payload. Returns a `ShellEvent::DirectoryChanged`
    /// if the directory actually changed, or `None` if it is the same as
    /// the current directory. Also triggers environment re-detection on
    /// directory change.
    ///
    /// Returns up to 2 events: DirectoryChanged and EnvironmentChanged.
    pub fn handle_osc7(&mut self, payload: &str) -> Vec<ShellEvent> { ... }

    /// Update agent detection from a process list snapshot.
    ///
    /// Compares the current process tree against the previously detected
    /// agent. Emits `AgentStarted` if a new agent is found, `AgentStopped`
    /// if the previous agent is gone, or both if the agent changed.
    pub fn update_agents(
        &mut self,
        root_pid: u32,
        processes: &[ProcessEntry],
    ) -> Vec<ShellEvent> { ... }

    /// Force re-detection of the environment for the current directory.
    /// Returns `EnvironmentChanged` if the environment differs from the
    /// previously reported one, or an empty vec if unchanged.
    pub fn refresh_env(&mut self) -> Vec<ShellEvent> { ... }

    /// Reset all tracked state (e.g., when the surface's process exits).
    pub fn reset(&mut self) { ... }
}
```

**Key behaviors:**
- **Deduplication:** `handle_osc7` only emits `DirectoryChanged` if the new path differs from `current_dir`. Same path OSC 7 reports (which happen on every prompt in many shells) are silently absorbed.
- **Cascading detection:** When the directory changes, `handle_osc7` automatically calls `detect_env` on the new directory and may emit `EnvironmentChanged` alongside `DirectoryChanged`.
- **Agent lifecycle:** `update_agents` tracks a single "primary" agent (the first detected, or the most recently started). If the agent process exits (PID no longer in descendants), it emits `AgentStopped`. If a new agent appears, it emits `AgentStarted`.
- **Reset:** When a surface's shell exits and restarts, `reset()` clears all state so stale data from the old process is not carried forward.

**Files:**
- New: `crates/veil-shell/src/tracker.rs`

### Unit 7: Crate setup and public API (`veil-shell/src/lib.rs`, `Cargo.toml`)

Create the `veil-shell` crate, wire it into the workspace, and define the public API surface.

**Cargo.toml:**

```toml
[package]
name = "veil-shell"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true

[dependencies]
veil-core = { path = "../veil-core" }
thiserror.workspace = true
tracing.workspace = true
libc.workspace = true

[dev-dependencies]
tempfile.workspace = true
proptest.workspace = true

[lints]
workspace = true
```

**lib.rs:**

```rust
#![deny(unsafe_code)]
#![warn(missing_docs)]

//! Shell integration for Veil: directory tracking, agent detection,
//! and environment awareness.
//!
//! This crate provides the parsing and detection logic for shell
//! integration features. It processes OSC 7 payloads, inspects
//! process trees for known AI agents, and detects project environments.
//!
//! The crate produces [`ShellEvent`]s that are consumed by the PTY I/O
//! integration layer (VEI-70) to update workspace and conversation state.

pub mod agent_detector;
pub mod env_detector;
pub mod event;
pub mod osc7;
pub mod tracker;

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[allow(unsafe_code)]
pub mod process_list;

#[cfg(target_os = "windows")]
pub mod process_list;
```

**Workspace changes:**
- Add `"crates/veil-shell"` to the `members` list in the root `Cargo.toml`.

**Files:**
- New: `crates/veil-shell/Cargo.toml`
- New: `crates/veil-shell/src/lib.rs`
- Modified: `Cargo.toml` (workspace members)

## Test Strategy Per Unit

### Unit 1: OSC 7 parser

**Happy path:**
- `"7;file://hostname/Users/chris/project"` parses to hostname `"hostname"`, path `/Users/chris/project`.
- `"7;file:///tmp/test"` parses with empty hostname, path `/tmp/test`.
- `"7;file://localhost/home/user"` parses with hostname `"localhost"`.

**Percent-decoding:**
- `"7;file:///path%20with%20spaces/dir"` decodes to `/path with spaces/dir`.
- `"7;file:///path%2Fwith%2Fslashes"` decodes to `/path/with/slashes` (encoded slashes are valid).
- Mixed encoded and unencoded: `"7;file:///normal/path%20here"`.

**Error cases:**
- `""` returns `NotOsc7`.
- `"0;window title"` returns `NotOsc7`.
- `"7;"` (missing URI) returns `EmptyPath`.
- `"7;http://example.com/path"` returns `UnsupportedScheme { scheme: "http" }`.
- `"7;file://"` returns `EmptyPath`.
- `"7;file:///path%GG/bad"` returns `InvalidEncoding`.
- `"7;file:///path%2"` (truncated percent) returns `InvalidEncoding`.

**Property-based:**
- For any valid POSIX path, encoding it as a file URI and parsing should round-trip.
- Arbitrary byte strings as payload should never panic.

### Unit 2: Agent process detector

**Happy path:**
- Process list with `claude` as descendant of root PID -> detects `AgentKind::ClaudeCode`.
- Process list with `codex` as descendant -> detects `AgentKind::Codex`.
- Process list with `opencode` as descendant -> detects `AgentKind::OpenCode`.
- Process list with `aider` as descendant -> detects `AgentKind::Aider`.

**Process tree traversal:**
- Agent is a direct child of root -> detected.
- Agent is a grandchild (root -> shell -> agent) -> detected.
- Agent is a deep descendant (root -> shell -> tmux -> shell -> agent) -> detected.
- Agent is NOT a descendant of root (sibling process) -> not detected.

**Edge cases:**
- Empty process list -> empty result.
- Root PID not in process list -> empty result (no descendants).
- Process name that contains an agent name as substring (e.g., "claude-helper") -> not detected (exact match).
- Multiple agents under same root -> all detected.
- Root PID itself is the agent process -> detected.

**`identify_agent` unit tests:**
- Each known agent name maps to correct `AgentKind`.
- Unknown names return `None`.
- Case sensitivity: "Claude" (capital C) does not match "claude".

### Unit 3: Process lister

**Integration tests (macOS/Linux only, `#[cfg(unix)]`):**
- `list_processes()` returns a non-empty list.
- Current process PID is in the list.
- Current process's PPID is in the list and matches `std::process::id()` parent.
- `list_descendants` for current PID returns at least the current process.

**Unit tests:**
- `ProcessEntry` construction and field access.
- Windows stub returns appropriate error.

**Note:** These are integration tests since they depend on the actual OS process table. They are gated behind `#[cfg(unix)]`.

### Unit 4: Environment detector

**Happy path (using tempdir):**
- Create tempdir with `.git/` directory -> `Git` detected.
- Create tempdir with `Cargo.toml` -> `Rust` detected.
- Create tempdir with `package.json` -> `Node` detected.
- Create tempdir with `go.mod` -> `Go` detected.
- Create tempdir with `pyproject.toml` -> `Python` detected.
- Create tempdir with `pom.xml` -> `Java` detected.
- Create tempdir with `flake.nix` -> `Nix` detected.

**Multiple markers:**
- Tempdir with `.git/`, `Cargo.toml`, and `flake.nix` -> all three detected.

**No markers:**
- Empty tempdir -> empty `kinds` vec.

**Edge cases:**
- Non-existent path -> empty `kinds` vec, no error.
- Path to a file (not directory) -> empty `kinds` vec.
- `.git` as a file (git worktree) -> `Git` detected (Path::exists works for files too).
- Markers in parent directory but not in target -> not detected (no recursive search).

### Unit 5: Shell event types

- All variants are constructible with appropriate fields.
- Events are `Clone`, `PartialEq`, `Eq`, `Debug`.
- Pattern matching works on all variants.

### Unit 6: Surface state tracker

**Directory tracking:**
- `handle_osc7` with valid OSC 7 emits `DirectoryChanged`.
- Repeated `handle_osc7` with same path emits nothing (dedup).
- `handle_osc7` with different path emits `DirectoryChanged`.
- `handle_osc7` with invalid payload emits nothing (error swallowed).
- `current_dir()` returns the last valid directory.

**Environment cascading:**
- Directory change to a git+rust directory emits both `DirectoryChanged` and `EnvironmentChanged`.
- Directory change to a directory with same env markers as previous -> only `DirectoryChanged` emitted (env dedup).
- `refresh_env()` with no current dir -> empty vec.

**Agent tracking:**
- `update_agents` with agent in process list -> `AgentStarted` emitted, `active_agent()` returns it.
- `update_agents` again with same agent still running -> no event (dedup).
- `update_agents` with agent gone -> `AgentStopped` emitted, `active_agent()` returns `None`.
- `update_agents` with different agent -> `AgentStopped` for old + `AgentStarted` for new.
- `update_agents` with no agent -> no event if none was active before.

**Reset:**
- After `reset()`, `current_dir()`, `active_agent()`, `current_env()` all return `None`.
- After `reset()`, next OSC 7 emits `DirectoryChanged` even if it is the same path as pre-reset.

**Property-based:**
- Any sequence of `handle_osc7` calls produces at most one `DirectoryChanged` per call.
- `update_agents` never produces both `AgentStarted` and `AgentStopped` for the same `AgentKind` in a single call.

## Acceptance Criteria

1. `veil-shell` crate exists in the workspace and compiles cleanly.
2. `parse_osc7` correctly parses standard `file://` URIs with percent-decoding, including edge cases like spaces, unicode, and empty hostnames.
3. `identify_agent` maps all known agent process names to correct `AgentKind` variants.
4. `detect_agents` correctly walks a process tree and finds agent processes at any depth.
5. `list_processes` returns real process data on macOS and Linux (integration test).
6. `detect_env` detects all listed environment types via marker files.
7. `SurfaceShellState` deduplicates repeated OSC 7 reports with the same path.
8. `SurfaceShellState` emits `AgentStarted`/`AgentStopped` on agent lifecycle transitions.
9. `SurfaceShellState` cascades environment detection on directory change.
10. `SurfaceShellState.reset()` clears all state and allows re-detection.
11. All types derive appropriate traits (`Debug`, `Clone`, `PartialEq`, `Eq` where applicable).
12. All existing tests in the workspace continue to pass.
13. `cargo clippy --all-targets --all-features -- -D warnings` passes.
14. `cargo fmt --check` passes.

## Dependencies

### Existing (already in workspace Cargo.toml)

- `veil-core` -- `AgentKind`, `SurfaceId` types
- `thiserror` -- error types
- `tracing` -- structured logging for detection operations
- `libc` -- process listing on macOS (sysctl FFI)
- `tempfile` -- temp directories for tests (dev-dependency)
- `proptest` -- property-based tests (dev-dependency)

### New dependencies: None

All functionality is implemented using `std` library features (`Path::exists`, `fs::read_dir`, `process::Command`), existing workspace dependencies, and platform-specific syscalls via `libc` (already a workspace dependency).

### Build requirements

- macOS or Linux for full test suite (process listing tests are `#[cfg(unix)]`)
- No external tools required at build time
- No external tools required at runtime (process listing uses OS APIs, not shell commands)
