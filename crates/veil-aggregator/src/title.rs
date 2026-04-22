//! Title generation for agent conversation sessions.
//!
//! Pure functions for generating meaningful conversation titles from session
//! metadata. Agent-provided titles are preferred when they look meaningful;
//! gibberish/UUID titles fall back to first-message extraction.

/// Generate a display title for a session.
///
/// Priority: agent-provided title (if not gibberish) > heuristic from first
/// message > fallback.
pub fn generate_title(agent_title: Option<&str>, first_user_message: Option<&str>) -> String {
    // 1. Try agent-provided title if it's non-empty and not gibberish.
    if let Some(title) = agent_title {
        let trimmed = title.trim();
        if !trimmed.is_empty() && !is_gibberish_title(trimmed) {
            return trimmed.to_string();
        }
    }

    // 2. Try extracting a topic from the first user message.
    if let Some(msg) = first_user_message {
        let trimmed = msg.trim();
        if !trimmed.is_empty() {
            return extract_topic_from_message(trimmed);
        }
    }

    // 3. Fallback.
    "Untitled session".to_string()
}

/// Returns true if the string looks like a UUID, hash, or other non-meaningful identifier.
fn is_gibberish_title(title: &str) -> bool {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return true;
    }

    // Purely numeric strings are gibberish.
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }

    // UUID pattern: 8-4-4-4-12 hex digits.
    if trimmed.len() == 36
        && trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        && trimmed.chars().filter(|&c| c == '-').count() == 4
    {
        return true;
    }

    // Hex hash: only hex digits, at least 8 chars long (and contains at least
    // one letter so it wasn't caught by the numeric check above).
    if trimmed.len() >= 8 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }

    false
}

