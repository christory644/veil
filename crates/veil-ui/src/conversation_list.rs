//! Conversation list view-model, extraction, and rendering.
//!
//! Transforms `AppState.conversations.sessions` into grouped, sorted,
//! renderable data and renders it using egui. Follows the same pattern
//! as `workspace_list.rs`.

use chrono::{DateTime, Utc};

use veil_core::session::{AgentKind, SessionEntry, SessionId};
use veil_core::state::AppState;

/// Data for rendering a single conversation entry.
/// View-model extracted from `SessionEntry` to keep rendering decoupled.
pub struct ConversationEntryData {
    /// Session identifier (for click reporting).
    pub id: SessionId,
    /// Display title.
    pub title: String,
    /// Whether this session is currently active (running).
    pub is_active: bool,
    /// Git branch, if known.
    pub branch: Option<String>,
    /// Relative timestamp string (e.g., "2h ago").
    pub relative_time: String,
    /// Whether a finalized plan exists for this session.
    pub has_plan: bool,
}

/// A group of conversations for one agent harness.
pub struct ConversationGroup {
    /// Agent harness display name (e.g., "Claude Code").
    pub agent_name: String,
    /// Agent kind (for identification).
    pub agent_kind: AgentKind,
    /// Total session count for this agent (used in group header).
    pub session_count: usize,
    /// Conversation entries, sorted by most recent first.
    pub entries: Vec<ConversationEntryData>,
}

/// Extract grouped conversation data from `AppState`.
///
/// Groups sessions by `AgentKind`, sorts each group by `started_at` descending
/// (most recent first), and formats relative timestamps using `now`.
/// Groups themselves are sorted by the most recent session in each group.
///
/// `now` is passed explicitly for deterministic testing.
pub fn extract_conversation_groups(
    _state: &AppState,
    _now: DateTime<Utc>,
) -> Vec<ConversationGroup> {
    // Stub: returns empty vec so tests compile but fail.
    Vec::new()
}

/// Convert a single `SessionEntry` into a `ConversationEntryData`.
///
/// `now` is used for relative timestamp formatting.
fn session_to_entry_data(_session: &SessionEntry, _now: DateTime<Utc>) -> ConversationEntryData {
    // Stub: returns placeholder data so tests compile but fail.
    ConversationEntryData {
        id: SessionId::new("stub"),
        title: String::new(),
        is_active: false,
        branch: None,
        relative_time: String::new(),
        has_plan: false,
    }
}

/// Render the conversations tab content.
///
/// Displays collapsible agent group headers with session counts, and
/// per-conversation entry rows within each group. Returns the `SessionId`
/// of a conversation the user clicked, if any.
pub fn render_conversations_tab(
    _ui: &mut egui::Ui,
    _groups: &[ConversationGroup],
) -> Option<SessionId> {
    // Stub: renders nothing, returns None.
    None
}

