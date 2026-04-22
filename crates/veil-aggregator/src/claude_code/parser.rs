//! JSONL file parser for Claude Code session files.
//!
//! Reads a session JSONL file line by line and produces a [`ParsedSession`]
//! summary. Handles I/O errors and malformed lines gracefully -- a single
//! bad line does not prevent parsing the rest of the file.

#![allow(unused_imports)]

use std::path::Path;

use chrono::{DateTime, Utc};

use crate::adapter::AdapterError;

use super::jsonl::{AssistantRecord, ContentBlock, JournalRecord, MessageContent, UserRecord};

/// Summary of a parsed Claude Code session JSONL file.
/// Contains the extracted metadata needed to build a `SessionEntry`.
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
    /// Slug (auto-generated name -- treated as gibberish for title purposes).
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
    /// Total count of `tool_use` blocks across all assistant messages.
    pub tool_use_count: usize,
    /// Number of lines that failed to parse.
    pub parse_error_count: usize,
}

/// Mutable accumulator used while scanning a JSONL file line by line.
struct SessionAccumulator {
    session_id_from_records: Option<String>,
    cwd: Option<String>,
    git_branch: Option<String>,
    version: Option<String>,
    slug: Option<String>,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    first_user_message: Option<String>,
    first_assistant_message: Option<String>,
    user_message_count: usize,
    assistant_message_count: usize,
    tool_use_count: usize,
    parse_error_count: usize,
    /// Whether the last user record was a compact summary, so we can
    /// skip the paired assistant response as well.
    last_user_was_compact_summary: bool,
}

impl SessionAccumulator {
    fn new() -> Self {
        Self {
            session_id_from_records: None,
            cwd: None,
            git_branch: None,
            version: None,
            slug: None,
            started_at: None,
            ended_at: None,
            first_user_message: None,
            first_assistant_message: None,
            user_message_count: 0,
            assistant_message_count: 0,
            tool_use_count: 0,
            parse_error_count: 0,
            last_user_was_compact_summary: false,
        }
    }

    /// Set a field to `value` if it is currently `None`.
    fn set_first(slot: &mut Option<String>, value: Option<&String>) {
        if slot.is_none() {
            if let Some(v) = value {
                *slot = Some(v.clone());
            }
        }
    }

    /// Update `started_at` / `ended_at` timestamps, keeping earliest and latest.
    fn update_timestamps(&mut self, ts: DateTime<Utc>) {
        match self.started_at {
            Some(existing) if existing <= ts => {}
            _ => self.started_at = Some(ts),
        }
        match self.ended_at {
            Some(existing) if existing >= ts => {}
            _ => self.ended_at = Some(ts),
        }
    }

    fn process_user(&mut self, user: &UserRecord) {
        Self::set_first(&mut self.session_id_from_records, user.session_id.as_ref());
        Self::set_first(&mut self.cwd, user.cwd.as_ref());
        Self::set_first(&mut self.git_branch, user.git_branch.as_ref());
        Self::set_first(&mut self.version, user.version.as_ref());

        if let Some(ts) = user.timestamp {
            self.update_timestamps(ts);
        }

        if user.is_compact_summary {
            self.last_user_was_compact_summary = true;
            return;
        }
        self.last_user_was_compact_summary = false;

        if let Some(text) = extract_user_text(user) {
            self.user_message_count += 1;
            if self.first_user_message.is_none() {
                self.first_user_message = Some(text.to_string());
            }
        }
    }

    fn process_assistant(&mut self, asst: &AssistantRecord) {
        Self::set_first(&mut self.session_id_from_records, asst.session_id.as_ref());
        Self::set_first(&mut self.cwd, asst.cwd.as_ref());
        Self::set_first(&mut self.git_branch, asst.git_branch.as_ref());
        Self::set_first(&mut self.version, asst.version.as_ref());
        Self::set_first(&mut self.slug, asst.slug.as_ref());

        if let Some(ts) = asst.timestamp {
            self.update_timestamps(ts);
        }

        if self.last_user_was_compact_summary {
            self.last_user_was_compact_summary = false;
            return;
        }

        self.assistant_message_count += 1;

        if let Some(ref msg) = asst.message {
            if let Some(ref content) = msg.content {
                self.tool_use_count += count_tool_uses(content);
                if self.first_assistant_message.is_none() {
                    if let Some(text) = extract_assistant_text(content) {
                        self.first_assistant_message = Some(text.to_string());
                    }
                }
            }
        }
    }

