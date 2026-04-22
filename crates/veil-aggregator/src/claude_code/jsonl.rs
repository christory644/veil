//! JSONL deserialization types for Claude Code session files.
//!
//! These types map directly to the observed JSONL format in `~/.claude/projects/`.
//! All fields are `Option<T>` for resilience against format evolution.
//! Many fields exist for correct serde deserialization even if not yet read
//! by the parser — they will be used by VEI-27 (metadata extraction).

#![allow(dead_code)] // serde fields must exist for deserialization; read access comes in VEI-27

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// A single JSONL record from a Claude Code session file.
/// Uses internally tagged dispatch on the `type` field.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum JournalRecord {
    /// User message (text or tool results).
    #[serde(rename = "user")]
    User(UserRecord),
    /// Assistant response (text, thinking, `tool_use`).
    #[serde(rename = "assistant")]
    Assistant(AssistantRecord),
    /// System event (hooks, compaction boundaries).
    #[serde(rename = "system")]
    System(SystemRecord),
    /// Message queue bookkeeping.
    #[serde(rename = "queue-operation")]
    QueueOperation(QueueOperationRecord),
    /// PR association event.
    #[serde(rename = "pr-link")]
    PrLink(PrLinkRecord),
    /// Hook progress indicator.
    #[serde(rename = "progress")]
    Progress(ProgressRecord),
    /// Internal bookkeeping.
    #[serde(rename = "last-prompt")]
    LastPrompt(LastPromptRecord),
    /// File state snapshot.
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot(FileHistorySnapshotRecord),
}

/// User message record.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRecord {
    /// Unique message identifier.
    pub uuid: Option<String>,
    /// Parent message UUID (for threading).
    pub parent_uuid: Option<String>,
    /// When the message was sent.
    pub timestamp: Option<DateTime<Utc>>,
    /// Session this message belongs to.
    pub session_id: Option<String>,
    /// Working directory at message time.
    pub cwd: Option<String>,
    /// Git branch at message time.
    pub git_branch: Option<String>,
    /// Claude Code version string.
    pub version: Option<String>,
    /// The message payload (role + content).
    pub message: Option<MessagePayload>,
    /// Whether this is a subagent/branched conversation.
    #[serde(default)]
    pub is_sidechain: bool,
    /// Whether this is a context-compaction summary message.
    #[serde(default)]
    pub is_compact_summary: bool,
}

/// Assistant message record.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRecord {
    /// Unique message identifier.
    pub uuid: Option<String>,
    /// Parent message UUID (for threading).
    pub parent_uuid: Option<String>,
    /// When the message was sent.
    pub timestamp: Option<DateTime<Utc>>,
    /// Session this message belongs to.
    pub session_id: Option<String>,
    /// Working directory at message time.
    pub cwd: Option<String>,
    /// Git branch at message time.
    pub git_branch: Option<String>,
    /// Claude Code version string.
    pub version: Option<String>,
    /// Auto-generated slug (gibberish name).
    pub slug: Option<String>,
    /// The message payload (role + content array).
    pub message: Option<AssistantMessagePayload>,
    /// Whether this is a subagent/branched conversation.
    #[serde(default)]
    pub is_sidechain: bool,
}

/// Message payload for user messages.
#[derive(Debug, Deserialize)]
pub struct MessagePayload {
    /// Message role (should be "user").
    pub role: Option<String>,
    /// Content — either a plain string or array of content blocks.
    pub content: MessageContent,
}

/// Message content is either a plain string or an array of content blocks.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain text content.
    Text(String),
    /// Array of content blocks (e.g., `tool_result`).
    Blocks(Vec<ContentBlock>),
}

/// Message payload for assistant messages.
#[derive(Debug, Deserialize)]
pub struct AssistantMessagePayload {
    /// Message role (should be "assistant").
    pub role: Option<String>,
    /// Model used for this response.
    pub model: Option<String>,
    /// Content blocks (text, thinking, `tool_use`).
    pub content: Option<Vec<ContentBlock>>,
    /// Why the response stopped.
    pub stop_reason: Option<String>,
}

