# VEI-20: Error Handling -- Transparent, Structured Error Display

## Context

Veil's error handling philosophy is: transparent, informative, user-in-control. Like Rust compiler errors -- structured, contextual, actionable. The system design document (section "Error Handling") establishes four principles:

1. **Never silently swallow errors** -- every failure surfaces to the user in context
2. **Never auto-dismiss** -- user acknowledges errors on their own timeline
3. **Never crash the whole app** -- isolate failures to their component
4. **Be informative** -- show what happened, why, and what options the user has

Today, errors in Veil are ad-hoc. Each crate has its own `thiserror` error types (`ConfigError`, `StateError`, `WorkspaceError`, `SocketError`, `GhosttyError`, `PtyError`), but there is no unified representation for errors that need to be surfaced to the user. The existing `StateUpdate::ActorError` variant carries only a flat `actor_name` and `message` string -- no severity, no recovery actions, no component identification, no pane association.

### What this task covers

VEI-20 builds the **error reporting core** -- the types, formatting, state tracking, and conversions that all user-facing error display will build on:

- `ErrorReport` -- a structured error representation with severity, component, message, detail, suggestion, recovery actions, and optional pane association
- `ErrorSeverity`, `ErrorComponent`, `RecoveryAction` enums
- Rust compiler-style `format_display()` producing `error[component]: message` output
- `Display` trait on `ErrorReport`
- `StateUpdate::ErrorOccurred` and `StateUpdate::ErrorDismissed` variants
- `AppState` error tracking with auto-incrementing IDs and per-pane filtering
- `From` conversions from `ConfigError` and `PersistenceError` to `ErrorReport` (in veil-core), `SocketError` to `ErrorReport` (in veil-socket)
- Builder API: `with_detail()`, `with_suggestion()`, `with_recovery_actions()`, `with_pane_id()`

### What is out of scope (deferred)

