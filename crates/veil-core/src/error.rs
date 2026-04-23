//! Structured error reporting types for user-facing error display.
//!
//! Modeled after Rust compiler errors: severity, component, primary message,
//! optional detail/suggestion, and recovery actions.

use crate::workspace::PaneId;

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

impl std::fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
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

impl std::fmt::Display for ErrorComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
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

impl std::fmt::Display for RecoveryAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
}

/// Unique identifier for a tracked error in AppState.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ErrorId(u64);

impl ErrorId {
    /// Create a new `ErrorId` from a raw u64.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Return the inner u64 value.
    pub fn as_u64(self) -> u64 {
        self.0
    }
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

impl ErrorReport {
    /// Create a new ErrorReport with the required fields.
    pub fn new(
        _severity: ErrorSeverity,
        _component: ErrorComponent,
        _message: impl Into<String>,
    ) -> Self {
        // Stub: returns defaults so tests that check field values will fail.
        Self {
            severity: ErrorSeverity::Warning,
            component: ErrorComponent::System,
            message: String::new(),
            detail: None,
            suggestion: None,
            recovery_actions: Vec::new(),
            pane_id: None,
        }
    }

    /// Add extended detail.
    pub fn with_detail(self, _detail: impl Into<String>) -> Self {
        // Stub: does not set the detail field.
        self
    }

    /// Add a suggestion.
    pub fn with_suggestion(self, _suggestion: impl Into<String>) -> Self {
        // Stub: does not set the suggestion field.
        self
    }

    /// Set recovery actions.
    pub fn with_recovery_actions(self, _actions: Vec<RecoveryAction>) -> Self {
        // Stub: does not set recovery_actions.
        self
    }

    /// Associate with a specific pane.
    pub fn with_pane_id(self, _pane_id: PaneId) -> Self {
        // Stub: does not set pane_id.
        self
    }

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
    pub fn format_display(&self) -> String {
        // Stub: returns empty string.
        String::new()
    }
}

impl std::fmt::Display for ErrorReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_display())
    }
}

/// Errors from workspace persistence operations (save/restore).
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    /// Failed to read the persisted state file.
    #[error("failed to read state file at {path}: {source}")]
    ReadError {
        /// Path to the state file.
        path: std::path::PathBuf,
        /// Underlying IO error.
        source: std::io::Error,
    },
    /// Failed to write the persisted state file.
    #[error("failed to write state file at {path}: {source}")]
    WriteError {
        /// Path to the state file.
        path: std::path::PathBuf,
        /// Underlying IO error.
        source: std::io::Error,
    },
    /// Failed to parse the persisted state.
    #[error("failed to parse state file at {path}: {message}")]
    ParseError {
        /// Path to the state file.
        path: std::path::PathBuf,
        /// Human-readable error message.
        message: String,
    },
    /// The state file format version is not supported.
    #[error("unsupported state file version: {version}")]
    UnsupportedVersion {
        /// The unsupported version number.
        version: u32,
    },
}

// Stub From<ConfigError>: returns a default ErrorReport (tests will fail).
impl From<crate::config::ConfigError> for ErrorReport {
    fn from(_err: crate::config::ConfigError) -> Self {
        ErrorReport::new(ErrorSeverity::Warning, ErrorComponent::System, "")
    }
}

