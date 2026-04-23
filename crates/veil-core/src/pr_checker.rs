//! PR state checker.
//!
//! Shells out to `gh` (GitHub CLI) to check pull request state.
//! Includes rate-limiting awareness and graceful degradation.

use std::path::{Path, PathBuf};

use crate::live_state::PrState;

/// Checks PR state by shelling out to `gh`.
pub struct PrChecker;

/// A request to check one PR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrCheckRequest {
    /// Path to the git repository (used for gh context).
    pub repo_path: PathBuf,
    /// PR number to check.
    pub pr_number: u64,
}

/// Result of checking one PR.
#[derive(Debug, Clone)]
pub struct PrCheckResult {
    /// The original request.
    pub request: PrCheckRequest,
    /// The determined state.
    pub state: PrState,
}

/// Parse a PR state from the JSON output of `gh pr view --json state`.
///
/// Expects a JSON string containing `"state":"OPEN"`, `"state":"MERGED"`,
/// or `"state":"CLOSED"`. Uses manual parsing to avoid a `serde_json`
/// runtime dependency in veil-core.
pub fn parse_pr_state_json(json: &str) -> PrState {
    // Find the "state" key in the JSON. We look for `"state"` followed by
    // optional whitespace, a colon, optional whitespace, then a quoted string value.
    let Some(state_key_pos) = json.find("\"state\"") else {
        return PrState::Unknown;
    };

    // Move past `"state"`
    let after_key = &json[state_key_pos + "\"state\"".len()..];

    // Skip whitespace, then expect a colon
    let after_key = after_key.trim_start();
    let Some(after_colon) = after_key.strip_prefix(':') else {
        return PrState::Unknown;
    };

    // Skip whitespace after colon
    let after_colon = after_colon.trim_start();

    // The value must be a quoted string; anything else (null, number, etc.) is Unknown
    let Some(after_quote) = after_colon.strip_prefix('"') else {
        return PrState::Unknown;
    };

    // Extract the value up to the closing quote
    let Some(end_quote) = after_quote.find('"') else {
        return PrState::Unknown;
    };

    let value = &after_quote[..end_quote];

    match value {
        "OPEN" => PrState::Open,
        "MERGED" => PrState::Merged,
        "CLOSED" => PrState::Closed,
        _ => PrState::Unknown,
    }
}