- In-pane error display UI rendering
- Sidebar error display (warning badges, inline warnings)
- Surface isolation (one crashed terminal doesn't affect others)
- SQLite corruption recovery
- `From` conversions for `PtyError`, `GhosttyError`, `AggregatorError` (those crates don't depend on veil-core)

### Why now

The existing error handling is insufficient for a user-facing application. Without a structured error representation, every new component would invent its own way of surfacing errors to users -- or worse, swallow them silently. Building the core error types now ensures all future components have a consistent, tested foundation for error reporting.

## Implementation Units

### Unit 1: Error enums and ErrorReport type (`veil-core/src/error.rs`)

Define the core error representation types that all user-facing errors will be converted into.

**Types:**

```rust
// crates/veil-core/src/error.rs

/// How severe the error is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorSeverity {
    /// Degraded functionality, but the component keeps working.
    Warning,
    /// The operation failed, but the component can recover.
    Error,
    /// The component is unusable and cannot recover.
    Fatal,
}

/// Which component produced the error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorComponent {
    /// Configuration system (parse errors, missing config, validation).
    Config,
    /// Workspace persistence (save/restore state).
    Persistence,
    /// Terminal emulation (libghosty).
    Terminal,
    /// PTY management (process spawn, I/O).
    Pty,
    /// Socket API server.
    Socket,
    /// Session aggregator (adapter failures, indexing).
    Aggregator,
    /// OS-level or system errors.
    System,
}

/// What the user can do about an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecoveryAction {
    /// Retry the failed operation.
    Retry,
    /// Close the affected pane/component.
    Close,
    /// Dismiss the error (acknowledge and continue).
    Dismiss,
    /// Report the error (open a bug report flow).
    Report,
}

/// Unique identifier for a tracked error in AppState.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ErrorId(u64);

impl ErrorId {
    /// Create a new `ErrorId` from a raw u64.
    pub fn new(id: u64) -> Self { Self(id) }
    /// Return the inner u64 value.
    pub fn as_u64(self) -> u64 { self.0 }
}

/// A structured error report for user-facing display.
///
/// Modeled after Rust compiler errors: a severity level, component source,
/// primary message, optional detail and suggestion lines, and actionable
/// recovery options.
#[derive(Debug, Clone)]
pub struct ErrorReport {
    /// How severe this error is.
    pub severity: ErrorSeverity,
    /// Which component produced this error.
    pub component: ErrorComponent,
    /// Primary error message (one line, human-readable).
    pub message: String,
    /// Optional extended detail (technical context, expandable in UI).
    pub detail: Option<String>,
    /// Optional suggestion for how to fix the problem.
    pub suggestion: Option<String>,
    /// What the user can do about this error.
    pub recovery_actions: Vec<RecoveryAction>,
    /// Which pane this error is associated with, if any.
    pub pane_id: Option<PaneId>,
}
```

**Builder API on ErrorReport:**

```rust
impl ErrorReport {
    /// Create a new ErrorReport with the required fields.
    pub fn new(
        severity: ErrorSeverity,
        component: ErrorComponent,
        message: impl Into<String>,
    ) -> Self;

    /// Add extended detail.
    pub fn with_detail(self, detail: impl Into<String>) -> Self;

    /// Add a suggestion.
    pub fn with_suggestion(self, suggestion: impl Into<String>) -> Self;

    /// Set recovery actions.
    pub fn with_recovery_actions(self, actions: Vec<RecoveryAction>) -> Self;

    /// Associate with a specific pane.
    pub fn with_pane_id(self, pane_id: PaneId) -> Self;
}
```

**Display trait and format_display():**

```rust
impl ErrorReport {
    /// Produce a Rust-compiler-style formatted error string.
    ///
    /// Format:
    /// ```text
    /// error[config]: failed to parse config file
    ///   --> detail: unexpected character at line 5, column 12
    ///   = help: check your TOML syntax
    ///   = actions: Retry, Dismiss
    /// ```
    ///
    /// For warnings, the prefix is `warning[component]`.
    /// For fatal errors, the prefix is `fatal[component]`.
    pub fn format_display(&self) -> String;
}

impl std::fmt::Display for ErrorReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_display())
    }
}
```

**Display trait on enums:**

`ErrorSeverity`, `ErrorComponent`, and `RecoveryAction` all implement `Display` for use in formatted output:
- `ErrorSeverity::Warning` -> `"warning"`, `Error` -> `"error"`, `Fatal` -> `"fatal"`
- `ErrorComponent::Config` -> `"config"`, `Persistence` -> `"persistence"`, etc.
- `RecoveryAction::Retry` -> `"Retry"`, `Close` -> `"Close"`, etc.

**Changes to `lib.rs`:**

Add `pub mod error;` to `veil-core/src/lib.rs`.

**Test strategy:**

| Test | Type | What it verifies |
|------|------|-----------------|
| `error_severity_display_warning` | Unit | `Warning.to_string()` == `"warning"` |
| `error_severity_display_error` | Unit | `Error.to_string()` == `"error"` |
| `error_severity_display_fatal` | Unit | `Fatal.to_string()` == `"fatal"` |
| `error_component_display_all_variants` | Unit | Each variant formats to its lowercase name |
| `recovery_action_display_all_variants` | Unit | Each variant formats with capitalized name |
| `error_severity_equality` | Unit | Same variants are equal, different are not |
| `error_component_equality` | Unit | Same variants are equal, different are not |
| `recovery_action_equality` | Unit | Same variants are equal, different are not |
| `error_id_new_and_as_u64` | Unit | Round-trips correctly |
| `error_id_equality` | Unit | Same inner values are equal |
| `error_report_new_sets_required_fields` | Unit | Severity, component, message set; optionals are None/empty |
| `error_report_with_detail` | Unit | Builder sets detail field |
| `error_report_with_suggestion` | Unit | Builder sets suggestion field |
| `error_report_with_recovery_actions` | Unit | Builder sets recovery_actions |
| `error_report_with_pane_id` | Unit | Builder sets pane_id |
| `error_report_builder_chaining` | Unit | All builder methods can be chained |
| `format_display_error_no_optionals` | Unit | Produces `error[component]: message` with no extra lines |
| `format_display_warning_prefix` | Unit | Uses `warning[component]` prefix |
| `format_display_fatal_prefix` | Unit | Uses `fatal[component]` prefix |
| `format_display_with_detail` | Unit | Includes `  --> detail: ...` line |
| `format_display_with_suggestion` | Unit | Includes `  = help: ...` line |
| `format_display_with_actions` | Unit | Includes `  = actions: ...` line |
| `format_display_full_report` | Unit | All optional fields present, complete multi-line output |
| `display_trait_matches_format_display` | Unit | `format!("{}", report)` == `report.format_display()` |

### Unit 2: From conversions -- ConfigError and PersistenceError (`veil-core/src/error.rs`)

Implement `From<ConfigError>` and `From<PersistenceError>` for `ErrorReport` so that domain-specific errors from veil-core crates can be automatically converted into user-facing error reports.

**PersistenceError definition:**

There is no `PersistenceError` in the codebase yet. Since the task description specifies a `From<PersistenceError>` conversion and workspace persistence is a P0 feature (see PRD), we need to define it. The `PersistenceError` belongs in `veil-core` since persistence is a core concern (state serialization).

```rust
// crates/veil-core/src/error.rs (or a separate persistence module, but
// keeping it in error.rs since it is minimal and directly consumed by
// the From impl)

