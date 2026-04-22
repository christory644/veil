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
    // Stub: always returns fallback — tests will fail because the logic isn't implemented.
    let _ = agent_title;
    let _ = first_user_message;
    String::new()
}

/// Returns true if the string looks like a UUID, hash, or other non-meaningful identifier.
fn is_gibberish_title(title: &str) -> bool {
    // Stub: always returns false — tests will fail.
    let _ = title;
    false
}

/// Extract a topic phrase from the first user message.
///
/// Truncates to reasonable length, strips common prefixes ("please", "can you", etc.).
fn extract_topic_from_message(message: &str) -> String {
    // Stub: returns the message unchanged — tests will fail.
    message.to_string()
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
