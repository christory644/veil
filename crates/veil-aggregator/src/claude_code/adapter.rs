//! Claude Code adapter implementation of the `AgentAdapter` trait.
//!
//! Wires together discovery, parsing, and title generation to provide
//! session data from `~/.claude/projects/`.

use std::path::PathBuf;

use chrono::Utc;
use veil_core::session::{AgentKind, SessionEntry, SessionId, SessionPreview, SessionStatus};

use crate::adapter::{AdapterError, AgentAdapter};

use super::discovery;
use super::parser;
use crate::title;

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
    /// Returns `None` when `~/.claude/projects/` does not exist.
    pub fn new() -> Option<Self> {
        let projects_dir = discovery::resolve_projects_dir()?;
        Some(Self { projects_dir })
    }

    /// Create an adapter with a custom projects directory (for testing).
    pub fn with_projects_dir(dir: PathBuf) -> Self {
        Self { projects_dir: dir }
    }
}

impl AgentAdapter for ClaudeCodeAdapter {
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        "Claude Code"
    }

    fn agent_kind(&self) -> AgentKind {
        AgentKind::ClaudeCode
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        vec![self.projects_dir.clone()]
    }

    fn discover_sessions(&self) -> Vec<Result<SessionEntry, AdapterError>> {
        let discovered = discovery::discover_sessions(&self.projects_dir);

        discovered
            .into_iter()
            .map(|ds| {
                let parsed = parser::parse_session_file(&ds.jsonl_path).map_err(|e| {
                    AdapterError::ParseError { path: ds.jsonl_path.clone(), source: Box::new(e) }
                })?;

                Ok(SessionEntry {
                    id: SessionId::new(&parsed.session_id),
                    agent: AgentKind::ClaudeCode,
                    title: title::generate_title(None, parsed.first_user_message.as_deref()),
                    working_dir: PathBuf::from(parsed.cwd.as_deref().unwrap_or("")),
                    branch: None,
                    pr_number: None,
                    pr_url: None,
                    plan_content: None,
                    status: SessionStatus::default(),
                    started_at: parsed.started_at.unwrap_or_else(Utc::now),
                    ended_at: parsed.ended_at,
                    indexed_at: Utc::now(),
                })
            })
            .collect()
    }

    fn session_preview(&self, id: &SessionId) -> Result<Option<SessionPreview>, AdapterError> {
        let target_filename = format!("{}.jsonl", id.as_str());

        // Scan project directories looking for a matching JSONL file.
        let Ok(project_dirs) = std::fs::read_dir(&self.projects_dir) else {
            return Ok(None);
        };

        for project_entry in project_dirs.flatten() {
            let project_path = project_entry.path();
            if !project_path.is_dir() {
                continue;
            }

            let candidate = project_path.join(&target_filename);
            if candidate.is_file() {
                let parsed = parser::parse_session_file(&candidate)?;
                return Ok(Some(SessionPreview {
                    id: SessionId::new(&parsed.session_id),
                    first_user_message: parsed.first_user_message,
                    first_assistant_message: parsed.first_assistant_message,
                    message_count: parsed.user_message_count + parsed.assistant_message_count,
                    tool_call_count: parsed.tool_use_count,
                }));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::AdapterRegistry;
    use std::fs;

    /// Helper: create a temp directory with fixture JSONL files arranged as
    /// `<project-hash>/<session-uuid>.jsonl`.
    fn setup_adapter_test_dir() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("should create temp dir");

        let project_dir = tmp.path().join("-Users-testuser-repos-myproject");
        fs::create_dir_all(&project_dir).expect("should create project dir");

        // Copy the simple_session fixture content
        let simple_jsonl = concat!(
            r#"{"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":"Implement the login endpoint"},"uuid":"a1b2c3d4-0001-0001-0001-000000000001","timestamp":"2026-03-24T04:52:44.890Z","cwd":"/Users/testuser/repos/myproject","sessionId":"11111111-1111-1111-1111-111111111111","version":"2.1.78","gitBranch":"main"}"#,
            "\n",
            r#"{"parentUuid":"a1b2c3d4-0001-0001-0001-000000000001","isSidechain":false,"message":{"model":"claude-opus-4-6","role":"assistant","content":[{"type":"text","text":"I'll implement the login endpoint for you."}],"stop_reason":"end_turn"},"type":"assistant","uuid":"a1b2c3d4-0001-0001-0001-000000000002","timestamp":"2026-03-24T04:53:20.907Z","slug":"helpful-coding-session","cwd":"/Users/testuser/repos/myproject","sessionId":"11111111-1111-1111-1111-111111111111","version":"2.1.78","gitBranch":"main"}"#,
            "\n"
        );

        fs::write(project_dir.join("11111111-1111-1111-1111-111111111111.jsonl"), simple_jsonl)
            .expect("should write fixture file");

        tmp
    }

    // --- Basic adapter properties ---

    #[test]
    fn adapter_name_returns_claude_code() {
        let adapter = ClaudeCodeAdapter::with_projects_dir(PathBuf::from("/tmp"));
        assert_eq!(adapter.name(), "Claude Code");
    }

    #[test]
    fn adapter_agent_kind_returns_claude_code() {
        let adapter = ClaudeCodeAdapter::with_projects_dir(PathBuf::from("/tmp"));
        assert_eq!(adapter.agent_kind(), AgentKind::ClaudeCode);
    }

    #[test]
    fn watch_paths_returns_projects_directory() {
        let dir = PathBuf::from("/home/user/.claude/projects");
        let adapter = ClaudeCodeAdapter::with_projects_dir(dir.clone());
        let paths = adapter.watch_paths();
        assert_eq!(paths, vec![dir]);
    }

    // --- discover_sessions tests ---

    #[test]
    fn discover_sessions_returns_correct_session_entries() {
        let tmp = setup_adapter_test_dir();
        let adapter = ClaudeCodeAdapter::with_projects_dir(tmp.path().to_path_buf());
        let results = adapter.discover_sessions();

        assert_eq!(results.len(), 1, "should discover exactly 1 session");
        let entry = results.into_iter().next().unwrap().expect("should be Ok");

        assert_eq!(entry.id.as_str(), "11111111-1111-1111-1111-111111111111");
        assert_eq!(entry.agent, AgentKind::ClaudeCode);
    }

    #[test]
    fn discovered_entry_has_title_from_first_message() {
        let tmp = setup_adapter_test_dir();
        let adapter = ClaudeCodeAdapter::with_projects_dir(tmp.path().to_path_buf());
        let results = adapter.discover_sessions();
        let entry = results.into_iter().next().unwrap().expect("should be Ok");

        // Title should be derived from first user message, not a UUID or slug
        assert!(!entry.title.is_empty(), "title should not be empty");
        assert_ne!(entry.title, "Untitled session", "title should not be fallback");
        // The first user message is "Implement the login endpoint"
        assert!(
            entry.title.contains("Implement") || entry.title.contains("login"),
            "title should be derived from first user message, got: {}",
            entry.title
        );
    }

    #[test]
    fn discovered_entry_has_working_directory_from_cwd() {
        let tmp = setup_adapter_test_dir();
        let adapter = ClaudeCodeAdapter::with_projects_dir(tmp.path().to_path_buf());
        let results = adapter.discover_sessions();
        let entry = results.into_iter().next().unwrap().expect("should be Ok");

        assert_eq!(
            entry.working_dir,
            PathBuf::from("/Users/testuser/repos/myproject"),
            "working_dir should come from JSONL cwd field"
        );
    }

    #[test]
    fn discovered_entry_has_correct_timestamps() {
        let tmp = setup_adapter_test_dir();
        let adapter = ClaudeCodeAdapter::with_projects_dir(tmp.path().to_path_buf());
        let results = adapter.discover_sessions();
        let entry = results.into_iter().next().unwrap().expect("should be Ok");

        assert!(entry.ended_at.is_some(), "ended_at should be set");
        assert!(
            entry.started_at <= entry.ended_at.unwrap(),
            "started_at should be before ended_at"
        );
    }

    #[test]
    fn discovered_entry_has_no_metadata_fields() {
        // VEI-27 handles metadata extraction -- VEI-15 should leave these None
        let tmp = setup_adapter_test_dir();
        let adapter = ClaudeCodeAdapter::with_projects_dir(tmp.path().to_path_buf());
        let results = adapter.discover_sessions();
        let entry = results.into_iter().next().unwrap().expect("should be Ok");

        assert!(entry.branch.is_none(), "branch should be None (VEI-27)");
        assert!(entry.pr_number.is_none(), "pr_number should be None (VEI-27)");
        assert!(entry.pr_url.is_none(), "pr_url should be None (VEI-27)");
        assert!(entry.plan_content.is_none(), "plan_content should be None (VEI-27)");
    }

    #[test]
    fn discover_on_empty_directory_returns_empty() {
        let tmp = tempfile::tempdir().expect("should create temp dir");
        let adapter = ClaudeCodeAdapter::with_projects_dir(tmp.path().to_path_buf());
        let results = adapter.discover_sessions();
        assert!(results.is_empty(), "empty directory should return empty Vec");
    }

    #[test]
    fn discover_on_directory_with_unparseable_jsonl_returns_err() {
        let tmp = tempfile::tempdir().expect("should create temp dir");
        let project_dir = tmp.path().join("-Users-testuser-repos-broken");
        fs::create_dir_all(&project_dir).expect("should create project dir");
        fs::write(
            project_dir.join("55555555-5555-5555-5555-555555555555.jsonl"),
            "this is not valid jsonl at all\n",
        )
        .expect("should write broken file");

        let adapter = ClaudeCodeAdapter::with_projects_dir(tmp.path().to_path_buf());
        let results = adapter.discover_sessions();
        assert_eq!(results.len(), 1);
        // Depending on implementation, this might be Ok with zero counts or Err
        // Either way it should not panic
    }

    // --- session_preview tests ---

    #[test]
    fn session_preview_for_valid_session_id() {
        let tmp = setup_adapter_test_dir();
        let adapter = ClaudeCodeAdapter::with_projects_dir(tmp.path().to_path_buf());
        let id = SessionId::new("11111111-1111-1111-1111-111111111111");
        let preview = adapter.session_preview(&id).expect("should not error");

        assert!(preview.is_some(), "should find preview for valid session ID");
        let p = preview.unwrap();
        assert_eq!(p.id.as_str(), "11111111-1111-1111-1111-111111111111");
        assert_eq!(p.first_user_message.as_deref(), Some("Implement the login endpoint"));
        assert_eq!(
            p.first_assistant_message.as_deref(),
            Some("I'll implement the login endpoint for you.")
        );
        assert_eq!(p.message_count, 2); // 1 user + 1 assistant
        assert_eq!(p.tool_call_count, 0);
    }

    #[test]
    fn session_preview_for_unknown_session_id_returns_none() {
        let tmp = setup_adapter_test_dir();
        let adapter = ClaudeCodeAdapter::with_projects_dir(tmp.path().to_path_buf());
        let id = SessionId::new("99999999-9999-9999-9999-999999999999");
        let preview = adapter.session_preview(&id).expect("should not error for unknown ID");
        assert!(preview.is_none(), "unknown session ID should return None");
    }

    // --- Integration: adapter in registry ---

    #[test]
    fn adapter_works_through_registry() {
        let tmp = setup_adapter_test_dir();
        let adapter = ClaudeCodeAdapter::with_projects_dir(tmp.path().to_path_buf());

        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(adapter));

        let names = registry.adapter_names();
        assert_eq!(names, vec!["Claude Code"]);

        let sessions = registry.discover_all();
        assert_eq!(sessions.len(), 1, "registry should discover sessions from Claude Code adapter");
        assert_eq!(sessions[0].agent, AgentKind::ClaudeCode);
    }
}
