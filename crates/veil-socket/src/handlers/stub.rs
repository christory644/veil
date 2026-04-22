//! Stub handlers that return NOT_IMPLEMENTED for unimplemented method groups.
#![allow(dead_code)]

use crate::rpc::RpcOutcome;

/// Return a NOT_IMPLEMENTED error for methods not yet implemented.
///
/// The response has:
/// - `code`: `-32001` (`NOT_IMPLEMENTED`)
/// - `message`: `"Method not yet implemented: <method>"`
/// - `data`: `None`
#[allow(unused_variables)]
pub(crate) fn not_implemented(id: serde_json::Value, method: &str) -> RpcOutcome {
    todo!("implement stub::not_implemented")
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::NOT_IMPLEMENTED;
    use serde_json::json;

    // ── Unit 5: Stub handlers ─────────────────────────────────────────────────

    #[test]
    fn not_implemented_has_correct_code() {
        let outcome = not_implemented(json!(1), "surface.split");
        match outcome {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, NOT_IMPLEMENTED),
            RpcOutcome::Ok(_) => panic!("expected Err variant"),
        }
    }

    #[test]
    fn not_implemented_embeds_method_name() {
        let outcome = not_implemented(json!(1), "session.search");
        match outcome {
            RpcOutcome::Err(e) => {
                assert!(
                    e.error.message.contains("session.search"),
                    "message should contain method name, got: {}",
                    e.error.message
                );
            }
            RpcOutcome::Ok(_) => panic!("expected Err variant"),
        }
    }

    #[test]
    fn not_implemented_is_err_outcome() {
        let outcome = not_implemented(json!(42), "sidebar.set_status");
        assert!(matches!(outcome, RpcOutcome::Err(_)));
    }
}