/// A content block within a message.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text response from the assistant.
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },
    /// Extended thinking content.
    #[serde(rename = "thinking")]
    Thinking {
        /// The thinking content.
        thinking: String,
    },
    /// Tool invocation.
    #[serde(rename = "tool_use")]
    ToolUse {
        /// Tool use ID.
        id: Option<String>,
        /// Tool name (e.g., "Bash", "Read", "Edit").
        name: Option<String>,
        /// Tool input (unbounded structure).
        input: Option<serde_json::Value>,
    },
    /// Tool execution result (in user messages).
    #[serde(rename = "tool_result")]
    ToolResult {
        /// ID of the `tool_use` this result corresponds to.
        tool_use_id: Option<String>,
        /// Result content (unbounded structure).
        content: Option<serde_json::Value>,
        /// Whether the tool execution errored.
        #[serde(default)]
        is_error: bool,
    },
}

/// System event record.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemRecord {
    /// When the event occurred.
    pub timestamp: Option<DateTime<Utc>>,
    /// Session this event belongs to.
    pub session_id: Option<String>,
    /// Event subtype (e.g., `stop_hook_summary`, `compact_boundary`).
    pub subtype: Option<String>,
}

/// Queue operation record.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueOperationRecord {
    /// Operation type (e.g., "enqueue", "dequeue").
    pub operation: Option<String>,
    /// When the operation occurred.
    pub timestamp: Option<DateTime<Utc>>,
    /// Session this operation belongs to.
    pub session_id: Option<String>,
    /// Content of the queued message.
    pub content: Option<String>,
}

/// PR link record.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrLinkRecord {
    /// Session this PR link belongs to.
    pub session_id: Option<String>,
    /// Pull request number.
    pub pr_number: Option<u64>,
    /// Pull request URL.
    pub pr_url: Option<String>,
    /// Repository identifier.
    pub pr_repository: Option<String>,
    /// When the PR link was recorded.
    pub timestamp: Option<DateTime<Utc>>,
}

/// Progress indicator record.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressRecord {
    /// When the progress event occurred.
    pub timestamp: Option<DateTime<Utc>>,
    /// Session this progress belongs to.
    pub session_id: Option<String>,
}

/// Last prompt bookkeeping record.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastPromptRecord {
    /// Session this record belongs to.
    pub session_id: Option<String>,
}

