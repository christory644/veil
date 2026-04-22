//! JSONL file parser for Claude Code session files.
//!
//! Reads a session JSONL file line by line and produces a [`ParsedSession`]
//! summary. Handles I/O errors and malformed lines gracefully -- a single
//! bad line does not prevent parsing the rest of the file.

#![allow(unused_imports)]
#![allow(dead_code)]

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
    /// Total count of tool_use blocks across all assistant messages.
    pub tool_use_count: usize,
    /// Number of lines that failed to parse.
    pub parse_error_count: usize,
}

/// Parse a single JSONL file into a `ParsedSession`.
///
/// Reads line by line, accumulating metadata. Malformed lines are counted
/// in `parse_error_count` but do not prevent parsing the rest of the file.
pub fn parse_session_file(_path: &Path) -> Result<ParsedSession, AdapterError> {
    unimplemented!("parse_session_file: will be implemented in GREEN phase")
}

/// Parse a single JSONL line into a `JournalRecord`.
/// Returns `None` for lines that fail to parse (with a `tracing::debug` log).
pub fn parse_line(_line: &str) -> Option<JournalRecord> {
    unimplemented!("parse_line: will be implemented in GREEN phase")
}

/// Extract the first plain-text user message from a `UserRecord`,
/// skipping compact summaries and `tool_result` messages.
fn extract_user_text(_record: &UserRecord) -> Option<&str> {
    unimplemented!("extract_user_text: will be implemented in GREEN phase")
}

/// Extract the first text block from an assistant message's content array.
fn extract_assistant_text(_content: &[ContentBlock]) -> Option<&str> {
    unimplemented!("extract_assistant_text: will be implemented in GREEN phase")
}

/// Count `tool_use` blocks in an assistant message's content array.
fn count_tool_uses(_content: &[ContentBlock]) -> usize {
    unimplemented!("count_tool_uses: will be implemented in GREEN phase")
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