// Stub From<PersistenceError>: returns a default ErrorReport (tests will fail).
impl From<PersistenceError> for ErrorReport {
    fn from(_err: PersistenceError) -> Self {
        ErrorReport::new(ErrorSeverity::Warning, ErrorComponent::System, "")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigError;
    use std::path::PathBuf;

    // ============================================================
    // Unit 1: ErrorSeverity Display
    // ============================================================

    #[test]
    fn error_severity_display_warning() {
        assert_eq!(ErrorSeverity::Warning.to_string(), "warning");
    }

    #[test]
    fn error_severity_display_error() {
        assert_eq!(ErrorSeverity::Error.to_string(), "error");
    }

    #[test]
    fn error_severity_display_fatal() {
        assert_eq!(ErrorSeverity::Fatal.to_string(), "fatal");
    }

    // ============================================================
    // Unit 1: ErrorComponent Display
    // ============================================================

    #[test]
    fn error_component_display_all_variants() {
        assert_eq!(ErrorComponent::Config.to_string(), "config");
        assert_eq!(ErrorComponent::Persistence.to_string(), "persistence");
        assert_eq!(ErrorComponent::Terminal.to_string(), "terminal");
        assert_eq!(ErrorComponent::Pty.to_string(), "pty");
        assert_eq!(ErrorComponent::Socket.to_string(), "socket");
        assert_eq!(ErrorComponent::Aggregator.to_string(), "aggregator");
        assert_eq!(ErrorComponent::System.to_string(), "system");
    }

    // ============================================================
    // Unit 1: RecoveryAction Display
    // ============================================================

    #[test]
    fn recovery_action_display_all_variants() {
        assert_eq!(RecoveryAction::Retry.to_string(), "Retry");
        assert_eq!(RecoveryAction::Close.to_string(), "Close");
        assert_eq!(RecoveryAction::Dismiss.to_string(), "Dismiss");
        assert_eq!(RecoveryAction::Report.to_string(), "Report");
    }

    // ============================================================
    // Unit 1: Equality
    // ============================================================

    #[test]
    fn error_severity_equality() {
        assert_eq!(ErrorSeverity::Warning, ErrorSeverity::Warning);
        assert_eq!(ErrorSeverity::Error, ErrorSeverity::Error);
        assert_eq!(ErrorSeverity::Fatal, ErrorSeverity::Fatal);
        assert_ne!(ErrorSeverity::Warning, ErrorSeverity::Error);
        assert_ne!(ErrorSeverity::Error, ErrorSeverity::Fatal);
        assert_ne!(ErrorSeverity::Warning, ErrorSeverity::Fatal);
    }

    #[test]
    fn error_component_equality() {
        assert_eq!(ErrorComponent::Config, ErrorComponent::Config);
        assert_eq!(ErrorComponent::Socket, ErrorComponent::Socket);
        assert_ne!(ErrorComponent::Config, ErrorComponent::Socket);
        assert_ne!(ErrorComponent::Persistence, ErrorComponent::Terminal);
    }

    #[test]
    fn recovery_action_equality() {
        assert_eq!(RecoveryAction::Retry, RecoveryAction::Retry);
        assert_eq!(RecoveryAction::Close, RecoveryAction::Close);
        assert_ne!(RecoveryAction::Retry, RecoveryAction::Close);
        assert_ne!(RecoveryAction::Dismiss, RecoveryAction::Report);
    }

    // ============================================================
    // Unit 1: ErrorId
    // ============================================================

    #[test]
    fn error_id_new_and_as_u64() {
        let id = ErrorId::new(42);
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn error_id_equality() {
        assert_eq!(ErrorId::new(1), ErrorId::new(1));
        assert_ne!(ErrorId::new(1), ErrorId::new(2));
    }

    // ============================================================
    // Unit 1: ErrorReport::new
    // ============================================================

    #[test]
    fn error_report_new_sets_required_fields() {
        let report =
            ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, "something broke");
        assert_eq!(report.severity, ErrorSeverity::Error);
        assert_eq!(report.component, ErrorComponent::Config);
        assert_eq!(report.message, "something broke");
        assert!(report.detail.is_none());
        assert!(report.suggestion.is_none());
        assert!(report.recovery_actions.is_empty());
        assert!(report.pane_id.is_none());
    }

    // ============================================================
    // Unit 1: Builder API
    // ============================================================

    #[test]
    fn error_report_with_detail() {
        let report = ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, "msg")
            .with_detail("some detail");
        assert_eq!(report.detail.as_deref(), Some("some detail"));
    }