/// Errors from workspace persistence operations (save/restore).
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    /// Failed to read the persisted state file.
    #[error("failed to read state file at {path}: {source}")]
    ReadError {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    /// Failed to write the persisted state file.
    #[error("failed to write state file at {path}: {source}")]
    WriteError {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    /// Failed to parse the persisted state.
    #[error("failed to parse state file at {path}: {message}")]
    ParseError {
        path: std::path::PathBuf,
        message: String,
    },
    /// The state file format version is not supported.
    #[error("unsupported state file version: {version}")]
    UnsupportedVersion { version: u32 },
}
```

**From conversions:**

```rust
impl From<ConfigError> for ErrorReport {
    fn from(err: ConfigError) -> Self {
        match &err {
            ConfigError::ReadError { path, .. } => {
                ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, err.to_string())
                    .with_detail(format!("could not read {}", path.display()))
                    .with_suggestion("check that the file exists and is readable")
                    .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::Dismiss])
            }
            ConfigError::ParseError { path, .. } => {
                ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, err.to_string())
                    .with_detail(format!("syntax error in {}", path.display()))
                    .with_suggestion("check your TOML syntax")
                    .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::Dismiss])
            }
            ConfigError::ParseErrorWithLocation { path, line, column, .. } => {
                ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, err.to_string())
                    .with_detail(format!("at {}:{}:{}", path.display(), line, column))
                    .with_suggestion("check your TOML syntax at the indicated location")
                    .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::Dismiss])
            }
            ConfigError::NoConfigDir => {
                ErrorReport::new(ErrorSeverity::Warning, ErrorComponent::Config, err.to_string())
                    .with_suggestion("using default configuration")
                    .with_recovery_actions(vec![RecoveryAction::Dismiss])
            }
        }
    }
}

impl From<PersistenceError> for ErrorReport {
    fn from(err: PersistenceError) -> Self {
        match &err {
            PersistenceError::ReadError { path, .. } => {
                ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Persistence, err.to_string())
                    .with_detail(format!("could not read {}", path.display()))
                    .with_suggestion("starting with empty workspace state")
                    .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::Dismiss])
            }
            PersistenceError::WriteError { path, .. } => {
                ErrorReport::new(ErrorSeverity::Warning, ErrorComponent::Persistence, err.to_string())
                    .with_detail(format!("could not write {}", path.display()))
                    .with_suggestion("check disk space and file permissions")
                    .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::Dismiss])
            }
            PersistenceError::ParseError { path, .. } => {
                ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Persistence, err.to_string())
                    .with_detail(format!("corrupt state file at {}", path.display()))
                    .with_suggestion("the file may be corrupted; starting fresh")
                    .with_recovery_actions(vec![RecoveryAction::Dismiss])
            }
            PersistenceError::UnsupportedVersion { version } => {
                ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Persistence, err.to_string())
                    .with_detail(format!("state file version {} is not supported by this version of Veil", version))
                    .with_suggestion("upgrade Veil or delete the state file to start fresh")
                    .with_recovery_actions(vec![RecoveryAction::Dismiss])
            }
        }
    }
}
```

**Test strategy:**

| Test | Type | What it verifies |
|------|------|-----------------|
| `from_config_read_error` | Unit | `ConfigError::ReadError` converts to `ErrorReport` with severity Error, component Config, detail mentioning the path |
| `from_config_parse_error` | Unit | `ConfigError::ParseError` converts with TOML syntax suggestion |
| `from_config_parse_error_with_location` | Unit | Includes line/column in detail |
| `from_config_no_config_dir` | Unit | Converts to Warning severity (not Error) |
| `from_config_error_has_recovery_actions` | Unit | All ConfigError conversions include at least one recovery action |
| `from_persistence_read_error` | Unit | `PersistenceError::ReadError` converts with component Persistence |
| `from_persistence_write_error` | Unit | Write error is Warning severity (non-fatal, app continues) |
| `from_persistence_parse_error` | Unit | Parse error suggests starting fresh |
| `from_persistence_unsupported_version` | Unit | Includes version number in detail |
| `persistence_error_display` | Unit | All `PersistenceError` variants have informative Display output |
| `config_error_into_report_preserves_message` | Unit | The ErrorReport message contains the original error's Display string |

### Unit 3: From conversion -- SocketError (`veil-socket/src/error_conversion.rs`)

Implement `From<SocketError>` for `ErrorReport` in the veil-socket crate, since veil-socket already depends on veil-core.

**New file:** `crates/veil-socket/src/error_conversion.rs`

```rust
use veil_core::error::{ErrorComponent, ErrorReport, ErrorSeverity, RecoveryAction};
use crate::transport::SocketError;

