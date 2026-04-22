//! OSC notification parser.
//!
//! Parses OSC 9, OSC 99, and OSC 777 escape sequences from payload strings.
//! The parser operates on the content between `\x1b]` and the string terminator
//! (`\x07` or `\x1b\\`), which is provided by the terminal engine (libghosty).

use crate::notification::OscSequenceType;

/// Parsed notification from an OSC sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OscNotification {
    /// Which OSC sequence type produced this.
    pub sequence_type: OscSequenceType,
    /// Notification title (if the sequence supports it).
    pub title: Option<String>,
    /// Notification body/message.
    pub body: String,
}

/// Errors from OSC notification parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OscParseError {
    /// The payload is not a notification OSC sequence.
    #[error("not a notification OSC sequence")]
    NotNotification,
    /// The payload is malformed.
    #[error("malformed OSC payload: {reason}")]
    Malformed {
        /// Description of why the payload is malformed.
        reason: String,
    },
    /// The notification body is empty.
    #[error("empty notification body")]
    EmptyBody,
}

/// Try to parse an OSC payload string as a notification.
///
/// The `payload` is the content between `\x1b]` and the string terminator
/// (`\x07` or `\x1b\\`). For example, for the sequence `\x1b]9;hello\x07`,
/// the payload is `"9;hello"`.
///
/// Returns `Err(OscParseError::NotNotification)` if the payload is not an
/// OSC 9/99/777 notification sequence. This is the expected "no match" case,
/// not a true error.
pub fn parse_osc_notification(payload: &str) -> Result<OscNotification, OscParseError> {
    if let Some(rest) = payload.strip_prefix("9;") {
        return parse_osc9(rest);
    }
    if let Some(rest) = payload.strip_prefix("99;") {
        return parse_osc99(rest);
    }
    if let Some(rest) = payload.strip_prefix("777;") {
        return parse_osc777(rest);
    }
    Err(OscParseError::NotNotification)
}

/// Parse an OSC 9 (iTerm2/ConEmu) notification payload.
///
/// `rest` is the content after the `"9;"` prefix (i.e., the message body).
fn parse_osc9(body: &str) -> Result<OscNotification, OscParseError> {
    if body.is_empty() {
        return Err(OscParseError::EmptyBody);
    }
    Ok(OscNotification {
        sequence_type: OscSequenceType::Osc9,
        title: None,
        body: body.to_string(),
    })
}

/// Parse an OSC 99 (kitty) notification payload.
///
/// `rest` is the content after the `"99;"` prefix. Expected format:
/// `<params>;<data>` where params are colon-separated key-value pairs
/// (e.g., `i=1:p=title`).
fn parse_osc99(rest: &str) -> Result<OscNotification, OscParseError> {
    let (params, data) = match rest.find(';') {
        Some(pos) => (&rest[..pos], &rest[pos + 1..]),
        None => {
            return Err(OscParseError::Malformed {
                reason: "missing payload separator in OSC 99".to_string(),
            })
        }
    };

    if data.is_empty() {
        return Err(OscParseError::EmptyBody);
    }

    let is_title = params.split(':').any(|kv| kv == "p=title");

    if is_title {
        Ok(OscNotification {
            sequence_type: OscSequenceType::Osc99,
            title: Some(data.to_string()),
            body: String::new(),
        })
    } else {
        Ok(OscNotification {
            sequence_type: OscSequenceType::Osc99,
            title: None,
            body: data.to_string(),
        })
    }
}

