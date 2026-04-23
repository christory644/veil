//! OSC 7 parser for working directory reporting.
//!
//! Parses OSC 7 payloads containing `file://` URIs into working directory
//! paths. OSC 7 is the standard mechanism terminals use to report the
//! shell's current working directory.

use std::path::PathBuf;

/// A parsed OSC 7 working directory report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Osc7Report {
    /// The hostname from the URI (empty string if omitted).
    pub hostname: String,
    /// The decoded filesystem path.
    pub path: PathBuf,
}

/// Errors from OSC 7 parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Osc7Error {
    /// The payload is not an OSC 7 sequence.
    #[error("not an OSC 7 sequence")]
    NotOsc7,
    /// The URI scheme is not `file://`.
    #[error("unsupported URI scheme: {scheme}")]
    UnsupportedScheme {
        /// The scheme that was found.
        scheme: String,
    },
    /// The path is empty after decoding.
    #[error("empty path in OSC 7 URI")]
    EmptyPath,
    /// The URI contains invalid percent-encoding.
    #[error("invalid percent-encoding in OSC 7 URI: {detail}")]
    InvalidEncoding {
        /// Description of the encoding error.
        detail: String,
    },
}

/// Parse an OSC 7 payload string into a directory report.
///
/// The `payload` is the content between `\x1b]` and the string terminator.
/// Expected format: `7;file://hostname/path/to/directory`
///
/// The path component is percent-decoded (e.g., `%20` becomes a space).
/// On POSIX, the path is used directly. On Windows, the path is
/// converted from `/C:/Users/...` to `C:\Users\...` format.
pub fn parse_osc7(_payload: &str) -> Result<Osc7Report, Osc7Error> {
    todo!()
}

/// Percent-decode a URI path component.
///
/// Decodes `%XX` sequences where `XX` is a two-digit hex value.
/// Invalid sequences (e.g., `%GG`, truncated `%X`) are returned as errors.
#[allow(dead_code)]
fn percent_decode(_input: &str) -> Result<String, Osc7Error> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ── Happy path ──────────────────────────────────────────────────

    #[test]
    fn parse_with_hostname() {
        let result = parse_osc7("7;file://hostname/Users/chris/project").unwrap();
        assert_eq!(result.hostname, "hostname");
        assert_eq!(result.path, PathBuf::from("/Users/chris/project"));
    }

    #[test]
    fn parse_empty_hostname() {
        let result = parse_osc7("7;file:///tmp/test").unwrap();
        assert_eq!(result.hostname, "");
        assert_eq!(result.path, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn parse_localhost_hostname() {
        let result = parse_osc7("7;file://localhost/home/user").unwrap();
        assert_eq!(result.hostname, "localhost");
        assert_eq!(result.path, PathBuf::from("/home/user"));
    }

    // ── Percent-decoding ────────────────────────────────────────────

    #[test]
    fn decode_spaces() {
        let result = parse_osc7("7;file:///path%20with%20spaces/dir").unwrap();
        assert_eq!(result.path, PathBuf::from("/path with spaces/dir"));
    }

    #[test]
    fn decode_encoded_slashes() {
        let result = parse_osc7("7;file:///path%2Fwith%2Fslashes").unwrap();
        assert_eq!(result.path, PathBuf::from("/path/with/slashes"));
    }

    #[test]
    fn decode_mixed_encoded_and_literal() {
        let result = parse_osc7("7;file:///normal/path%20here").unwrap();
        assert_eq!(result.path, PathBuf::from("/normal/path here"));
    }

    // ── Error cases ─────────────────────────────────────────────────

    #[test]
    fn empty_payload_returns_not_osc7() {
        assert_eq!(parse_osc7(""), Err(Osc7Error::NotOsc7));
    }

    #[test]
    fn wrong_osc_number_returns_not_osc7() {
        assert_eq!(parse_osc7("0;window title"), Err(Osc7Error::NotOsc7));
    }

    #[test]
    fn missing_uri_returns_empty_path() {
        assert_eq!(parse_osc7("7;"), Err(Osc7Error::EmptyPath));
    }

    #[test]
    fn http_scheme_returns_unsupported() {
        assert_eq!(
            parse_osc7("7;http://example.com/path"),
            Err(Osc7Error::UnsupportedScheme { scheme: "http".to_string() })
        );
    }

    #[test]
    fn file_scheme_only_returns_empty_path() {
        assert_eq!(parse_osc7("7;file://"), Err(Osc7Error::EmptyPath));
    }

    #[test]
    fn invalid_percent_hex_returns_encoding_error() {
        let result = parse_osc7("7;file:///path%GG/bad");
        assert!(matches!(result, Err(Osc7Error::InvalidEncoding { .. })));
    }

    #[test]
    fn truncated_percent_returns_encoding_error() {
        let result = parse_osc7("7;file:///path%2");
        assert!(matches!(result, Err(Osc7Error::InvalidEncoding { .. })));
    }

    // ── Property-based ──────────────────────────────────────────────

    proptest! {
        #[test]
        fn round_trip_posix_paths(path in "/[a-zA-Z0-9_/]{1,50}") {
            // Encode the path as a file URI (percent-encoding only spaces
            // for simplicity) and verify it round-trips.
            let uri = format!("7;file://{path}");
            let result = parse_osc7(&uri);
            // A valid POSIX path with only alphanumeric + _ + / should
            // always parse successfully.
            prop_assert!(result.is_ok(), "failed to parse: {:?}", result);
            let report = result.unwrap();
            prop_assert_eq!(report.path, PathBuf::from(&path));
        }

        #[test]
        fn arbitrary_payloads_never_panic(payload in "\\PC*") {
            // We do not care about the result, only that it does not panic.
            let _ = parse_osc7(&payload);
        }
    }
}
