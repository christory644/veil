//! Core session types shared across the Veil application.
//!
//! These types represent AI agent conversation sessions and are used by
//! veil-aggregator (storage), veil-ui (display), and veil-socket (API).

use chrono::{DateTime, Utc};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

/// Opaque session identifier — wraps a String (agent-specific ID format).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Create a new `SessionId` from any string value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the inner string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Which agent harness produced this session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentKind {
    /// Claude Code agent sessions.
    ClaudeCode,
    /// OpenAI Codex agent sessions.
    Codex,
    /// OpenCode agent sessions.
    OpenCode,
    /// Aider agent sessions.
    Aider,
    /// Unknown or custom agent harness.
    Unknown(String),
}

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClaudeCode => write!(f, "Claude Code"),
            Self::Codex => write!(f, "Codex"),
            Self::OpenCode => write!(f, "OpenCode"),
            Self::Aider => write!(f, "Aider"),
            Self::Unknown(name) => write!(f, "{name}"),
        }
    }
}

/// Error returned when parsing an `AgentKind` from a string fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseAgentKindError;

impl fmt::Display for ParseAgentKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown agent kind")
    }
}

impl std::error::Error for ParseAgentKindError {}

impl FromStr for AgentKind {
    type Err = ParseAgentKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Claude Code" => Ok(Self::ClaudeCode),
            "Codex" => Ok(Self::Codex),
            "OpenCode" => Ok(Self::OpenCode),
            "Aider" => Ok(Self::Aider),
            other => Ok(Self::Unknown(other.to_string())),
        }
    }
}

/// Lifecycle state of a session.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SessionStatus {
    /// Session is currently active / in progress.
    Active,
    /// Session completed normally.
    Completed,
    /// Session ended with an error.
    Errored,
    /// Status is unknown or could not be determined.
    #[default]
    Unknown,
}

impl fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Completed => write!(f, "completed"),
            Self::Errored => write!(f, "errored"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl FromStr for SessionStatus {
    type Err = ParseAgentKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "completed" => Ok(Self::Completed),
            "errored" => Ok(Self::Errored),
            "unknown" => Ok(Self::Unknown),
            _ => Err(ParseAgentKindError),
        }
    }
}

/// Metadata record for one agent conversation session.
///
/// This is the primary data structure stored in SQLite and
/// passed to the UI layer for rendering conversation list entries.
#[derive(Debug, Clone)]
pub struct SessionEntry {
    /// Unique identifier for this session.
    pub id: SessionId,
    /// Which agent harness produced this session.
    pub agent: AgentKind,
    /// Display title for the session.
    pub title: String,
    /// Working directory where the session was started.
    pub working_dir: PathBuf,
    /// Git branch active during the session, if detectable.
    pub branch: Option<String>,
    /// Associated pull request number, if any.
    pub pr_number: Option<u64>,
    /// Associated pull request URL, if any.
    pub pr_url: Option<String>,
    /// Plan/spec content associated with this session.
    pub plan_content: Option<String>,
    /// Lifecycle status of the session.
    pub status: SessionStatus,
    /// When the session started.
    pub started_at: DateTime<Utc>,
    /// When the session ended, if it has ended.
    pub ended_at: Option<DateTime<Utc>>,
    /// When this record was last indexed/updated.
    pub indexed_at: DateTime<Utc>,
}

/// Lightweight preview content for a specific session.
///
/// Loaded lazily when user selects a conversation.
#[derive(Debug, Clone)]
pub struct SessionPreview {
    /// Session this preview belongs to.
    pub id: SessionId,
    /// First user message in the conversation.
    pub first_user_message: Option<String>,
    /// First assistant response in the conversation.
    pub first_assistant_message: Option<String>,
    /// Total number of messages in the session.
    pub message_count: usize,
    /// Total number of tool calls in the session.
    pub tool_call_count: usize,
}

/// Search result entry returned by FTS5 queries.
#[derive(Debug, Clone)]
pub struct SessionSearchResult {
    /// The matched session entry.
    pub entry: SessionEntry,
    /// Relevance score from FTS5.
    pub relevance: f64,
    /// Snippet of matching text with highlights.
    pub snippet: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // --- SessionId tests ---

    #[test]
    fn session_id_equal_when_same_inner_string() {
        let id1 = SessionId::new("abc-123");
        let id2 = SessionId::new("abc-123");
        assert_eq!(id1, id2);
    }

    #[test]
    fn session_id_not_equal_when_different_strings() {
        let id1 = SessionId::new("abc-123");
        let id2 = SessionId::new("xyz-789");
        assert_ne!(id1, id2);
    }

    #[test]
    fn session_id_display_shows_inner_value() {
        let id = SessionId::new("session-42");
        assert_eq!(id.to_string(), "session-42");
    }