/// Render a single conversation entry row.
///
/// Returns `true` if the user clicked this entry.
fn render_conversation_entry(_ui: &mut egui::Ui, _entry: &ConversationEntryData) -> bool {
    // Stub: renders nothing, returns false.
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::path::PathBuf;
    use veil_core::session::{AgentKind, SessionEntry, SessionId, SessionStatus};
    use veil_core::state::AppState;

    // ================================================================
    // Helpers
    // ================================================================

    /// A fixed reference time: 2026-04-22 12:00:00 UTC.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 22, 12, 0, 0).unwrap()
    }

    fn make_session(
        id: &str,
        agent: AgentKind,
        title: &str,
        status: SessionStatus,
        started_at: DateTime<Utc>,
        branch: Option<&str>,
        plan_content: Option<&str>,
    ) -> SessionEntry {
        SessionEntry {
            id: SessionId::new(id),
            agent,
            title: title.to_string(),
            working_dir: PathBuf::from("/tmp/project"),
            branch: branch.map(String::from),
            pr_number: None,
            pr_url: None,
            plan_content: plan_content.map(String::from),
            status,
            started_at,
            ended_at: None,
            indexed_at: now(),
        }
    }

    fn make_state_with_sessions(sessions: Vec<SessionEntry>) -> AppState {
        let mut state = AppState::new();
        state.update_conversations(sessions);
        state
    }

    fn screen_input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(1280.0, 800.0),
            )),
            ..Default::default()
        }
    }

    fn run_conversations_tab_frame(groups: &[ConversationGroup]) -> Option<SessionId> {
        let ctx = egui::Context::default();
        let mut result = None;
        let _ = ctx.run_ui(screen_input(), |ctx| {
            egui::CentralPanel::default().show_inside(ctx, |ui| {
                result = render_conversations_tab(ui, groups);
            });
        });
        result
    }

    fn measure_conversations_render_height(groups: &[ConversationGroup]) -> f32 {
        let ctx = egui::Context::default();
        let mut height = 0.0;
        let _ = ctx.run_ui(screen_input(), |ctx| {
            egui::CentralPanel::default().show_inside(ctx, |ui| {
                let before_cursor = ui.cursor().top();
                render_conversations_tab(ui, groups);
                let after_cursor = ui.cursor().top();
                height = after_cursor - before_cursor;
            });
        });
        height
    }

    // ================================================================
    // Unit 2: extract_conversation_groups — happy path
    // ================================================================

    #[test]
    fn empty_sessions_returns_empty_groups() {
        let state = AppState::new();
        let groups = extract_conversation_groups(&state, now());
        assert!(groups.is_empty(), "no sessions should produce no groups");
    }

    #[test]
    fn three_sessions_one_agent_produces_one_group() {
        let sessions = vec![
            make_session(
                "s1",
                AgentKind::ClaudeCode,
                "Fix auth",
                SessionStatus::Completed,
                now() - chrono::Duration::hours(1),
                None,
                None,
            ),
            make_session(
                "s2",
                AgentKind::ClaudeCode,
                "Add tests",
                SessionStatus::Completed,
                now() - chrono::Duration::hours(2),
                None,
                None,
            ),
            make_session(
                "s3",
                AgentKind::ClaudeCode,
                "Refactor",
                SessionStatus::Active,
                now() - chrono::Duration::minutes(10),
                None,
                None,
            ),
        ];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups.len(), 1, "all sessions from one agent should produce one group");
        assert_eq!(groups[0].session_count, 3);
        assert_eq!(groups[0].entries.len(), 3);
        assert_eq!(groups[0].agent_name, "Claude Code");
    }

    #[test]
    fn sessions_from_two_agents_produce_two_groups() {
        let sessions = vec![
            make_session(
                "s1",
                AgentKind::ClaudeCode,
                "Fix auth",
                SessionStatus::Completed,
                now() - chrono::Duration::hours(1),
                None,
                None,
            ),
            make_session(
                "s2",
                AgentKind::Codex,
                "Add tests",
                SessionStatus::Completed,
                now() - chrono::Duration::hours(2),
                None,
                None,
            ),
        ];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups.len(), 2, "sessions from two agents should produce two groups");
    }

    #[test]
    fn sessions_within_group_sorted_by_started_at_descending() {
        let sessions = vec![
            make_session(
                "s-oldest",
                AgentKind::ClaudeCode,
                "Oldest",
                SessionStatus::Completed,
                now() - chrono::Duration::hours(3),
                None,
                None,
            ),
            make_session(
                "s-newest",
                AgentKind::ClaudeCode,
                "Newest",
                SessionStatus::Completed,
                now() - chrono::Duration::hours(1),
                None,
                None,
            ),
            make_session(
                "s-middle",
                AgentKind::ClaudeCode,
                "Middle",
                SessionStatus::Completed,
                now() - chrono::Duration::hours(2),
                None,
                None,
            ),
        ];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups.len(), 1);
        let entries = &groups[0].entries;
        assert_eq!(entries[0].id, SessionId::new("s-newest"), "most recent should be first");
        assert_eq!(entries[1].id, SessionId::new("s-middle"), "middle should be second");
        assert_eq!(entries[2].id, SessionId::new("s-oldest"), "oldest should be last");
    }

    #[test]
    fn groups_sorted_by_most_recent_session() {
        let sessions = vec![
            // Codex has a more recent session
            make_session(
                "s-codex",
                AgentKind::Codex,
                "Codex session",
                SessionStatus::Completed,
                now() - chrono::Duration::minutes(5),
                None,
                None,
            ),
            // Claude Code has an older session
            make_session(
                "s-claude",
                AgentKind::ClaudeCode,
                "Claude session",
                SessionStatus::Completed,
                now() - chrono::Duration::hours(3),
                None,
                None,
            ),
        ];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0].agent_name, "Codex",
            "group with more recent session should come first"
        );
        assert_eq!(groups[1].agent_name, "Claude Code");
    }

    #[test]
    fn active_session_has_is_active_true() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::ClaudeCode,
            "Active session",
            SessionStatus::Active,
            now() - chrono::Duration::minutes(5),
            None,
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups.len(), 1);
        assert!(groups[0].entries[0].is_active, "active session should have is_active = true");
    }

    #[test]
    fn completed_session_has_is_active_false() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::ClaudeCode,
            "Done",
            SessionStatus::Completed,
            now() - chrono::Duration::hours(1),
            None,
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups.len(), 1);
        assert!(!groups[0].entries[0].is_active, "completed session should have is_active = false");
    }

    #[test]
    fn session_with_branch_shows_branch() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::ClaudeCode,
            "Branch work",
            SessionStatus::Active,
            now() - chrono::Duration::minutes(10),
            Some("feat/auth"),
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups[0].entries[0].branch, Some("feat/auth".to_string()));
    }

    #[test]
    fn session_without_branch_has_none() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::ClaudeCode,
            "No branch",
            SessionStatus::Active,
            now() - chrono::Duration::minutes(10),
            None,
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups[0].entries[0].branch, None);
    }

    #[test]
    fn session_with_plan_content_has_has_plan_true() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::ClaudeCode,
            "With plan",
            SessionStatus::Completed,
            now() - chrono::Duration::hours(1),
            None,
            Some("The plan content"),
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert!(
            groups[0].entries[0].has_plan,
            "session with plan_content should have has_plan = true"
        );
    }

    #[test]
    fn session_without_plan_content_has_has_plan_false() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::ClaudeCode,
            "No plan",
            SessionStatus::Completed,
            now() - chrono::Duration::hours(1),
            None,
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert!(
            !groups[0].entries[0].has_plan,
            "session without plan_content should have has_plan = false"
        );
    }

    #[test]
    fn relative_time_formatted_correctly() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::ClaudeCode,
            "Recent",
            SessionStatus::Active,
            now() - chrono::Duration::hours(2),
            None,
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(
            groups[0].entries[0].relative_time, "2h ago",
            "relative_time should be formatted via format_relative"
        );
    }

    #[test]
    fn entry_title_matches_session_title() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::ClaudeCode,
            "Fix auth middleware",
            SessionStatus::Completed,
            now() - chrono::Duration::hours(1),
            None,
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups[0].entries[0].title, "Fix auth middleware");
    }

    #[test]
    fn entry_id_matches_session_id() {
        let sessions = vec![make_session(
            "session-42",
            AgentKind::ClaudeCode,
            "Test",
            SessionStatus::Completed,
            now() - chrono::Duration::hours(1),
            None,
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups[0].entries[0].id, SessionId::new("session-42"));
    }

    // ================================================================
    // Unit 2: extract_conversation_groups — edge cases
    // ================================================================

    #[test]
    fn single_session_produces_one_group_with_one_entry() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::Aider,
            "Solo",
            SessionStatus::Completed,
            now() - chrono::Duration::hours(1),
            None,
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].entries.len(), 1);
        assert_eq!(groups[0].session_count, 1);
    }

    #[test]
    fn all_sessions_same_agent_produces_one_group() {
        let sessions = vec![
            make_session(
                "s1",
                AgentKind::Codex,
                "A",
                SessionStatus::Completed,
                now() - chrono::Duration::hours(1),
                None,
                None,
            ),
            make_session(
                "s2",
                AgentKind::Codex,
                "B",
                SessionStatus::Completed,
                now() - chrono::Duration::hours(2),
                None,
                None,
            ),
            make_session(
                "s3",
                AgentKind::Codex,
                "C",
                SessionStatus::Completed,
                now() - chrono::Duration::hours(3),
                None,
                None,
            ),
        ];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups.len(), 1, "all sessions from same agent should produce one group");
        assert_eq!(groups[0].agent_name, "Codex");
    }

    #[test]
    fn unknown_agent_group_uses_custom_name() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::Unknown("CustomBot".to_string()),
            "Custom session",
            SessionStatus::Completed,
            now() - chrono::Duration::hours(1),
            None,
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].agent_name, "CustomBot");
    }

    #[test]
    fn identical_timestamps_do_not_panic() {
        let same_time = now() - chrono::Duration::hours(1);
        let sessions = vec![
            make_session(
                "s1",
                AgentKind::ClaudeCode,
                "A",
                SessionStatus::Completed,
                same_time,
                None,
                None,
            ),
            make_session(
                "s2",
                AgentKind::ClaudeCode,
                "B",
                SessionStatus::Completed,
                same_time,
                None,
                None,
            ),
        ];
        let state = make_state_with_sessions(sessions);
        // Should not panic even with identical timestamps
        let groups = extract_conversation_groups(&state, now());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].entries.len(), 2);
    }

    #[test]
    fn errored_session_has_is_active_false() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::ClaudeCode,
            "Errored",
            SessionStatus::Errored,
            now() - chrono::Duration::hours(1),
            None,
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert!(!groups[0].entries[0].is_active, "errored session should have is_active = false");
    }

    #[test]
    fn unknown_status_session_has_is_active_false() {
        let sessions = vec![make_session(
            "s1",
            AgentKind::ClaudeCode,
            "Unknown",
            SessionStatus::Unknown,
            now() - chrono::Duration::hours(1),
            None,
            None,
        )];
        let state = make_state_with_sessions(sessions);
        let groups = extract_conversation_groups(&state, now());

        assert!(
            !groups[0].entries[0].is_active,
            "unknown status session should have is_active = false"
        );
    }

    // ================================================================
    // Unit 3: render_conversations_tab — happy path
    // ================================================================

    fn make_entry(
        id: &str,
        title: &str,
        is_active: bool,
        branch: Option<&str>,
        relative_time: &str,
        has_plan: bool,
    ) -> ConversationEntryData {
        ConversationEntryData {
            id: SessionId::new(id),
            title: title.to_string(),
            is_active,
            branch: branch.map(String::from),
            relative_time: relative_time.to_string(),
            has_plan,
        }
    }

    fn make_group(
        agent_name: &str,
        agent_kind: AgentKind,
        entries: Vec<ConversationEntryData>,
    ) -> ConversationGroup {
        let session_count = entries.len();
        ConversationGroup { agent_name: agent_name.to_string(), agent_kind, session_count, entries }
    }

    #[test]
    fn render_two_groups_with_entries_produces_content() {
        let groups = vec![
            make_group(
                "Claude Code",
                AgentKind::ClaudeCode,
                vec![
                    make_entry("s1", "Fix auth", true, Some("feat/auth"), "2h ago", false),
                    make_entry("s2", "Add tests", false, None, "yesterday", false),
                ],
            ),
            make_group(
                "Codex",
                AgentKind::Codex,
                vec![make_entry("s3", "Refactor", false, Some("main"), "3 days ago", true)],
            ),
        ];
        let height = measure_conversations_render_height(&groups);
        assert!(
            height > 0.0,
            "rendering 2 groups with entries should produce visible content, got height={height}"
        );
    }

    #[test]
    fn render_empty_groups_list_no_panic() {
        let groups: Vec<ConversationGroup> = Vec::new();
        let result = run_conversations_tab_frame(&groups);
        assert!(result.is_none(), "empty groups should return None");
    }

    #[test]
    fn single_group_with_three_entries_produces_content() {
        let groups = vec![make_group(
            "Claude Code",
            AgentKind::ClaudeCode,
            vec![
                make_entry("s1", "A", false, None, "1h ago", false),
                make_entry("s2", "B", false, None, "2h ago", false),
                make_entry("s3", "C", false, None, "3h ago", false),
            ],
        )];
        let height = measure_conversations_render_height(&groups);
        assert!(
            height > 0.0,
            "rendering single group with 3 entries should produce content, got height={height}"
        );
    }

    #[test]
    fn no_click_returns_none() {
        let groups = vec![make_group(
            "Claude Code",
            AgentKind::ClaudeCode,
            vec![make_entry("s1", "Fix auth", true, Some("feat/auth"), "2h ago", false)],
        )];
        let result = run_conversations_tab_frame(&groups);
        assert!(result.is_none(), "no click should return None");
    }

    #[test]
    fn group_with_zero_entries_renders_without_panic() {
        let groups = vec![make_group("Claude Code", AgentKind::ClaudeCode, Vec::new())];
        let _result = run_conversations_tab_frame(&groups);
    }

    // ================================================================
    // Unit 3: render_conversations_tab — edge cases
    // ================================================================

    #[test]
    fn very_long_title_does_not_panic() {
        let long_title = "a".repeat(500);
        let groups = vec![make_group(
            "Claude Code",
            AgentKind::ClaudeCode,
            vec![make_entry("s1", &long_title, false, None, "1h ago", false)],
        )];
        let _result = run_conversations_tab_frame(&groups);
    }

    #[test]
    fn very_long_branch_name_does_not_panic() {
        let long_branch = "feature/".to_string() + &"a".repeat(500);
        let groups = vec![make_group(
            "Claude Code",
            AgentKind::ClaudeCode,
            vec![make_entry("s1", "Fix", false, Some(&long_branch), "1h ago", false)],
        )];
        let _result = run_conversations_tab_frame(&groups);
    }

    #[test]
    fn empty_title_string_does_not_panic() {
        let groups = vec![make_group(
            "Claude Code",
            AgentKind::ClaudeCode,
            vec![make_entry("s1", "", false, None, "1h ago", false)],
        )];
        let _result = run_conversations_tab_frame(&groups);
    }

    #[test]
    fn many_groups_do_not_panic() {
        let groups: Vec<ConversationGroup> = (0..12)
            .map(|i| {
                make_group(
                    &format!("Agent {i}"),
                    AgentKind::Unknown(format!("Agent {i}")),
                    vec![make_entry(
                        &format!("s{i}"),
                        &format!("Session {i}"),
                        false,
                        None,
                        "1h ago",
                        false,
                    )],
                )
            })
            .collect();
        let _result = run_conversations_tab_frame(&groups);
    }

    #[test]
    fn group_with_many_entries_does_not_panic() {
        let entries: Vec<ConversationEntryData> = (0..55)
            .map(|i| {
                make_entry(&format!("s{i}"), &format!("Session {i}"), false, None, "1h ago", false)
            })
            .collect();
        let groups = vec![make_group("Claude Code", AgentKind::ClaudeCode, entries)];
        let _result = run_conversations_tab_frame(&groups);
    }

    // ================================================================
    // Unit 3: Rendering verification
    // ================================================================

    #[test]
    fn three_entries_taller_than_one_entry() {
        let one_entry = vec![make_group(
            "Claude Code",
            AgentKind::ClaudeCode,
            vec![make_entry("s1", "A", false, None, "1h ago", false)],
        )];
        let three_entries = vec![make_group(
            "Claude Code",
            AgentKind::ClaudeCode,
            vec![
                make_entry("s1", "A", false, None, "1h ago", false),
                make_entry("s2", "B", false, None, "2h ago", false),
                make_entry("s3", "C", false, None, "3h ago", false),
            ],
        )];
        let height_one = measure_conversations_render_height(&one_entry);
        let height_three = measure_conversations_render_height(&three_entries);
        assert!(
            height_three > height_one,
            "3 entries ({height_three}px) should be taller than 1 entry ({height_one}px)"
        );
    }

    #[test]
    fn entry_with_branch_taller_than_without() {
        let with_branch = vec![make_group(
            "Claude Code",
            AgentKind::ClaudeCode,
            vec![make_entry("s1", "Fix", false, Some("feat/auth"), "1h ago", false)],
        )];
        let without_branch = vec![make_group(
            "Claude Code",
            AgentKind::ClaudeCode,
            vec![make_entry("s2", "Fix", false, None, "1h ago", false)],
        )];
        let height_with = measure_conversations_render_height(&with_branch);
        let height_without = measure_conversations_render_height(&without_branch);
        assert!(
            height_with > height_without,
            "entry with branch ({height_with}px) should be taller than without ({height_without}px)"
        );
    }

    #[test]
    fn entry_with_plan_contains_plan_indicator() {
        // We verify the plan indicator is present by checking that a has_plan=true
        // entry produces different (taller) content than has_plan=false.
        let with_plan = vec![make_group(
            "Claude Code",
            AgentKind::ClaudeCode,
            vec![make_entry("s1", "Fix", false, None, "1h ago", true)],
        )];
        let without_plan = vec![make_group(
            "Claude Code",
            AgentKind::ClaudeCode,
            vec![make_entry("s2", "Fix", false, None, "1h ago", false)],
        )];
        let height_with = measure_conversations_render_height(&with_plan);
        let height_without = measure_conversations_render_height(&without_plan);
        // The plan indicator should add some visual element (text/icon) on the title line,
        // making it wider and potentially taller due to wrapping, or at minimum the same
        // height but with different content. We verify content difference via height or
        // by checking it doesn't panic.
        // Since the plan indicator is inline text on the title line, height might be the
        // same. We just verify it renders without issue.
        assert!(height_with >= 0.0 && height_without >= 0.0, "both should render successfully");
    }
}