impl From<SocketError> for ErrorReport {
    fn from(err: SocketError) -> Self {
        match &err {
            SocketError::Io(io_err) => {
                ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Socket, err.to_string())
                    .with_detail(format!("I/O error: {}", io_err))
                    .with_suggestion("check that no other Veil instance is running")
                    .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::Dismiss])
            }
            SocketError::UnsupportedPlatform => {
                ErrorReport::new(ErrorSeverity::Warning, ErrorComponent::Socket, err.to_string())
                    .with_suggestion("the socket API is not available on this platform")
                    .with_recovery_actions(vec![RecoveryAction::Dismiss])
            }
        }
    }
}
```

**Changes to `veil-socket/src/lib.rs`:** Add `mod error_conversion;` to the module declarations.

**Test strategy:**

| Test | Type | What it verifies |
|------|------|-----------------|
| `from_socket_io_error` | Unit | `SocketError::Io` converts to Error severity with Socket component |
| `from_socket_unsupported_platform` | Unit | `SocketError::UnsupportedPlatform` converts to Warning severity |
| `socket_error_report_has_recovery_actions` | Unit | Both variants include at least one recovery action |
| `socket_error_into_report_preserves_message` | Unit | ErrorReport message contains the SocketError Display string |

### Unit 4: StateUpdate variants and AppState error tracking

Wire the error reporting system into the existing state management architecture. Add `StateUpdate::ErrorOccurred` and `StateUpdate::ErrorDismissed` variants, and add error tracking to `AppState`.

**Changes to `message.rs`:**

Add two new `StateUpdate` variants:

```rust
pub enum StateUpdate {
    // ... existing variants ...

    /// A structured error occurred that should be displayed to the user.
    ErrorOccurred(ErrorReport),

    /// The user dismissed an error.
    ErrorDismissed {
        /// The ID of the error to dismiss.
        error_id: ErrorId,
    },
}
```

**Changes to `state.rs`:**

Add error tracking fields and methods to `AppState`:

```rust
use crate::error::{ErrorId, ErrorReport};

/// An error tracked in AppState, with an assigned ID.
#[derive(Debug, Clone)]
pub struct TrackedError {
    /// Unique identifier for this error.
    pub id: ErrorId,
    /// The error report.
    pub report: ErrorReport,
}