    #[test]
    fn session_id_as_str_returns_inner() {
        let id = SessionId::new("test-id");
        assert_eq!(id.as_str(), "test-id");
    }

    // --- AgentKind tests ---

    #[test]
    fn agent_kind_display_claude_code() {
        assert_eq!(AgentKind::ClaudeCode.to_string(), "Claude Code");
    }

    #[test]
    fn agent_kind_display_codex() {
        assert_eq!(AgentKind::Codex.to_string(), "Codex");
    }

    #[test]
    fn agent_kind_display_open_code() {
        assert_eq!(AgentKind::OpenCode.to_string(), "OpenCode");
    }

    #[test]
    fn agent_kind_display_aider() {
        assert_eq!(AgentKind::Aider.to_string(), "Aider");
    }

    #[test]
    fn agent_kind_unknown_wraps_arbitrary_string() {
        let kind = AgentKind::Unknown("CustomAgent".to_string());
        assert_eq!(kind.to_string(), "CustomAgent");
    }

    #[test]
    fn agent_kind_round_trip_known_variants() {
        let variants =
            [AgentKind::ClaudeCode, AgentKind::Codex, AgentKind::OpenCode, AgentKind::Aider];
        for variant in &variants {
            let s = variant.to_string();
            let parsed: AgentKind = s.parse().expect("should parse known variant");
            assert_eq!(&parsed, variant);
        }
    }

    #[test]
    fn agent_kind_round_trip_unknown_variant() {
        let original = AgentKind::Unknown("MyCustomTool".to_string());
        let s = original.to_string();
        let parsed: AgentKind = s.parse().expect("should parse unknown variant");
        assert_eq!(parsed, original);
    }

    // --- SessionStatus tests ---

    #[test]
    fn session_status_default_is_unknown() {
        let status = SessionStatus::default();
        assert_eq!(status, SessionStatus::Unknown);
    }

    #[test]
    fn session_status_display_values() {
        assert_eq!(SessionStatus::Active.to_string(), "active");
        assert_eq!(SessionStatus::Completed.to_string(), "completed");
        assert_eq!(SessionStatus::Errored.to_string(), "errored");
        assert_eq!(SessionStatus::Unknown.to_string(), "unknown");
    }

    #[test]
    fn session_status_round_trip() {
        let variants = [
            SessionStatus::Active,
            SessionStatus::Completed,
            SessionStatus::Errored,
            SessionStatus::Unknown,
        ];
        for variant in &variants {
            let s = variant.to_string();
            let parsed: SessionStatus = s.parse().expect("should parse status");
            assert_eq!(&parsed, variant);
        }
    }

    // --- SessionEntry tests ---

    fn make_test_entry() -> SessionEntry {
        SessionEntry {
            id: SessionId::new("test-session-1"),
            agent: AgentKind::ClaudeCode,
            title: "Fix auth middleware".to_string(),
            working_dir: PathBuf::from("/home/user/project"),
            branch: Some("feature/auth-fix".to_string()),
            pr_number: Some(42),
            pr_url: Some("https://github.com/org/repo/pull/42".to_string()),
            plan_content: Some("Fix the JWT validation logic".to_string()),
            status: SessionStatus::Completed,
            started_at: Utc::now(),
            ended_at: Some(Utc::now()),
            indexed_at: Utc::now(),
        }
    }

    #[test]
    fn session_entry_all_fields_populated() {
        let entry = make_test_entry();
        assert_eq!(entry.id.as_str(), "test-session-1");
        assert_eq!(entry.agent, AgentKind::ClaudeCode);
        assert_eq!(entry.title, "Fix auth middleware");
        assert_eq!(entry.working_dir, PathBuf::from("/home/user/project"));
        assert_eq!(entry.branch.as_deref(), Some("feature/auth-fix"));
        assert_eq!(entry.pr_number, Some(42));
        assert!(entry.pr_url.is_some());
        assert!(entry.plan_content.is_some());
        assert_eq!(entry.status, SessionStatus::Completed);
        assert!(entry.ended_at.is_some());
    }

    #[test]
    fn session_entry_optional_fields_as_none() {
        let entry = SessionEntry {
            id: SessionId::new("minimal-session"),
            agent: AgentKind::Codex,
            title: "Quick fix".to_string(),
            working_dir: PathBuf::from("/tmp"),
            branch: None,
            pr_number: None,
            pr_url: None,
            plan_content: None,
            status: SessionStatus::Active,
            started_at: Utc::now(),
            ended_at: None,
            indexed_at: Utc::now(),
        };
        assert!(entry.branch.is_none());
        assert!(entry.pr_number.is_none());
        assert!(entry.pr_url.is_none());
        assert!(entry.plan_content.is_none());
        assert!(entry.ended_at.is_none());
    }
}