    #[test]
    fn error_report_with_suggestion() {
        let report = ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, "msg")
            .with_suggestion("try this");
        assert_eq!(report.suggestion.as_deref(), Some("try this"));
    }

    #[test]
    fn error_report_with_recovery_actions() {
        let report = ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, "msg")
            .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::Dismiss]);
        assert_eq!(report.recovery_actions.len(), 2);
        assert_eq!(report.recovery_actions[0], RecoveryAction::Retry);
        assert_eq!(report.recovery_actions[1], RecoveryAction::Dismiss);
    }

    #[test]
    fn error_report_with_pane_id() {
        let pane = PaneId::new(7);
        let report = ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Terminal, "msg")
            .with_pane_id(pane);
        assert_eq!(report.pane_id, Some(pane));
    }

    #[test]
    fn error_report_builder_chaining() {
        let pane = PaneId::new(99);
        let report = ErrorReport::new(ErrorSeverity::Fatal, ErrorComponent::Pty, "process crashed")
            .with_detail("segfault at 0x0")
            .with_suggestion("restart the terminal")
            .with_recovery_actions(vec![RecoveryAction::Close, RecoveryAction::Report])
            .with_pane_id(pane);

        assert_eq!(report.severity, ErrorSeverity::Fatal);
        assert_eq!(report.component, ErrorComponent::Pty);
        assert_eq!(report.message, "process crashed");
        assert_eq!(report.detail.as_deref(), Some("segfault at 0x0"));
        assert_eq!(report.suggestion.as_deref(), Some("restart the terminal"));
        assert_eq!(report.recovery_actions.len(), 2);
        assert_eq!(report.pane_id, Some(pane));
    }

    // ============================================================
    // Unit 1: format_display
    // ============================================================

    #[test]
    fn format_display_error_no_optionals() {
        let report = ErrorReport::new(
            ErrorSeverity::Error,
            ErrorComponent::Config,
            "failed to parse config file",
        );
        let output = report.format_display();
        assert_eq!(output, "error[config]: failed to parse config file");
    }

    #[test]
    fn format_display_warning_prefix() {
        let report = ErrorReport::new(
            ErrorSeverity::Warning,
            ErrorComponent::Persistence,
            "state file missing",
        );
        let output = report.format_display();
        assert!(
            output.starts_with("warning[persistence]:"),
            "expected warning prefix, got: {output}"
        );
    }

    #[test]
    fn format_display_fatal_prefix() {
        let report =
            ErrorReport::new(ErrorSeverity::Fatal, ErrorComponent::Terminal, "GPU driver crash");
        let output = report.format_display();
        assert!(output.starts_with("fatal[terminal]:"), "expected fatal prefix, got: {output}");
    }

    #[test]
    fn format_display_with_detail() {
        let report = ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, "parse error")
            .with_detail("unexpected character at line 5, column 12");
        let output = report.format_display();
        assert!(
            output.contains("  --> detail: unexpected character at line 5, column 12"),
            "should contain detail line, got: {output}"
        );
    }

    #[test]
    fn format_display_with_suggestion() {
        let report = ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, "parse error")
            .with_suggestion("check your TOML syntax");
        let output = report.format_display();
        assert!(
            output.contains("  = help: check your TOML syntax"),
            "should contain help line, got: {output}"
        );
    }

    #[test]
    fn format_display_with_actions() {
        let report = ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, "parse error")
            .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::Dismiss]);
        let output = report.format_display();
        assert!(
            output.contains("  = actions: Retry, Dismiss"),
            "should contain actions line, got: {output}"
        );
    }

    #[test]
    fn format_display_full_report() {
        let report = ErrorReport::new(
            ErrorSeverity::Error,
            ErrorComponent::Config,
            "failed to parse config file",
        )
        .with_detail("unexpected character at line 5, column 12")
        .with_suggestion("check your TOML syntax")
        .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::Dismiss]);

        let output = report.format_display();
        let expected = "\
error[config]: failed to parse config file\n\
  --> detail: unexpected character at line 5, column 12\n\
  = help: check your TOML syntax\n\
  = actions: Retry, Dismiss";
        assert_eq!(output, expected);
    }

    #[test]
    fn display_trait_matches_format_display() {
        let report = ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Config, "test message")
            .with_detail("detail here")
            .with_suggestion("suggestion here");
        let display_output = format!("{}", report);
        let format_output = report.format_display();
        assert!(!format_output.is_empty(), "format_display should produce non-empty output");
        assert_eq!(display_output, format_output);
    }

    // ============================================================
    // Unit 2: From<ConfigError>
    // ============================================================

    #[test]
    fn from_config_read_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let config_err = ConfigError::ReadError {
            path: PathBuf::from("/home/user/.config/veil/config.toml"),
            source: io_err,
        };
        let report: ErrorReport = config_err.into();
        assert_eq!(report.severity, ErrorSeverity::Error);
        assert_eq!(report.component, ErrorComponent::Config);
        assert!(
            report
                .detail
                .as_ref()
                .map_or(false, |d| d.contains("/home/user/.config/veil/config.toml")),
            "detail should mention the path, got: {:?}",
            report.detail
        );
    }

    #[test]
    fn from_config_parse_error() {
        let config_err = ConfigError::ParseError {
            path: PathBuf::from("/config.toml"),
            message: "unexpected character".to_string(),
        };
        let report: ErrorReport = config_err.into();
        assert_eq!(report.severity, ErrorSeverity::Error);
        assert_eq!(report.component, ErrorComponent::Config);
        assert!(
            report.suggestion.as_ref().map_or(false, |s| s.contains("TOML")),
            "suggestion should mention TOML syntax, got: {:?}",
            report.suggestion
        );
    }

    #[test]
    fn from_config_parse_error_with_location() {
        let config_err = ConfigError::ParseErrorWithLocation {
            path: PathBuf::from("/config.toml"),
            line: 5,
            column: 12,
            message: "invalid type".to_string(),
        };
        let report: ErrorReport = config_err.into();
        assert_eq!(report.severity, ErrorSeverity::Error);
        assert_eq!(report.component, ErrorComponent::Config);
        let detail = report.detail.as_ref().expect("should have detail");
        assert!(
            detail.contains("5") && detail.contains("12"),
            "detail should contain line and column, got: {detail}"
        );
    }

    #[test]
    fn from_config_no_config_dir() {
        let config_err = ConfigError::NoConfigDir;
        let report: ErrorReport = config_err.into();
        assert_eq!(
            report.severity,
            ErrorSeverity::Warning,
            "NoConfigDir should be Warning, not Error"
        );
        assert_eq!(
            report.component,
            ErrorComponent::Config,
            "NoConfigDir should have Config component"
        );
        assert!(report.suggestion.is_some(), "NoConfigDir should have a suggestion");
    }

    #[test]
    fn from_config_error_has_recovery_actions() {
        // All ConfigError variants should produce at least one recovery action
        let errors: Vec<ConfigError> = vec![
            ConfigError::ReadError {
                path: PathBuf::from("/test"),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
            },
            ConfigError::ParseError { path: PathBuf::from("/test"), message: "bad".to_string() },
            ConfigError::ParseErrorWithLocation {
                path: PathBuf::from("/test"),
                line: 1,
                column: 1,
                message: "bad".to_string(),
            },
            ConfigError::NoConfigDir,
        ];
        for err in errors {
            let report: ErrorReport = err.into();
            assert!(
                !report.recovery_actions.is_empty(),
                "every ConfigError conversion should have at least one recovery action"
            );
        }
    }

    #[test]
    fn config_error_into_report_preserves_message() {
        let config_err = ConfigError::ParseError {
            path: PathBuf::from("/test.toml"),
            message: "unexpected character".to_string(),
        };
        let display_str = config_err.to_string();
        let report: ErrorReport = config_err.into();
        assert!(!report.message.is_empty(), "report message should not be empty");
        assert!(
            report.message.contains(&display_str) || display_str.contains(&report.message),
            "report message '{}' should contain or be contained in original error display '{}'",
            report.message,
            display_str
        );
    }

    // ============================================================
    // Unit 2: From<PersistenceError>
    // ============================================================

    #[test]
    fn from_persistence_read_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err =
            PersistenceError::ReadError { path: PathBuf::from("/data/state.json"), source: io_err };
        let report: ErrorReport = err.into();
        assert_eq!(report.severity, ErrorSeverity::Error);
        assert_eq!(report.component, ErrorComponent::Persistence);
    }

    #[test]
    fn from_persistence_write_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "disk full");
        let err = PersistenceError::WriteError {
            path: PathBuf::from("/data/state.json"),
            source: io_err,
        };
        let report: ErrorReport = err.into();
        assert_eq!(
            report.severity,
            ErrorSeverity::Warning,
            "write errors should be Warning (non-fatal, app continues)"
        );
        assert_eq!(
            report.component,
            ErrorComponent::Persistence,
            "write errors should have Persistence component"
        );
        assert!(report.suggestion.is_some(), "write errors should have a suggestion");
    }

    #[test]
    fn from_persistence_parse_error() {
        let err = PersistenceError::ParseError {
            path: PathBuf::from("/data/state.json"),
            message: "invalid JSON".to_string(),
        };
        let report: ErrorReport = err.into();
        assert_eq!(report.severity, ErrorSeverity::Error);
        assert!(
            report.suggestion.as_ref().map_or(false, |s| s.contains("fresh")),
            "suggestion should mention starting fresh, got: {:?}",
            report.suggestion
        );
    }

    #[test]
    fn from_persistence_unsupported_version() {
        let err = PersistenceError::UnsupportedVersion { version: 99 };
        let report: ErrorReport = err.into();
        assert_eq!(report.severity, ErrorSeverity::Error);
        let detail = report.detail.as_ref().expect("should have detail");
        assert!(
            detail.contains("99"),
            "detail should contain the version number 99, got: {detail}"
        );
    }

    #[test]
    fn persistence_error_display() {
        let read_err = PersistenceError::ReadError {
            path: PathBuf::from("/state.json"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        assert!(
            read_err.to_string().contains("/state.json"),
            "ReadError display should contain path"
        );

        let write_err = PersistenceError::WriteError {
            path: PathBuf::from("/state.json"),
            source: std::io::Error::new(std::io::ErrorKind::Other, "disk full"),
        };
        assert!(
            write_err.to_string().contains("/state.json"),
            "WriteError display should contain path"
        );

        let parse_err = PersistenceError::ParseError {
            path: PathBuf::from("/state.json"),
            message: "bad format".to_string(),
        };
        assert!(
            parse_err.to_string().contains("bad format"),
            "ParseError display should contain message"
        );

        let version_err = PersistenceError::UnsupportedVersion { version: 5 };
        assert!(
            version_err.to_string().contains("5"),
            "UnsupportedVersion display should contain version"
        );
    }
}
