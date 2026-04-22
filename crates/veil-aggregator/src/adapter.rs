//! Agent adapter trait and error types for the session aggregator.
//!
//! Each agent harness (Claude Code, Codex, `OpenCode`, etc.) implements
//! the [`AgentAdapter`] trait to discover sessions and provide preview content.

use std::fmt;
use std::path::PathBuf;
use veil_core::session::{AgentKind, SessionEntry, SessionId, SessionPreview};

/// Errors that can occur during adapter operations.
#[derive(Debug)]
pub enum AdapterError {
    /// Session data directory not found or inaccessible.
    DataDirNotFound(PathBuf),
    /// Failed to parse session file.
    ParseError {
        /// Path to the file that failed to parse.
        path: PathBuf,
        /// The underlying parse error.
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// I/O error reading session data.
    IoError(std::io::Error),
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataDirNotFound(path) => {
                write!(f, "session data directory not found: {}", path.display())
            }
            Self::ParseError { path, source } => {
                write!(f, "failed to parse session file {}: {source}", path.display())
            }
            Self::IoError(err) => write!(f, "I/O error reading session data: {err}"),
        }
    }
}

impl std::error::Error for AdapterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ParseError { source, .. } => Some(source.as_ref()),
            Self::IoError(err) => Some(err),
            Self::DataDirNotFound(_) => None,
        }
    }
}

impl From<std::io::Error> for AdapterError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err)
    }
}

/// Trait implemented by each agent harness adapter.
///
/// Adapters discover sessions on the filesystem, parse their metadata,
/// and provide preview content. They are expected to be stateless --
/// the session store (database) owns the persistent state.
pub trait AgentAdapter: Send + Sync {
    /// Human-readable name for this harness (e.g., "Claude Code").
    fn name(&self) -> &str;

    /// Which `AgentKind` this adapter handles.
    fn agent_kind(&self) -> AgentKind;

    /// Filesystem paths this adapter monitors for session data.
    /// Used by the file watcher (VEI-25) to know what to watch.
    fn watch_paths(&self) -> Vec<PathBuf>;

    /// Discover all sessions this adapter can find.
    /// Returns entries with as much metadata as the adapter can extract.
    /// Must not panic -- returns errors per-session via Result in the Vec.
    fn discover_sessions(&self) -> Vec<Result<SessionEntry, AdapterError>>;

    /// Load preview content for a specific session.
    /// Returns None if the session ID is not recognized or data is unavailable.
    fn session_preview(&self, id: &SessionId) -> Result<Option<SessionPreview>, AdapterError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{make_test_entry, MockAdapter};

    #[test]
    fn trait_is_object_safe() {
        // This compiles only if AgentAdapter is object-safe.
        let adapter = MockAdapter::new("Test", AgentKind::ClaudeCode);
        let _boxed: Box<dyn AgentAdapter> = Box::new(adapter);
    }

    #[test]
    fn adapter_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Box<dyn AgentAdapter>>();
    }

    #[test]
    fn mock_adapter_name_and_kind() {
        let adapter = MockAdapter::new("Claude Code", AgentKind::ClaudeCode);
        assert_eq!(adapter.name(), "Claude Code");
        assert_eq!(adapter.agent_kind(), AgentKind::ClaudeCode);
    }

    #[test]
    fn discover_sessions_with_mixed_ok_and_err() {
        let entry = make_test_entry("ok-session", AgentKind::ClaudeCode);
        let adapter = MockAdapter::new("Test", AgentKind::ClaudeCode)
            .with_sessions(vec![entry])
            .with_errors(vec![AdapterError::DataDirNotFound(PathBuf::from("/missing"))]);

        let results = adapter.discover_sessions();
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
    }

    #[test]
    fn session_preview_returns_none_for_unknown_id() {
        let adapter = MockAdapter::new("Test", AgentKind::ClaudeCode);
        let result =
            adapter.session_preview(&SessionId::new("nonexistent")).expect("should not error");
        assert!(result.is_none());
    }

    #[test]
    fn session_preview_returns_some_when_available() {
        let preview = SessionPreview {
            id: SessionId::new("preview-session"),
            first_user_message: Some("Hello".to_string()),
            first_assistant_message: Some("Hi there".to_string()),
            message_count: 10,
            tool_call_count: 3,
        };
        let adapter = MockAdapter::new("Test", AgentKind::ClaudeCode).with_preview(preview);
        let result =
            adapter.session_preview(&SessionId::new("preview-session")).expect("should not error");
        assert!(result.is_some());
        let p = result.unwrap();
        assert_eq!(p.message_count, 10);
        assert_eq!(p.tool_call_count, 3);
    }

    #[test]
    fn adapter_error_display_data_dir_not_found() {
        let err = AdapterError::DataDirNotFound(PathBuf::from("/data/sessions"));
        let msg = err.to_string();
        assert!(msg.contains("/data/sessions"), "should contain the path: {msg}");
        assert!(msg.contains("not found"), "should mention not found: {msg}");
    }

    #[test]
    fn adapter_error_display_parse_error() {
        let err = AdapterError::ParseError {
            path: PathBuf::from("/data/session.json"),
            source: "invalid JSON".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("session.json"), "should contain the filename: {msg}");
        assert!(msg.contains("invalid JSON"), "should contain source error: {msg}");
    }

    #[test]
    fn adapter_error_display_io_error() {
        let err = AdapterError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"));
        let msg = err.to_string();
        assert!(msg.contains("gone"), "should contain io error message: {msg}");
    }

    #[test]
    fn watch_paths_returns_configured_paths() {
        let adapter = MockAdapter::new("Test", AgentKind::ClaudeCode)
            .with_watch_paths(vec![PathBuf::from("/home/user/.claude"), PathBuf::from("/tmp")]);
        let paths = adapter.watch_paths();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("/home/user/.claude"));
    }
}