// In AppState:
pub struct AppState {
    // ... existing fields ...
    /// Active errors being displayed to the user.
    pub errors: Vec<TrackedError>,
    // next_id already exists and will be used for error IDs too
}
```

**New methods on AppState:**

```rust
impl AppState {
    /// Track a new error. Returns the assigned ErrorId.
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
        self.errors
            .iter()
            .filter(|e| e.report.pane_id == Some(pane_id))
            .collect()
    }

    /// Get errors not associated with any pane (global errors).
    pub fn global_errors(&self) -> Vec<&TrackedError> {
        self.errors
            .iter()
            .filter(|e| e.report.pane_id.is_none())
            .collect()
    }
}
```

**Initialize `errors` field in `AppState::new()`** as `Vec::new()`.

**Test strategy:**

| Test | Type | What it verifies |
|------|------|-----------------|
| `new_state_has_no_errors` | Unit | `AppState::new().errors` is empty |
| `add_error_returns_unique_id` | Unit | Two calls return different IDs |
| `add_error_increases_error_count` | Unit | `active_errors().len()` increases by 1 |
| `add_error_preserves_report_fields` | Unit | Stored report matches the one passed in |
| `dismiss_error_removes_it` | Unit | After dismiss, error is gone from `active_errors()` |
| `dismiss_error_returns_true` | Unit | Returns true when error exists |
| `dismiss_nonexistent_error_returns_false` | Unit | Returns false for unknown ID |
| `dismiss_error_does_not_affect_others` | Unit | Other errors remain after dismissing one |
| `errors_for_pane_filters_correctly` | Unit | Only errors with matching pane_id returned |
| `errors_for_pane_empty_when_no_match` | Unit | Returns empty vec for pane with no errors |
| `global_errors_filters_correctly` | Unit | Only errors with `pane_id: None` returned |
| `global_errors_excludes_pane_errors` | Unit | Errors with pane_id set are excluded |
| `state_update_error_occurred_round_trip` | Async unit | `StateUpdate::ErrorOccurred` round-trips through mpsc channel |
| `state_update_error_dismissed_round_trip` | Async unit | `StateUpdate::ErrorDismissed` round-trips through mpsc channel |
| `error_ids_share_sequence_with_other_ids` | Unit | Error IDs come from the same `next_id()` sequence as workspace/pane IDs, ensuring global uniqueness |
| `proptest_add_dismiss_invariants` | Property | After random add/dismiss sequences: no duplicate IDs, active_errors count matches additions minus successful dismissals |

## Acceptance Criteria

1. **Error types exist** -- `ErrorSeverity`, `ErrorComponent`, `RecoveryAction`, `ErrorId`, `ErrorReport`, and `PersistenceError` are defined in `veil-core/src/error.rs` with full doc comments and `Display` implementations.

2. **Builder API works** -- `ErrorReport::new()` creates a minimal report; `with_detail()`, `with_suggestion()`, `with_recovery_actions()`, `with_pane_id()` chain correctly.

3. **Rust compiler-style formatting** -- `ErrorReport::format_display()` produces multi-line output matching the format: `error[component]: message` / `  --> detail: ...` / `  = help: ...` / `  = actions: ...`. The `Display` trait delegates to `format_display()`.

4. **From conversions** -- `ConfigError`, `PersistenceError`, and `SocketError` all convert to `ErrorReport` with appropriate severity, component, message, detail, suggestion, and recovery actions.

5. **StateUpdate integration** -- `StateUpdate::ErrorOccurred(ErrorReport)` and `StateUpdate::ErrorDismissed { error_id }` variants exist and can be sent through the mpsc channel.

6. **AppState error tracking** -- `AppState` tracks active errors via `add_error()`, `dismiss_error()`, `active_errors()`, `errors_for_pane()`, and `global_errors()`. Error IDs come from the existing `next_id()` sequence.

7. **Module wiring** -- `veil-core/src/lib.rs` exports `pub mod error`, `veil-socket/src/lib.rs` includes the error conversion module.

8. **33+ tests pass** -- At minimum: 23 tests in Unit 1, 11 tests in Unit 2, 4 tests in Unit 3, 16 tests in Unit 4 = 54 tests total. All are specific, named, and verify distinct behaviors.

9. **Quality gate passes** -- `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo build` all pass.

10. **No regressions** -- All existing tests in `state.rs`, `message.rs`, and veil-socket still pass.

## Dependencies

### Existing (already in Cargo.toml)

- `thiserror` -- error type derivation for `PersistenceError`
- `tracing` -- structured logging
- `tokio` (dev, with `sync` + `macros` + `rt`) -- async channel tests for StateUpdate round-trips
- `proptest` (dev) -- property-based tests for state invariants
- `std::fmt` -- `Display` trait implementations

### In veil-socket

- `veil-core` -- already a dependency; the `From<SocketError>` impl uses `veil_core::error::*`

### No new dependencies needed

All required functionality is covered by existing crate dependencies. The error types are pure Rust structs/enums with standard trait implementations. No platform-specific code is needed for this task.