/// File history snapshot record.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistorySnapshotRecord {
    /// Session this snapshot belongs to.
    pub session_id: Option<String>,
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Arbitrary byte sequences fed to serde_json must never panic.
        /// They should return Err for invalid input.
        #[test]
        fn arbitrary_strings_never_panic_on_deserialize(input in "\\PC*") {
            // Must not panic — Ok or Err are both fine.
            let _ = serde_json::from_str::<JournalRecord>(&input);
        }

        /// Arbitrary valid JSON objects with random type fields must not panic.
        #[test]
        fn arbitrary_json_object_with_random_type_never_panics(
            type_val in "[a-z_-]{1,20}",
            extra_key in "[a-z]{1,10}",
            extra_val in "[a-zA-Z0-9 ]{0,50}",
        ) {
            let json = format!(
                r#"{{"type": "{type_val}", "{extra_key}": "{extra_val}"}}"#
            );
            let _ = serde_json::from_str::<JournalRecord>(&json);
        }

        /// Valid user records with arbitrary string content must deserialize
        /// without panic.
        #[test]
        fn user_records_with_arbitrary_content_never_panic(
            content in "\\PC{0,500}"
        ) {
            // Escape the content for valid JSON embedding
            let escaped = serde_json::to_string(&content).unwrap();
            let json = format!(
                r#"{{"type": "user", "message": {{"role": "user", "content": {escaped}}}}}"#
            );
            let result = serde_json::from_str::<JournalRecord>(&json);
            // Should parse successfully since we built valid JSON
            prop_assert!(result.is_ok(), "valid user record should parse: {:?}", result.err());
        }

        /// Valid assistant records with arbitrary text blocks must deserialize
        /// without panic.
        #[test]
        fn assistant_records_with_arbitrary_text_never_panic(
            text in "\\PC{0,500}"
        ) {
            let escaped = serde_json::to_string(&text).unwrap();
            let json = format!(
                r#"{{"type": "assistant", "message": {{"role": "assistant", "content": [{{"type": "text", "text": {escaped}}}]}}}}"#
            );
            let result = serde_json::from_str::<JournalRecord>(&json);
            prop_assert!(result.is_ok(), "valid assistant record should parse: {:?}", result.err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_user_message_with_text_content() {
        let json = r#"{
            "parentUuid": null,
            "isSidechain": false,
            "type": "user",
            "message": {
                "role": "user",
                "content": "Implement the login endpoint"
            },
            "uuid": "a1b2c3d4-0001-0001-0001-000000000001",
            "timestamp": "2026-03-24T04:52:44.890Z",
            "cwd": "/Users/testuser/repos/myproject",
            "sessionId": "11111111-1111-1111-1111-111111111111",
            "version": "2.1.78",
            "gitBranch": "main"
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::User(user) => {
                assert_eq!(user.uuid.as_deref(), Some("a1b2c3d4-0001-0001-0001-000000000001"));
                assert_eq!(user.cwd.as_deref(), Some("/Users/testuser/repos/myproject"));
                assert_eq!(
                    user.session_id.as_deref(),
                    Some("11111111-1111-1111-1111-111111111111")
                );
                assert_eq!(user.version.as_deref(), Some("2.1.78"));
                assert_eq!(user.git_branch.as_deref(), Some("main"));
                assert!(!user.is_sidechain);
                assert!(!user.is_compact_summary);
                let msg = user.message.expect("should have message");
                assert_eq!(msg.role.as_deref(), Some("user"));
                match msg.content {
                    MessageContent::Text(text) => {
                        assert_eq!(text, "Implement the login endpoint");
                    }
                    MessageContent::Blocks(_) => panic!("expected Text content, got Blocks"),
                }
            }
            other => panic!("expected User record, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_user_message_with_tool_result_content() {
        let json = r#"{
            "type": "user",
            "message": {
                "role": "user",
                "content": [
                    {
                        "tool_use_id": "toolu_001",
                        "type": "tool_result",
                        "content": "output text...",
                        "is_error": false
                    }
                ]
            },
            "uuid": "a1b2c3d4-0001-0001-0001-000000000003",
            "timestamp": "2026-03-24T04:53:00.000Z",
            "sessionId": "11111111-1111-1111-1111-111111111111"
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::User(user) => {
                let msg = user.message.expect("should have message");
                match msg.content {
                    MessageContent::Blocks(blocks) => {
                        assert_eq!(blocks.len(), 1);
                        match &blocks[0] {
                            ContentBlock::ToolResult { tool_use_id, is_error, .. } => {
                                assert_eq!(tool_use_id.as_deref(), Some("toolu_001"));
                                assert!(!is_error);
                            }
                            other => panic!("expected ToolResult block, got {other:?}"),
                        }
                    }
                    MessageContent::Text(_) => panic!("expected Blocks content, got Text"),
                }
            }
            other => panic!("expected User record, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_assistant_message_with_text_block() {
        let json = r#"{
            "parentUuid": "a1b2c3d4-0001-0001-0001-000000000001",
            "isSidechain": false,
            "message": {
                "model": "claude-opus-4-6",
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "I'll implement the login endpoint for you."}
                ],
                "stop_reason": "end_turn"
            },
            "type": "assistant",
            "uuid": "a1b2c3d4-0001-0001-0001-000000000002",
            "timestamp": "2026-03-24T04:53:20.907Z",
            "slug": "helpful-coding-session",
            "cwd": "/Users/testuser/repos/myproject",
            "sessionId": "11111111-1111-1111-1111-111111111111",
            "version": "2.1.78",
            "gitBranch": "main"
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::Assistant(asst) => {
                assert_eq!(asst.slug.as_deref(), Some("helpful-coding-session"));
                assert_eq!(asst.cwd.as_deref(), Some("/Users/testuser/repos/myproject"));
                let msg = asst.message.expect("should have message");
                assert_eq!(msg.model.as_deref(), Some("claude-opus-4-6"));
                assert_eq!(msg.stop_reason.as_deref(), Some("end_turn"));
                let content = msg.content.expect("should have content");
                assert_eq!(content.len(), 1);
                match &content[0] {
                    ContentBlock::Text { text } => {
                        assert_eq!(text, "I'll implement the login endpoint for you.");
                    }
                    other => panic!("expected Text block, got {other:?}"),
                }
            }
            other => panic!("expected Assistant record, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_assistant_message_with_tool_use_block() {
        let json = r#"{
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [
                    {
                        "type": "tool_use",
                        "id": "toolu_001",
                        "name": "Bash",
                        "input": {"command": "cargo test"}
                    }
                ],
                "stop_reason": "tool_use"
            },
            "uuid": "test-uuid",
            "timestamp": "2026-03-24T04:53:20.907Z",
            "sessionId": "11111111-1111-1111-1111-111111111111"
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::Assistant(asst) => {
                let msg = asst.message.expect("should have message");
                let content = msg.content.expect("should have content");
                assert_eq!(content.len(), 1);
                match &content[0] {
                    ContentBlock::ToolUse { id, name, input } => {
                        assert_eq!(id.as_deref(), Some("toolu_001"));
                        assert_eq!(name.as_deref(), Some("Bash"));
                        assert!(input.is_some());
                    }
                    other => panic!("expected ToolUse block, got {other:?}"),
                }
            }
            other => panic!("expected Assistant record, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_assistant_message_with_thinking_block() {
        let json = r#"{
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Let me analyze this problem."},
                    {"type": "text", "text": "Here is my analysis."}
                ],
                "stop_reason": "end_turn"
            },
            "uuid": "test-uuid",
            "timestamp": "2026-03-24T04:53:20.907Z",
            "sessionId": "11111111-1111-1111-1111-111111111111"
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::Assistant(asst) => {
                let msg = asst.message.expect("should have message");
                let content = msg.content.expect("should have content");
                assert_eq!(content.len(), 2);
                match &content[0] {
                    ContentBlock::Thinking { thinking } => {
                        assert_eq!(thinking, "Let me analyze this problem.");
                    }
                    other => panic!("expected Thinking block, got {other:?}"),
                }
                match &content[1] {
                    ContentBlock::Text { text } => {
                        assert_eq!(text, "Here is my analysis.");
                    }
                    other => panic!("expected Text block, got {other:?}"),
                }
            }
            other => panic!("expected Assistant record, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_system_record() {
        let json = r#"{
            "type": "system",
            "timestamp": "2026-03-24T05:00:00.000Z",
            "sessionId": "11111111-1111-1111-1111-111111111111",
            "subtype": "compact_boundary"
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::System(sys) => {
                assert!(sys.timestamp.is_some());
                assert_eq!(sys.subtype.as_deref(), Some("compact_boundary"));
                assert_eq!(sys.session_id.as_deref(), Some("11111111-1111-1111-1111-111111111111"));
            }
            other => panic!("expected System record, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_queue_operation_record() {
        let json = r#"{
            "type": "queue-operation",
            "operation": "enqueue",
            "timestamp": "2026-03-24T05:00:00.000Z",
            "sessionId": "11111111-1111-1111-1111-111111111111",
            "content": "user message text"
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::QueueOperation(qo) => {
                assert_eq!(qo.operation.as_deref(), Some("enqueue"));
                assert_eq!(qo.content.as_deref(), Some("user message text"));
                assert_eq!(qo.session_id.as_deref(), Some("11111111-1111-1111-1111-111111111111"));
            }
            other => panic!("expected QueueOperation record, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_pr_link_record() {
        let json = r#"{
            "type": "pr-link",
            "sessionId": "11111111-1111-1111-1111-111111111111",
            "prNumber": 42,
            "prUrl": "https://github.com/org/repo/pull/42",
            "prRepository": "org/repo",
            "timestamp": "2026-03-24T05:00:00.000Z"
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::PrLink(pr) => {
                assert_eq!(pr.pr_number, Some(42));
                assert_eq!(pr.pr_url.as_deref(), Some("https://github.com/org/repo/pull/42"));
                assert_eq!(pr.pr_repository.as_deref(), Some("org/repo"));
            }
            other => panic!("expected PrLink record, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_progress_record() {
        let json = r#"{
            "type": "progress",
            "timestamp": "2026-03-24T05:00:00.000Z",
            "sessionId": "11111111-1111-1111-1111-111111111111"
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::Progress(p) => {
                assert!(p.timestamp.is_some());
                assert_eq!(p.session_id.as_deref(), Some("11111111-1111-1111-1111-111111111111"));
            }
            other => panic!("expected Progress record, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_last_prompt_record() {
        let json = r#"{
            "type": "last-prompt",
            "sessionId": "11111111-1111-1111-1111-111111111111"
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::LastPrompt(lp) => {
                assert_eq!(lp.session_id.as_deref(), Some("11111111-1111-1111-1111-111111111111"));
            }
            other => panic!("expected LastPrompt record, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_file_history_snapshot_record() {
        let json = r#"{
            "type": "file-history-snapshot",
            "sessionId": "11111111-1111-1111-1111-111111111111"
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::FileHistorySnapshot(fhs) => {
                assert_eq!(fhs.session_id.as_deref(), Some("11111111-1111-1111-1111-111111111111"));
            }
            other => panic!("expected FileHistorySnapshot record, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_field_produces_serde_error() {
        let json = r#"{"type": "some_future_type", "data": "hello"}"#;
        let result = serde_json::from_str::<JournalRecord>(json);
        assert!(result.is_err(), "unknown type should produce a serde error");
    }

    #[test]
    fn missing_optional_fields_deserialize_as_none() {
        let json = r#"{
            "type": "user",
            "message": {
                "role": "user",
                "content": "hello"
            }
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect("should deserialize");
        match record {
            JournalRecord::User(user) => {
                assert!(user.uuid.is_none());
                assert!(user.parent_uuid.is_none());
                assert!(user.timestamp.is_none());
                assert!(user.session_id.is_none());
                assert!(user.cwd.is_none());
                assert!(user.git_branch.is_none());
                assert!(user.version.is_none());
                assert!(!user.is_sidechain);
                assert!(!user.is_compact_summary);
            }
            other => panic!("expected User record, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_produces_serde_error() {
        let json = r"{this is not valid json}";
        let result = serde_json::from_str::<JournalRecord>(json);
        assert!(result.is_err(), "malformed JSON should produce a serde error");
    }

    #[test]
    fn empty_string_produces_serde_error() {
        let result = serde_json::from_str::<JournalRecord>("");
        assert!(result.is_err(), "empty string should produce a serde error");
    }

    #[test]
    fn extra_unknown_fields_ignored_by_default() {
        let json = r#"{
            "type": "user",
            "message": {
                "role": "user",
                "content": "hello"
            },
            "some_future_field": "some_value",
            "another_unknown": 42
        }"#;

        let record: JournalRecord = serde_json::from_str(json).expect(
            "extra unknown fields should be ignored (serde default behavior for forward compat)",
        );
        match record {
            JournalRecord::User(user) => {
                let msg = user.message.expect("should have message");
                match msg.content {
                    MessageContent::Text(text) => assert_eq!(text, "hello"),
                    MessageContent::Blocks(_) => panic!("expected Text content"),
                }
            }
            other => panic!("expected User record, got {other:?}"),
        }
    }
}