    fn track_session_id_and_timestamp(
        &mut self,
        session_id: Option<&String>,
        timestamp: Option<DateTime<Utc>>,
    ) {
        Self::set_first(&mut self.session_id_from_records, session_id);
        if let Some(ts) = timestamp {
            self.update_timestamps(ts);
        }
    }

    fn into_parsed_session(self, filename_stem: String) -> ParsedSession {
        ParsedSession {
            session_id: self.session_id_from_records.unwrap_or(filename_stem),
            cwd: self.cwd,
            git_branch: self.git_branch,
            version: self.version,
            slug: self.slug,
            started_at: self.started_at,
            ended_at: self.ended_at,
            first_user_message: self.first_user_message,
            first_assistant_message: self.first_assistant_message,
            user_message_count: self.user_message_count,
            assistant_message_count: self.assistant_message_count,
            tool_use_count: self.tool_use_count,
            parse_error_count: self.parse_error_count,
        }
    }
}

/// Parse a single JSONL file into a `ParsedSession`.
///
/// Reads line by line, accumulating metadata. Malformed lines are counted
/// in `parse_error_count` but do not prevent parsing the rest of the file.
pub fn parse_session_file(path: &Path) -> Result<ParsedSession, AdapterError> {
    use std::io::{BufRead, BufReader};

    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let filename_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    let mut acc = SessionAccumulator::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Some(record) = parse_line(line) else {
            acc.parse_error_count += 1;
            continue;
        };

        match record {
            JournalRecord::User(ref user) => acc.process_user(user),
            JournalRecord::Assistant(ref asst) => acc.process_assistant(asst),
            JournalRecord::System(ref r) => {
                acc.track_session_id_and_timestamp(r.session_id.as_ref(), r.timestamp);
            }
            JournalRecord::QueueOperation(ref r) => {
                acc.track_session_id_and_timestamp(r.session_id.as_ref(), r.timestamp);
            }
            JournalRecord::PrLink(ref r) => {
                acc.track_session_id_and_timestamp(r.session_id.as_ref(), r.timestamp);
            }
            JournalRecord::Progress(ref r) => {
                acc.track_session_id_and_timestamp(r.session_id.as_ref(), r.timestamp);
            }
            JournalRecord::LastPrompt(ref r) => {
                acc.track_session_id_and_timestamp(r.session_id.as_ref(), None);
            }
            JournalRecord::FileHistorySnapshot(ref r) => {
                acc.track_session_id_and_timestamp(r.session_id.as_ref(), None);
            }
        }
    }

    Ok(acc.into_parsed_session(filename_stem))
}

/// Parse a single JSONL line into a `JournalRecord`.
/// Returns `None` for lines that fail to parse (with a `tracing::debug` log).
pub fn parse_line(line: &str) -> Option<JournalRecord> {
    serde_json::from_str::<JournalRecord>(line).ok()
}

/// Extract the first plain-text user message from a `UserRecord`,
/// skipping compact summaries and `tool_result` messages.
fn extract_user_text(record: &UserRecord) -> Option<&str> {
    // Skip compact summaries entirely.
    if record.is_compact_summary {
        return None;
    }

    let message = record.message.as_ref()?;
    match &message.content {
        MessageContent::Text(text) => Some(text.as_str()),
        // Blocks content means tool_result — not a text user message.
        MessageContent::Blocks(_) => None,
    }
}

/// Extract the first text block from an assistant message's content array.
fn extract_assistant_text(content: &[ContentBlock]) -> Option<&str> {
    for block in content {
        if let ContentBlock::Text { text } = block {
            return Some(text.as_str());
        }
    }
    None
}