/// Extract a topic phrase from the first user message.
///
/// Truncates to reasonable length, strips common prefixes ("please", "can you", etc.).
fn extract_topic_from_message(message: &str) -> String {
    // Take only the first line of the message.
    let first_line = message.lines().next().unwrap_or("").trim();

    // Strip common conversational prefixes (case-insensitive).
    let prefixes: &[&str] = &[
        "please help me ",
        "please can you ",
        "please ",
        "could you please ",
        "could you ",
        "can you please ",
        "can you help me ",
        "can you ",
        "i'd like you to ",
        "i would like you to ",
        "i want you to ",
        "i need you to ",
        "i need help with ",
        "i need help ",
        "help me ",
    ];

    let lower = first_line.to_lowercase();
    let mut result = first_line;
    for prefix in prefixes {
        if lower.starts_with(prefix) {
            result = &first_line[prefix.len()..];
            break;
        }
    }

    let result = result.trim();

    // Truncate to ~80 characters, breaking at a char boundary and preferring
    // a word boundary when possible. We must not slice mid-codepoint or the
    // program will panic on non-ASCII input (emoji, CJK, accented chars).
    let max_len = 80;
    if result.len() <= max_len {
        return result.to_string();
    }

    // Find the last char boundary at or before `max_len` bytes.
    let safe_end = result
        .char_indices()
        .take_while(|(i, _)| *i < max_len)
        .last()
        .map_or(0, |(i, c)| i + c.len_utf8());

    // Find the last space before the safe boundary for a clean word break.
    let truncated = &result[..safe_end];
    if let Some(last_space) = truncated.rfind(' ') {
        format!("{}...", &result[..last_space])
    } else {
        // No space found — hard truncate at the char boundary.
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// generate_title with arbitrary inputs must never panic and must
        /// always return a non-empty string.
        #[test]
        fn generate_title_never_panics_and_returns_nonempty(
            agent_title in proptest::option::of("\\PC{0,200}"),
            first_message in proptest::option::of("\\PC{0,200}"),
        ) {
            let result = generate_title(agent_title.as_deref(), first_message.as_deref());
            prop_assert!(!result.is_empty(), "generate_title must return non-empty string");
        }

        /// is_gibberish_title with arbitrary strings must never panic.
        #[test]
        fn is_gibberish_never_panics(input in "\\PC{0,200}") {
            // Must not panic — true or false are both fine.
            let _ = is_gibberish_title(&input);
        }

        /// extract_topic_from_message with arbitrary strings must never panic
        /// and must return a string with length <= 100 (80 + "..." suffix).
        #[test]
        fn extract_topic_never_panics_and_respects_length_limit(
            input in "\\PC{0,500}"
        ) {
            let result = extract_topic_from_message(&input);
            prop_assert!(
                result.len() <= 100,
                "result length {} exceeds 100 for input of length {}: {:?}",
                result.len(), input.len(), result
            );
        }

        /// extract_topic_from_message with multi-byte Unicode characters
        /// must never panic (no mid-codepoint slicing).
        #[test]
        fn extract_topic_handles_multibyte_unicode(
            prefix in "[\\p{Han}\\p{Hiragana}\\p{Katakana}\\p{Emoji}]{0,50}",
            suffix in "[a-zA-Z ]{0,100}",
        ) {
            let input = format!("{prefix}{suffix}");
            let result = extract_topic_from_message(&input);
            prop_assert!(result.len() <= 100, "result too long: {}", result.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Agent-provided title tests ---

    #[test]
    fn meaningful_agent_title_used_as_is() {
        let result = generate_title(Some("Fix auth middleware"), None);
        assert_eq!(result, "Fix auth middleware");
    }

    #[test]
    fn uuid_style_title_falls_through_to_message() {
        let result = generate_title(
            Some("a1b2c3d4-e5f6-7890-abcd-ef1234567890"),
            Some("Help me fix the login flow"),
        );
        // Should not use the UUID as the title
        assert_ne!(result, "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
        // Should contain something derived from the message
        assert!(!result.is_empty());
    }

    #[test]
    fn hex_hash_title_falls_through_to_message() {
        let result = generate_title(Some("abc123def456"), Some("Refactor the database layer"));
        assert_ne!(result, "abc123def456");
        assert!(!result.is_empty());
    }

    #[test]
    fn purely_numeric_title_falls_through_to_message() {
        let result = generate_title(Some("1234567890"), Some("Add unit tests for parser"));
        assert_ne!(result, "1234567890");
        assert!(!result.is_empty());
    }

    // --- No agent title tests ---

    #[test]
    fn no_agent_title_with_message_extracts_topic() {
        let result = generate_title(None, Some("Help me fix the auth bug in the middleware"));
        assert!(!result.is_empty());
        // The result should be derived from the message, not a generic fallback
        assert_ne!(result, "Untitled session");
    }

    #[test]
    fn no_agent_title_no_message_returns_fallback() {
        let result = generate_title(None, None);
        assert!(!result.is_empty());
        // Should be a generic fallback like "Untitled session"
        assert!(
            result.contains("ntitled") || result.contains("session"),
            "Expected a fallback title, got: {result}"
        );
    }

    // --- Truncation and cleaning tests ---

    #[test]
    fn long_first_message_is_truncated() {
        let long_message = "a".repeat(200);
        let result = generate_title(None, Some(&long_message));
        assert!(!result.is_empty(), "should produce a title from the message");
        assert!(
            result.len() <= 100,
            "Title should be truncated to ~80 chars, got {} chars: {result}",
            result.len()
        );
    }

    #[test]
    fn common_prefixes_stripped_from_message() {
        let result = generate_title(None, Some("Please help me fix the auth bug"));
        assert!(!result.is_empty(), "should produce a title from the message");
        // "Please help me" should be stripped or reduced
        let lower = result.to_lowercase();
        assert!(
            !lower.starts_with("please"),
            "Common prefix 'please' should be stripped, got: {result}"
        );
    }

    // --- Edge cases ---

    #[test]
    fn empty_string_agent_title_treated_as_missing() {
        let result = generate_title(Some(""), Some("Fix the build"));
        // Empty title should be treated as if no title was provided
        // Should extract from message instead
        assert!(!result.is_empty());
        assert_ne!(result, "");
    }

    #[test]
    fn whitespace_only_agent_title_treated_as_missing() {
        let result = generate_title(Some("   "), Some("Fix the build"));
        assert!(!result.is_empty());
        // Should not be just whitespace
        assert_ne!(result.trim(), "");
    }

    #[test]
    fn message_with_only_whitespace_falls_through_to_fallback() {
        let result = generate_title(None, Some("   \n\t  "));
        assert!(!result.is_empty());
        // Should be a fallback, not whitespace
        assert!(
            result.contains("ntitled") || result.contains("session"),
            "Expected fallback for whitespace-only message, got: {result}"
        );
    }

    #[test]
    fn mixed_case_and_punctuation_preserved_in_meaningful_title() {
        let result = generate_title(Some("Fix: Auth-Middleware (JWT)"), None);
        assert_eq!(result, "Fix: Auth-Middleware (JWT)");
    }

    // --- is_gibberish_title tests (indirect, tested through generate_title) ---

    #[test]
    fn is_gibberish_detects_uuid() {
        // Test the internal function directly
        assert!(is_gibberish_title("a1b2c3d4-e5f6-7890-abcd-ef1234567890"));
    }

    #[test]
    fn is_gibberish_detects_hex_hash() {
        assert!(is_gibberish_title("abc123def456"));
    }

    #[test]
    fn is_gibberish_detects_numeric_string() {
        assert!(is_gibberish_title("1234567890"));
    }

    #[test]
    fn is_gibberish_allows_meaningful_title() {
        assert!(!is_gibberish_title("Fix auth middleware"));
    }

    #[test]
    fn is_gibberish_allows_title_with_numbers() {
        assert!(!is_gibberish_title("Fix bug #42 in auth"));
    }

    // --- extract_topic_from_message tests ---

    #[test]
    fn extract_topic_strips_please_prefix() {
        let result = extract_topic_from_message("Please help me fix the auth bug");
        let lower = result.to_lowercase();
        assert!(!lower.starts_with("please"), "Should strip 'please' prefix, got: {result}");
    }

    #[test]
    fn extract_topic_strips_can_you_prefix() {
        let result = extract_topic_from_message("Can you help me fix the auth bug");
        let lower = result.to_lowercase();
        assert!(!lower.starts_with("can you"), "Should strip 'can you' prefix, got: {result}");
    }

    #[test]
    fn extract_topic_truncates_long_message() {
        let long = "a".repeat(200);
        let result = extract_topic_from_message(&long);
        assert!(result.len() <= 100, "Should truncate to ~80 chars, got {} chars", result.len());
    }
}