/// Parse an OSC 777 (rxvt-unicode) notification payload.
///
/// `rest` is the content after the `"777;"` prefix. Expected format:
/// `notify;<title>;<body>`. The `notify` keyword is required.
fn parse_osc777(rest: &str) -> Result<OscNotification, OscParseError> {
    let mut parts = rest.splitn(3, ';');
    let keyword = parts.next().unwrap_or("");
    if keyword != "notify" {
        return Err(OscParseError::NotNotification);
    }
    let title_str = parts.next().unwrap_or("");
    let body_str = parts.next().unwrap_or("");

    if body_str.is_empty() {
        return Err(OscParseError::EmptyBody);
    }

    let title = if title_str.is_empty() { None } else { Some(title_str.to_string()) };

    Ok(OscNotification {
        sequence_type: OscSequenceType::Osc777,
        title,
        body: body_str.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // ================================================================
    // OSC 9 tests
    // ================================================================

    #[test]
    fn osc9_simple_message() {
        let result = parse_osc_notification("9;hello world").unwrap();
        assert_eq!(result.sequence_type, OscSequenceType::Osc9);
        assert_eq!(result.body, "hello world");
        assert_eq!(result.title, None);
    }

    #[test]
    fn osc9_empty_message() {
        let result = parse_osc_notification("9;");
        assert_eq!(result, Err(OscParseError::EmptyBody));
    }

    #[test]
    fn osc9_special_characters() {
        // Unicode, newlines, semicolons should all be preserved in the body
        let payload = "9;hello\nworld; with unicode \u{1F600} and semicolons;";
        let result = parse_osc_notification(payload).unwrap();
        assert_eq!(result.sequence_type, OscSequenceType::Osc9);
        assert_eq!(result.body, "hello\nworld; with unicode \u{1F600} and semicolons;");
    }

    // ================================================================
    // OSC 99 tests
    // ================================================================

    #[test]
    fn osc99_body_only() {
        let result = parse_osc_notification("99;i=1;hello").unwrap();
        assert_eq!(result.sequence_type, OscSequenceType::Osc99);
        assert_eq!(result.body, "hello");
    }

    #[test]
    fn osc99_with_title_payload() {
        let result = parse_osc_notification("99;i=1:p=title;My Title").unwrap();
        assert_eq!(result.sequence_type, OscSequenceType::Osc99);
        assert_eq!(result.title, Some("My Title".to_string()));
    }

    #[test]
    fn osc99_with_body_payload() {
        let result = parse_osc_notification("99;i=1:p=body;The body text").unwrap();
        assert_eq!(result.sequence_type, OscSequenceType::Osc99);
        assert_eq!(result.body, "The body text");
    }

    #[test]
    fn osc99_empty_payload() {
        let result = parse_osc_notification("99;i=1;");
        assert_eq!(result, Err(OscParseError::EmptyBody));
    }

    #[test]
    fn osc99_missing_id() {
        // No i= parameter, but should still work for one-shot notifications
        let result = parse_osc_notification("99;;hello").unwrap();
        assert_eq!(result.sequence_type, OscSequenceType::Osc99);
        assert_eq!(result.body, "hello");
    }

    // ================================================================
    // OSC 777 tests
    // ================================================================

    #[test]
    fn osc777_with_title_and_body() {
        let result = parse_osc_notification("777;notify;Title;Body").unwrap();
        assert_eq!(result.sequence_type, OscSequenceType::Osc777);
        assert_eq!(result.title, Some("Title".to_string()));
        assert_eq!(result.body, "Body");
    }

    #[test]
    fn osc777_body_only() {
        let result = parse_osc_notification("777;notify;;Body").unwrap();
        assert_eq!(result.sequence_type, OscSequenceType::Osc777);
        assert_eq!(result.title, None);
        assert_eq!(result.body, "Body");
    }

    #[test]
    fn osc777_missing_notify_keyword() {
        let result = parse_osc_notification("777;other;Title;Body");
        assert_eq!(result, Err(OscParseError::NotNotification));
    }

    #[test]
    fn osc777_empty_body() {
        let result = parse_osc_notification("777;notify;Title;");
        assert_eq!(result, Err(OscParseError::EmptyBody));
    }

    // ================================================================
    // Non-notification OSC sequences
    // ================================================================

    #[test]
    fn non_notification_osc() {
        let result = parse_osc_notification("0;window title");
        assert_eq!(result, Err(OscParseError::NotNotification));
    }

    #[test]
    fn osc7_pwd() {
        let result = parse_osc_notification("7;file:///tmp/test");
        assert_eq!(result, Err(OscParseError::NotNotification));
    }

    #[test]
    fn empty_payload() {
        let result = parse_osc_notification("");
        assert_eq!(result, Err(OscParseError::NotNotification));
    }

    #[test]
    fn garbage_payload() {
        let result = parse_osc_notification("not a real sequence");
        assert!(
            result == Err(OscParseError::NotNotification)
                || matches!(result, Err(OscParseError::Malformed { .. })),
            "garbage input should return NotNotification or Malformed, got {result:?}"
        );
    }

    // ================================================================
    // Property-based tests
    // ================================================================

    proptest! {
        #[test]
        fn proptest_osc9_roundtrip(s in "[^\x00]{1,200}") {
            let payload = format!("9;{s}");
            let result = parse_osc_notification(&payload).unwrap();
            prop_assert_eq!(result.body, s);
            prop_assert_eq!(result.sequence_type, OscSequenceType::Osc9);
        }

        #[test]
        fn proptest_no_panic_on_arbitrary_input(s in "\\PC{0,500}") {
            // Should never panic -- may return Ok or Err, but must not crash
            let _ = parse_osc_notification(&s);
        }
    }
}