/// Count `tool_use` blocks in an assistant message's content array.
fn count_tool_uses(content: &[ContentBlock]) -> usize {
    content.iter().filter(|block| matches!(block, ContentBlock::ToolUse { .. })).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Helper: path to a test fixture file.
    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/claude_code/testdata").join(name)
    }

    // --- parse_session_file tests ---

    #[test]
    fn parse_simple_session_file() {
        let path = fixture_path("simple_session.jsonl");
        let parsed = parse_session_file(&path).expect("should parse simple session");

        assert_eq!(parsed.session_id, "11111111-1111-1111-1111-111111111111");
        assert_eq!(parsed.cwd.as_deref(), Some("/Users/testuser/repos/myproject"));
        assert_eq!(parsed.git_branch.as_deref(), Some("main"));
        assert_eq!(parsed.version.as_deref(), Some("2.1.78"));
        assert_eq!(parsed.first_user_message.as_deref(), Some("Implement the login endpoint"));
        assert_eq!(
            parsed.first_assistant_message.as_deref(),
            Some("I'll implement the login endpoint for you.")
        );
        assert_eq!(parsed.user_message_count, 1);
        assert_eq!(parsed.assistant_message_count, 1);
        assert_eq!(parsed.tool_use_count, 0);
        assert_eq!(parsed.parse_error_count, 0);
    }

    #[test]
    fn parse_multi_turn_session_has_correct_counts() {
        let path = fixture_path("multi_turn_session.jsonl");
        let parsed = parse_session_file(&path).expect("should parse multi-turn session");

        assert_eq!(parsed.session_id, "22222222-2222-2222-2222-222222222222");
        assert_eq!(parsed.cwd.as_deref(), Some("/Users/testuser/repos/webapp"));
        assert_eq!(parsed.git_branch.as_deref(), Some("feature/db-pool"));
        // First user message is the text message, not the tool result
        assert_eq!(
            parsed.first_user_message.as_deref(),
            Some("Refactor the database layer to use connection pooling")
        );
        // First assistant text is the first text block (not thinking)
        assert_eq!(
            parsed.first_assistant_message.as_deref(),
            Some("I'll start by examining the current database configuration.")
        );
        // 2 text user messages (first + "Great, now add a health check..."), tool results don't count
        assert_eq!(parsed.user_message_count, 2);
        // 4 assistant messages total
        assert_eq!(parsed.assistant_message_count, 4);
        // 3 tool_use blocks total (Read, Edit, Bash)
        assert_eq!(parsed.tool_use_count, 3);
        assert_eq!(parsed.parse_error_count, 0);
    }

    #[test]
    fn parse_compact_summary_session_skips_summary_for_first_message() {
        let path = fixture_path("compact_summary_session.jsonl");
        let parsed = parse_session_file(&path).expect("should parse compact summary session");

        assert_eq!(parsed.session_id, "33333333-3333-3333-3333-333333333333");
        // The first user message should skip the compact summary and use the real message
        assert_eq!(
            parsed.first_user_message.as_deref(),
            Some("Now add rate limiting to the auth endpoint")
        );
        // The first assistant text should skip the compact summary assistant response
        assert_eq!(
            parsed.first_assistant_message.as_deref(),
            Some("I'll add rate limiting to the auth endpoint.")
        );
    }

    #[test]
    fn parse_empty_file_returns_empty_parsed_session() {
        let path = fixture_path("empty_session.jsonl");
        let parsed = parse_session_file(&path).expect("should parse empty session");

        assert!(parsed.first_user_message.is_none());
        assert!(parsed.first_assistant_message.is_none());
        assert!(parsed.started_at.is_none());
        assert!(parsed.ended_at.is_none());
        assert_eq!(parsed.user_message_count, 0);
        assert_eq!(parsed.assistant_message_count, 0);
        assert_eq!(parsed.tool_use_count, 0);
        assert_eq!(parsed.parse_error_count, 0);
    }

    #[test]
    fn parse_malformed_lines_counts_errors_and_parses_valid() {
        let path = fixture_path("malformed_lines.jsonl");
        let parsed = parse_session_file(&path).expect("should parse despite malformed lines");

        assert_eq!(parsed.session_id, "44444444-4444-4444-4444-444444444444");
        // Valid user and assistant messages should still be parsed
        assert_eq!(parsed.first_user_message.as_deref(), Some("Fix the broken tests"));
        assert_eq!(parsed.first_assistant_message.as_deref(), Some("I'll fix the broken tests."));
        assert_eq!(parsed.user_message_count, 1);
        assert_eq!(parsed.assistant_message_count, 1);
        // 3 malformed lines: invalid json, unknown type, plain string
        assert_eq!(parsed.parse_error_count, 3);
    }

    #[test]
    fn parse_timestamps_started_at_is_earliest_ended_at_is_latest() {
        let path = fixture_path("multi_turn_session.jsonl");
        let parsed = parse_session_file(&path).expect("should parse");

        let started = parsed.started_at.expect("should have started_at");
        let ended = parsed.ended_at.expect("should have ended_at");
        assert!(started < ended, "started_at ({started}) should be before ended_at ({ended})");
    }

    // --- parse_line tests ---

    #[test]
    fn parse_line_valid_user_message() {
        let line = r#"{"type":"user","message":{"role":"user","content":"hello"},"uuid":"test-uuid","timestamp":"2026-03-24T04:52:44.890Z","sessionId":"test-session"}"#;
        let record = parse_line(line);
        assert!(record.is_some(), "valid line should parse");
        match record.unwrap() {
            JournalRecord::User(_) => {} // expected
            other => panic!("expected User record, got {other:?}"),
        }
    }

    #[test]
    fn parse_line_invalid_json_returns_none() {
        let line = "{not valid json}";
        let record = parse_line(line);
        assert!(record.is_none(), "invalid JSON should return None");
    }

    #[test]
    fn parse_line_empty_string_returns_none() {
        let record = parse_line("");
        assert!(record.is_none(), "empty string should return None");
    }

    // --- extract_user_text tests ---

    #[test]
    fn extract_user_text_from_string_content() {
        let record = UserRecord {
            uuid: None,
            parent_uuid: None,
            timestamp: None,
            session_id: None,
            cwd: None,
            git_branch: None,
            version: None,
            message: Some(super::super::jsonl::MessagePayload {
                role: Some("user".to_string()),
                content: MessageContent::Text("hello world".to_string()),
            }),
            is_sidechain: false,
            is_compact_summary: false,
        };
        let text = extract_user_text(&record);
        assert_eq!(text, Some("hello world"));
    }

    #[test]
    fn extract_user_text_from_tool_result_returns_none() {
        let record = UserRecord {
            uuid: None,
            parent_uuid: None,
            timestamp: None,
            session_id: None,
            cwd: None,
            git_branch: None,
            version: None,
            message: Some(super::super::jsonl::MessagePayload {
                role: Some("user".to_string()),
                content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                    tool_use_id: Some("toolu_001".to_string()),
                    content: Some(serde_json::json!("output")),
                    is_error: false,
                }]),
            }),
            is_sidechain: false,
            is_compact_summary: false,
        };
        let text = extract_user_text(&record);
        assert!(text.is_none(), "tool_result content should not be extracted as user text");
    }

    #[test]
    fn extract_user_text_from_compact_summary_returns_none() {
        let record = UserRecord {
            uuid: None,
            parent_uuid: None,
            timestamp: None,
            session_id: None,
            cwd: None,
            git_branch: None,
            version: None,
            message: Some(super::super::jsonl::MessagePayload {
                role: Some("user".to_string()),
                content: MessageContent::Text("Summary of previous conversation".to_string()),
            }),
            is_sidechain: false,
            is_compact_summary: true,
        };
        let text = extract_user_text(&record);
        assert!(text.is_none(), "compact summary should not be extracted as user text");
    }

    // --- extract_assistant_text tests ---

    #[test]
    fn extract_assistant_text_from_text_block() {
        let content = vec![ContentBlock::Text { text: "Here is my response.".to_string() }];
        let text = extract_assistant_text(&content);
        assert_eq!(text, Some("Here is my response."));
    }

    #[test]
    fn extract_assistant_text_from_only_tool_use_blocks_returns_none() {
        let content = vec![ContentBlock::ToolUse {
            id: Some("toolu_001".to_string()),
            name: Some("Bash".to_string()),
            input: Some(serde_json::json!({"command": "ls"})),
        }];
        let text = extract_assistant_text(&content);
        assert!(text.is_none(), "only tool_use blocks should return None");
    }

    #[test]
    fn extract_assistant_text_from_thinking_plus_text_returns_text() {
        let content = vec![
            ContentBlock::Thinking { thinking: "Let me think about this.".to_string() },
            ContentBlock::Text { text: "Here is my analysis.".to_string() },
        ];
        let text = extract_assistant_text(&content);
        assert_eq!(
            text,
            Some("Here is my analysis."),
            "should return text block, not thinking block"
        );
    }

    // --- count_tool_uses tests ---

    #[test]
    fn count_tool_uses_with_zero_tool_use_blocks() {
        let content = vec![ContentBlock::Text { text: "Just text.".to_string() }];
        let count = count_tool_uses(&content);
        assert_eq!(count, 0);
    }

    #[test]
    fn count_tool_uses_with_multiple_tool_use_blocks() {
        let content = vec![
            ContentBlock::Text { text: "Let me do some things.".to_string() },
            ContentBlock::ToolUse {
                id: Some("toolu_001".to_string()),
                name: Some("Read".to_string()),
                input: None,
            },
            ContentBlock::ToolUse {
                id: Some("toolu_002".to_string()),
                name: Some("Edit".to_string()),
                input: None,
            },
            ContentBlock::ToolUse {
                id: Some("toolu_003".to_string()),
                name: Some("Bash".to_string()),
                input: None,
            },
        ];
        let count = count_tool_uses(&content);
        assert_eq!(count, 3);
    }
}