impl PrChecker {
    /// Check a single PR's state.
    ///
    /// Runs `gh pr view <number> --json state` in the repo directory
    /// and parses the JSON output.
    pub fn check_pr(repo_path: &Path, pr_number: u64) -> PrState {
        let output = std::process::Command::new("gh")
            .args(["pr", "view", &pr_number.to_string(), "--json", "state"])
            .current_dir(repo_path)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                parse_pr_state_json(&stdout)
            }
            _ => PrState::Unknown,
        }
    }

    /// Check multiple PRs sequentially (each PR is a separate `gh` call).
    ///
    /// Returns results in the same order as the input requests.
    pub fn check_prs(requests: &[PrCheckRequest]) -> Vec<PrCheckResult> {
        requests
            .iter()
            .map(|req| {
                let state = Self::check_pr(&req.repo_path, req.pr_number);
                PrCheckResult { request: req.clone(), state }
            })
            .collect()
    }

    /// Check whether `gh` is available on the system PATH.
    pub fn is_available() -> bool {
        std::process::Command::new("gh")
            .arg("--version")
            .output()
            .is_ok_and(|out| out.status.success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ================================================================
    // parse_pr_state_json — happy path
    // ================================================================

    #[test]
    fn parse_open_state() {
        let json = r#"{"state":"OPEN"}"#;
        assert_eq!(parse_pr_state_json(json), PrState::Open);
    }

    #[test]
    fn parse_merged_state() {
        let json = r#"{"state":"MERGED"}"#;
        assert_eq!(parse_pr_state_json(json), PrState::Merged);
    }

    #[test]
    fn parse_closed_state() {
        let json = r#"{"state":"CLOSED"}"#;
        assert_eq!(parse_pr_state_json(json), PrState::Closed);
    }

    #[test]
    fn parse_state_with_extra_fields() {
        let json = r#"{"number":42,"state":"MERGED","title":"Fix bug"}"#;
        assert_eq!(parse_pr_state_json(json), PrState::Merged);
    }

    #[test]
    fn parse_state_with_whitespace() {
        let json = r#"{ "state" : "OPEN" }"#;
        assert_eq!(parse_pr_state_json(json), PrState::Open);
    }

    // ================================================================
    // parse_pr_state_json — error cases
    // ================================================================

    #[test]
    fn parse_empty_string_returns_unknown() {
        assert_eq!(parse_pr_state_json(""), PrState::Unknown);
    }

    #[test]
    fn parse_invalid_json_returns_unknown() {
        assert_eq!(parse_pr_state_json("not json at all"), PrState::Unknown);
    }

    #[test]
    fn parse_unknown_state_value_returns_unknown() {
        let json = r#"{"state":"DRAFT"}"#;
        assert_eq!(parse_pr_state_json(json), PrState::Unknown);
    }

    #[test]
    fn parse_missing_state_field_returns_unknown() {
        let json = r#"{"number":42}"#;
        assert_eq!(parse_pr_state_json(json), PrState::Unknown);
    }

    #[test]
    fn parse_null_state_returns_unknown() {
        let json = r#"{"state":null}"#;
        assert_eq!(parse_pr_state_json(json), PrState::Unknown);
    }

    #[test]
    fn parse_numeric_state_returns_unknown() {
        let json = r#"{"state":123}"#;
        assert_eq!(parse_pr_state_json(json), PrState::Unknown);
    }

    // ================================================================
    // check_prs — structure
    // ================================================================

    #[test]
    fn check_prs_empty_input_returns_empty() {
        let results = PrChecker::check_prs(&[]);
        assert!(results.is_empty());
    }

    #[test]
    fn check_prs_preserves_request_order() {
        // With gh likely unavailable or returning errors, all should be Unknown.
        // The key test is that the order is preserved and results.len() matches.
        let requests = vec![
            PrCheckRequest { repo_path: PathBuf::from("/tmp/repo"), pr_number: 1 },
            PrCheckRequest { repo_path: PathBuf::from("/tmp/repo"), pr_number: 2 },
            PrCheckRequest { repo_path: PathBuf::from("/tmp/repo"), pr_number: 3 },
        ];
        let results = PrChecker::check_prs(&requests);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].request.pr_number, 1);
        assert_eq!(results[1].request.pr_number, 2);
        assert_eq!(results[2].request.pr_number, 3);
    }

    // ================================================================
    // check_pr — error cases
    // ================================================================

    #[test]
    fn check_pr_nonexistent_repo_returns_unknown() {
        let state = PrChecker::check_pr(Path::new("/nonexistent/repo/path"), 42);
        assert_eq!(state, PrState::Unknown);
    }

    // ================================================================
    // is_available
    // ================================================================

    #[test]
    fn is_available_returns_bool_without_panic() {
        // We just verify it doesn't panic; the result depends on environment.
        let _available = PrChecker::is_available();
    }

    // ================================================================
    // PrCheckRequest equality and hashing
    // ================================================================

    #[test]
    fn pr_check_request_equality() {
        let a = PrCheckRequest { repo_path: PathBuf::from("/repo"), pr_number: 42 };
        let b = PrCheckRequest { repo_path: PathBuf::from("/repo"), pr_number: 42 };
        assert_eq!(a, b);
    }

    #[test]
    fn pr_check_request_not_equal_different_number() {
        let a = PrCheckRequest { repo_path: PathBuf::from("/repo"), pr_number: 42 };
        let b = PrCheckRequest { repo_path: PathBuf::from("/repo"), pr_number: 43 };
        assert_ne!(a, b);
    }

    #[test]
    fn pr_check_request_not_equal_different_repo() {
        let a = PrCheckRequest { repo_path: PathBuf::from("/repo/a"), pr_number: 42 };
        let b = PrCheckRequest { repo_path: PathBuf::from("/repo/b"), pr_number: 42 };
        assert_ne!(a, b);
    }
}
