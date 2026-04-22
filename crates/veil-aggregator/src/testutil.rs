//! Shared test helpers for the veil-aggregator crate.
//!
//! This module provides a configurable [`MockAdapter`] and convenience
//! constructors for [`SessionEntry`] so that individual test modules
//! don't need to duplicate the boilerplate.

use std::path::PathBuf;

use chrono::Utc;
use veil_core::session::{AgentKind, SessionEntry, SessionId, SessionPreview, SessionStatus};

use crate::adapter::{AdapterError, AgentAdapter};

/// A test adapter with configurable sessions, errors, watch paths, and preview.
pub struct MockAdapter {
    name: String,
    kind: AgentKind,
    paths: Vec<PathBuf>,
    sessions: Vec<SessionEntry>,
    errors: Vec<AdapterError>,
    preview: Option<SessionPreview>,
}

impl MockAdapter {
    /// Create a new mock adapter with the given name and agent kind.
    pub fn new(name: &str, kind: AgentKind) -> Self {
        Self {
            name: name.to_string(),
            kind,
            paths: vec![],
            sessions: vec![],
            errors: vec![],
            preview: None,
        }
    }

    /// Configure sessions returned by [`AgentAdapter::discover_sessions`].
    pub fn with_sessions(mut self, sessions: Vec<SessionEntry>) -> Self {
        self.sessions = sessions;
        self
    }

    /// Configure errors mixed into [`AgentAdapter::discover_sessions`] results.
    pub fn with_errors(mut self, errors: Vec<AdapterError>) -> Self {
        self.errors = errors;
        self
    }

    /// Configure paths returned by [`AgentAdapter::watch_paths`].
    pub fn with_watch_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.paths = paths;
        self
    }

    /// Configure the preview returned by [`AgentAdapter::session_preview`].
    pub fn with_preview(mut self, preview: SessionPreview) -> Self {
        self.preview = Some(preview);
        self
    }
}

impl AgentAdapter for MockAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn agent_kind(&self) -> AgentKind {
        self.kind.clone()
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        self.paths.clone()
    }

    fn discover_sessions(&self) -> Vec<Result<SessionEntry, AdapterError>> {
        let mut results: Vec<Result<SessionEntry, AdapterError>> =
            self.sessions.iter().cloned().map(Ok).collect();
        for err in &self.errors {
            results.push(clone_adapter_error(err));
        }
        results
    }

    fn session_preview(&self, _id: &SessionId) -> Result<Option<SessionPreview>, AdapterError> {
        Ok(self.preview.clone())
    }
}

/// Clone an `AdapterError` value for use in test mocks.
///
/// `AdapterError` is not `Clone` (it wraps `Box<dyn Error>`), so we
/// reconstruct an equivalent error from each variant.
fn clone_adapter_error(err: &AdapterError) -> Result<SessionEntry, AdapterError> {
    match err {
        AdapterError::DataDirNotFound(p) => Err(AdapterError::DataDirNotFound(p.clone())),
        AdapterError::ParseError { path, .. } => {
            Err(AdapterError::ParseError { path: path.clone(), source: "mock error".into() })
        }
        AdapterError::IoError(_) => Err(AdapterError::IoError(std::io::Error::other("mock io"))),
    }
}

/// Build a minimal [`SessionEntry`] with the given id and agent kind.
///
/// Title defaults to `"Session {id}"`. All optional fields are `None`.
pub fn make_test_entry(id: &str, agent: AgentKind) -> SessionEntry {
    SessionEntry {
        id: SessionId::new(id),
        agent,
        title: format!("Session {id}"),
        working_dir: PathBuf::from("/tmp"),
        branch: None,
        pr_number: None,
        pr_url: None,
        plan_content: None,
        status: SessionStatus::Active,
        started_at: Utc::now(),
        ended_at: None,
        indexed_at: Utc::now(),
    }
}
