//! Method dispatcher — routes parsed JSON-RPC requests to handler functions.
#![allow(dead_code)]

use std::sync::Arc;
use tokio::sync::Mutex;
use veil_core::state::AppState;

use crate::rpc::{Request, RpcOutcome};

/// The central request dispatcher.
///
/// Holds shared state and routes requests to the appropriate handler.
pub struct Dispatcher {
    state: Arc<Mutex<AppState>>,
}

impl Dispatcher {
    /// Create a new dispatcher over the given shared state.
    #[allow(unused_variables)]
    pub fn new(state: Arc<Mutex<AppState>>) -> Self {
        todo!("implement Dispatcher::new")
    }

    /// Dispatch a parsed request and return the outcome.
    ///
    /// Returns `None` for notifications (requests with no `id`).
    #[allow(unused_variables, clippy::unused_self)]
    pub async fn dispatch(&self, request: Request) -> Option<RpcOutcome> {
        todo!("implement Dispatcher::dispatch")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::{METHOD_NOT_FOUND, NOT_IMPLEMENTED};
    use serde_json::json;

    fn make_state() -> Arc<Mutex<AppState>> {
        Arc::new(Mutex::new(AppState::new()))
    }

    fn make_request(method: &str, id: serde_json::Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: json!({}),
            id: Some(id),
        }
    }

    fn notification_request(method: &str) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: json!({}),
            id: None,
        }
    }

    // ── Unit 3: Method dispatcher ─────────────────────────────────────────────

    #[tokio::test]
    async fn dispatch_unknown_method_returns_method_not_found() {
        let dispatcher = Dispatcher::new(make_state());
        let req = make_request("foo.bar", json!(1));
        let outcome = dispatcher.dispatch(req).await.expect("should return outcome");
        match outcome {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, METHOD_NOT_FOUND),
            RpcOutcome::Ok(_) => panic!("expected Err outcome for unknown method"),
        }
    }

    #[tokio::test]
    async fn dispatch_notification_returns_none() {
        let dispatcher = Dispatcher::new(make_state());
        let req = notification_request("workspace.list");
        let outcome = dispatcher.dispatch(req).await;
        assert!(outcome.is_none(), "notifications should produce no response");
    }

    #[tokio::test]
    async fn dispatch_workspace_list_routes_to_handler() {
        let dispatcher = Dispatcher::new(make_state());
        let req = make_request("workspace.list", json!(1));
        let outcome = dispatcher.dispatch(req).await.expect("should return outcome");
        match outcome {
            RpcOutcome::Ok(_) => {}
            RpcOutcome::Err(e) => {
                panic!("expected Ok outcome for workspace.list, got error: {:?}", e.error)
            }
        }
    }

    #[tokio::test]
    async fn dispatch_surface_method_returns_not_implemented() {
        let dispatcher = Dispatcher::new(make_state());
        let req = make_request("surface.split", json!(1));
        let outcome = dispatcher.dispatch(req).await.expect("should return outcome");
        match outcome {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, NOT_IMPLEMENTED),
            RpcOutcome::Ok(_) => panic!("expected Err outcome for surface.split"),
        }
    }

    #[tokio::test]
    async fn dispatch_notification_method_returns_not_implemented() {
        let dispatcher = Dispatcher::new(make_state());
        let req = make_request("notification.create", json!(1));
        let outcome = dispatcher.dispatch(req).await.expect("should return outcome");
        match outcome {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, NOT_IMPLEMENTED),
            RpcOutcome::Ok(_) => panic!("expected Err outcome for notification.create"),
        }
    }

    #[tokio::test]
    async fn dispatch_session_method_returns_not_implemented() {
        let dispatcher = Dispatcher::new(make_state());
        let req = make_request("session.list", json!(1));
        let outcome = dispatcher.dispatch(req).await.expect("should return outcome");
        match outcome {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, NOT_IMPLEMENTED),
            RpcOutcome::Ok(_) => panic!("expected Err outcome for session.list"),
        }
    }

    #[tokio::test]
    async fn dispatch_sidebar_method_returns_not_implemented() {
        let dispatcher = Dispatcher::new(make_state());
        let req = make_request("sidebar.set_status", json!(1));
        let outcome = dispatcher.dispatch(req).await.expect("should return outcome");
        match outcome {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, NOT_IMPLEMENTED),
            RpcOutcome::Ok(_) => panic!("expected Err outcome for sidebar.set_status"),
        }
    }
}
