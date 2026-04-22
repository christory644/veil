//! Workspace method handlers for the JSON-RPC socket API.
//!
//! Each handler receives `Arc<Mutex<AppState>>`, JSON params, and the request
//! ID. It locks state, calls the appropriate `AppState` method, and returns an
//! `RpcOutcome`.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use veil_core::state::{AppState, StateError};
use veil_core::workspace::WorkspaceId;

use crate::rpc::{ErrorResponse, RpcOutcome};

// ── Helper ─────────────────────────────────────────────────────────────────────

/// Map a `StateError` to an `RpcOutcome::Err`.
fn map_state_error(id: serde_json::Value, ws_id: u64, err: StateError) -> RpcOutcome {
    match err {
        StateError::WorkspaceNotFound(_) => {
            RpcOutcome::Err(ErrorResponse::workspace_not_found(id, ws_id))
        }
        other => RpcOutcome::Err(ErrorResponse::internal_error(id, other.to_string())),
    }
}

// ── Handler: workspace.create ─────────────────────────────────────────────────

/// `workspace.create`
///
/// Params: `{ "name": string, "working_directory": string }`
/// Result: `{ "id": u64, "name": string, "working_directory": string }`
pub(crate) async fn create(
    state: &Arc<Mutex<AppState>>,
    params: serde_json::Value,
    id: serde_json::Value,
) -> RpcOutcome {
    let Some(name) = params.get("name").and_then(serde_json::Value::as_str) else {
        return RpcOutcome::Err(ErrorResponse::invalid_params(id, "missing field: name"));
    };
    let Some(wd) = params.get("working_directory").and_then(serde_json::Value::as_str) else {
        return RpcOutcome::Err(ErrorResponse::invalid_params(
            id,
            "missing field: working_directory",
        ));
    };
    let (name, wd) = (name.to_string(), wd.to_string());

    let mut guard = state.lock().await;
    let ws_id = guard.create_workspace(name.clone(), PathBuf::from(&wd));

    RpcOutcome::Ok(serde_json::json!({
        "id": ws_id.as_u64(),
        "name": name,
        "working_directory": wd,
    }))
}

// ── Handler: workspace.list ───────────────────────────────────────────────────

/// `workspace.list`
///
/// Params: `{}` (ignored)
/// Result: `[{ "id": u64, "name": string, "working_directory": string,
///              "active": bool, "branch": string|null }]`
pub(crate) async fn list(state: &Arc<Mutex<AppState>>, _id: serde_json::Value) -> RpcOutcome {
    let guard = state.lock().await;
    let active_id = guard.active_workspace_id;

    let entries: Vec<serde_json::Value> = guard
        .workspaces
        .iter()
        .map(|ws| {
            serde_json::json!({
                "id": ws.id.as_u64(),
                "name": ws.name,
                "working_directory": ws.working_directory.to_string_lossy(),
                "active": active_id == Some(ws.id),
                "branch": ws.branch,
            })
        })
        .collect();

    RpcOutcome::Ok(serde_json::Value::Array(entries))
}

// ── Handler: workspace.select ─────────────────────────────────────────────────

/// `workspace.select`
///
/// Params: `{ "id": u64 }`
/// Result: `{ "id": u64 }`
pub(crate) async fn select(
    state: &Arc<Mutex<AppState>>,
    params: serde_json::Value,
    id: serde_json::Value,
) -> RpcOutcome {
    let Some(ws_id_raw) = params.get("id").and_then(serde_json::Value::as_u64) else {
        return RpcOutcome::Err(ErrorResponse::invalid_params(id, "missing field: id"));
    };

    let mut guard = state.lock().await;
    match guard.set_active_workspace(WorkspaceId::new(ws_id_raw)) {
        Ok(()) => RpcOutcome::Ok(serde_json::json!({ "id": ws_id_raw })),
        Err(err) => map_state_error(id, ws_id_raw, err),
    }
}

// ── Handler: workspace.close ──────────────────────────────────────────────────

/// `workspace.close`
///
/// Params: `{ "id": u64 }`
/// Result: `{ "id": u64 }`
pub(crate) async fn close(
    state: &Arc<Mutex<AppState>>,
    params: serde_json::Value,
    id: serde_json::Value,
) -> RpcOutcome {
    let Some(ws_id_raw) = params.get("id").and_then(serde_json::Value::as_u64) else {
        return RpcOutcome::Err(ErrorResponse::invalid_params(id, "missing field: id"));
    };

    let mut guard = state.lock().await;
    match guard.close_workspace(WorkspaceId::new(ws_id_raw)) {
        Ok(_) => RpcOutcome::Ok(serde_json::json!({ "id": ws_id_raw })),
        Err(err) => map_state_error(id, ws_id_raw, err),
    }
}

// ── Handler: workspace.rename ─────────────────────────────────────────────────

/// `workspace.rename`
///
/// Params: `{ "id": u64, "name": string }`
/// Result: `{ "id": u64, "name": string }`
pub(crate) async fn rename(
    state: &Arc<Mutex<AppState>>,
    params: serde_json::Value,
    id: serde_json::Value,
) -> RpcOutcome {
    let Some(ws_id_raw) = params.get("id").and_then(serde_json::Value::as_u64) else {
        return RpcOutcome::Err(ErrorResponse::invalid_params(id, "missing field: id"));
    };
    let Some(new_name) = params.get("name").and_then(serde_json::Value::as_str) else {
        return RpcOutcome::Err(ErrorResponse::invalid_params(id, "missing field: name"));
    };
    let new_name = new_name.to_string();

    let mut guard = state.lock().await;
    match guard.rename_workspace(WorkspaceId::new(ws_id_raw), new_name.clone()) {
        Ok(()) => RpcOutcome::Ok(serde_json::json!({ "id": ws_id_raw, "name": new_name })),
        Err(err) => map_state_error(id, ws_id_raw, err),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::{INVALID_PARAMS, WORKSPACE_NOT_FOUND};
    use serde_json::json;

    fn make_state() -> Arc<Mutex<AppState>> {
        Arc::new(Mutex::new(AppState::new()))
    }

    async fn create_ws(state: &Arc<Mutex<AppState>>, name: &str, wd: &str) -> serde_json::Value {
        let params = json!({"name": name, "working_directory": wd});
        match create(state, params, json!(1)).await {
            RpcOutcome::Ok(v) => v,
            RpcOutcome::Err(e) => panic!("create failed: {:?}", e.error),
        }
    }

    // ── workspace.create ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_returns_workspace_id_and_name() {
        let state = make_state();
        let result =
            create(&state, json!({"name": "myws", "working_directory": "/tmp"}), json!(1)).await;
        match result {
            RpcOutcome::Ok(v) => {
                assert!(v.get("id").is_some(), "result should contain id field");
                assert_eq!(v["name"], "myws");
            }
            RpcOutcome::Err(e) => panic!("expected Ok, got error: {:?}", e.error),
        }
    }

    #[tokio::test]
    async fn create_missing_name_returns_invalid_params() {
        let state = make_state();
        let result = create(&state, json!({"working_directory": "/tmp"}), json!(1)).await;
        match result {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, INVALID_PARAMS),
            RpcOutcome::Ok(_) => panic!("expected invalid params error"),
        }
    }

    #[tokio::test]
    async fn create_missing_working_directory_returns_invalid_params() {
        let state = make_state();
        let result = create(&state, json!({"name": "myws"}), json!(1)).await;
        match result {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, INVALID_PARAMS),
            RpcOutcome::Ok(_) => panic!("expected invalid params error"),
        }
    }

    #[tokio::test]
    async fn create_adds_workspace_to_state() {
        let state = make_state();
        create(&state, json!({"name": "myws", "working_directory": "/tmp"}), json!(1)).await;
        let guard = state.lock().await;
        assert_eq!(guard.workspaces.len(), 1);
        assert_eq!(guard.workspaces[0].name, "myws");
    }

    #[tokio::test]
    async fn create_returns_active_true_for_first_workspace() {
        let state = make_state();
        create(&state, json!({"name": "first", "working_directory": "/tmp"}), json!(1)).await;
        // The list should show the first workspace as active.
        let list_result = match list(&state, json!(2)).await {
            RpcOutcome::Ok(v) => v,
            RpcOutcome::Err(e) => panic!("list failed: {:?}", e.error),
        };
        let workspaces = list_result.as_array().expect("should be array");
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0]["active"], json!(true));
    }

    // ── workspace.list ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_empty_state_returns_empty_array() {
        let state = make_state();
        let result = list(&state, json!(1)).await;
        match result {
            RpcOutcome::Ok(v) => {
                let arr = v.as_array().expect("result should be array");
                assert!(arr.is_empty(), "expected empty array");
            }
            RpcOutcome::Err(e) => panic!("expected Ok, got error: {:?}", e.error),
        }
    }

    #[tokio::test]
    async fn list_returns_all_workspaces() {
        let state = make_state();
        create_ws(&state, "ws1", "/tmp/a").await;
        create_ws(&state, "ws2", "/tmp/b").await;
        let result = list(&state, json!(1)).await;
        match result {
            RpcOutcome::Ok(v) => {
                let arr = v.as_array().expect("result should be array");
                assert_eq!(arr.len(), 2);
            }
            RpcOutcome::Err(e) => panic!("expected Ok, got error: {:?}", e.error),
        }
    }

    #[tokio::test]
    async fn list_marks_active_workspace() {
        let state = make_state();
        // Create two workspaces; first is active by default.
        let first = create_ws(&state, "ws1", "/tmp/a").await;
        create_ws(&state, "ws2", "/tmp/b").await;

        let result = match list(&state, json!(1)).await {
            RpcOutcome::Ok(v) => v,
            RpcOutcome::Err(e) => panic!("list failed: {:?}", e.error),
        };
        let arr = result.as_array().expect("array");
        let first_id = first["id"].as_u64().expect("id");
        for entry in arr {
            let entry_id = entry["id"].as_u64().expect("entry id");
            if entry_id == first_id {
                assert_eq!(entry["active"], json!(true), "first workspace should be active");
            } else {
                assert_eq!(entry["active"], json!(false), "other workspace should not be active");
            }
        }
    }

    #[tokio::test]
    async fn list_includes_branch_if_set() {
        let state = make_state();
        create_ws(&state, "branchws", "/tmp").await;

        // Manually set branch on the workspace.
        {
            let mut guard = state.lock().await;
            guard.workspaces[0].branch = Some("main".to_string());
        }

        let result = match list(&state, json!(1)).await {
            RpcOutcome::Ok(v) => v,
            RpcOutcome::Err(e) => panic!("list failed: {:?}", e.error),
        };
        let arr = result.as_array().expect("array");
        assert_eq!(arr[0]["branch"], json!("main"));
    }

    #[tokio::test]
    async fn list_branch_null_when_unset() {
        let state = make_state();
        create_ws(&state, "nobranchws", "/tmp").await;
        let result = match list(&state, json!(1)).await {
            RpcOutcome::Ok(v) => v,
            RpcOutcome::Err(e) => panic!("list failed: {:?}", e.error),
        };
        let arr = result.as_array().expect("array");
        assert_eq!(arr[0]["branch"], json!(null));
    }

    // ── workspace.select ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn select_valid_id_returns_ok() {
        let state = make_state();
        create_ws(&state, "ws2", "/tmp/b").await;
        let ws_id = {
            let guard = state.lock().await;
            guard.workspaces[0].id.as_u64()
        };

        let result = select(&state, json!({"id": ws_id}), json!(1)).await;
        assert!(matches!(result, RpcOutcome::Ok(_)));
    }

    #[tokio::test]
    async fn select_updates_active_workspace() {
        let state = make_state();
        create_ws(&state, "ws1", "/tmp/a").await;
        create_ws(&state, "ws2", "/tmp/b").await;

        let second_id = {
            let guard = state.lock().await;
            guard.workspaces[1].id.as_u64()
        };

        select(&state, json!({"id": second_id}), json!(1)).await;

        let guard = state.lock().await;
        let active = guard.active_workspace_id.expect("should have active");
        assert_eq!(active.as_u64(), second_id);
    }

    #[tokio::test]
    async fn select_nonexistent_returns_workspace_not_found() {
        let state = make_state();
        let result = select(&state, json!({"id": 99999_u64}), json!(1)).await;
        match result {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, WORKSPACE_NOT_FOUND),
            RpcOutcome::Ok(_) => panic!("expected workspace not found error"),
        }
    }

    #[tokio::test]
    async fn select_missing_id_param_returns_invalid_params() {
        let state = make_state();
        let result = select(&state, json!({}), json!(1)).await;
        match result {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, INVALID_PARAMS),
            RpcOutcome::Ok(_) => panic!("expected invalid params error"),
        }
    }

    // ── workspace.close ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn close_existing_workspace_returns_ok() {
        let state = make_state();
        create_ws(&state, "toclose", "/tmp").await;
        let ws_id = {
            let guard = state.lock().await;
            guard.workspaces[0].id.as_u64()
        };
        let result = close(&state, json!({"id": ws_id}), json!(1)).await;
        assert!(matches!(result, RpcOutcome::Ok(_)));
    }

    #[tokio::test]
    async fn close_removes_workspace_from_state() {
        let state = make_state();
        create_ws(&state, "toclose", "/tmp").await;
        let ws_id = {
            let guard = state.lock().await;
            guard.workspaces[0].id.as_u64()
        };
        close(&state, json!({"id": ws_id}), json!(1)).await;
        let guard = state.lock().await;
        assert!(guard.workspaces.is_empty(), "workspace should be removed after close");
    }

    #[tokio::test]
    async fn close_nonexistent_returns_workspace_not_found() {
        let state = make_state();
        let result = close(&state, json!({"id": 99999_u64}), json!(1)).await;
        match result {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, WORKSPACE_NOT_FOUND),
            RpcOutcome::Ok(_) => panic!("expected workspace not found error"),
        }
    }

    #[tokio::test]
    async fn close_missing_id_param_returns_invalid_params() {
        let state = make_state();
        let result = close(&state, json!({}), json!(1)).await;
        match result {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, INVALID_PARAMS),
            RpcOutcome::Ok(_) => panic!("expected invalid params error"),
        }
    }

    // ── workspace.rename ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn rename_valid_returns_new_name() {
        let state = make_state();
        create_ws(&state, "original", "/tmp").await;
        let ws_id = {
            let guard = state.lock().await;
            guard.workspaces[0].id.as_u64()
        };
        let result = rename(&state, json!({"id": ws_id, "name": "renamed"}), json!(1)).await;
        match result {
            RpcOutcome::Ok(v) => {
                assert_eq!(v["name"], "renamed");
                assert_eq!(v["id"], json!(ws_id));
            }
            RpcOutcome::Err(e) => panic!("expected Ok, got error: {:?}", e.error),
        }
    }

    #[tokio::test]
    async fn rename_updates_state() {
        let state = make_state();
        create_ws(&state, "original", "/tmp").await;
        let ws_id = {
            let guard = state.lock().await;
            guard.workspaces[0].id.as_u64()
        };
        rename(&state, json!({"id": ws_id, "name": "updated"}), json!(1)).await;
        let guard = state.lock().await;
        assert_eq!(guard.workspaces[0].name, "updated");
    }

    #[tokio::test]
    async fn rename_nonexistent_returns_workspace_not_found() {
        let state = make_state();
        let result = rename(&state, json!({"id": 99999_u64, "name": "anything"}), json!(1)).await;
        match result {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, WORKSPACE_NOT_FOUND),
            RpcOutcome::Ok(_) => panic!("expected workspace not found error"),
        }
    }

    #[tokio::test]
    async fn rename_missing_id_returns_invalid_params() {
        let state = make_state();
        let result = rename(&state, json!({"name": "newname"}), json!(1)).await;
        match result {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, INVALID_PARAMS),
            RpcOutcome::Ok(_) => panic!("expected invalid params error"),
        }
    }

    #[tokio::test]
    async fn rename_missing_name_returns_invalid_params() {
        let state = make_state();
        create_ws(&state, "ws", "/tmp").await;
        let ws_id = {
            let guard = state.lock().await;
            guard.workspaces[0].id.as_u64()
        };
        let result = rename(&state, json!({"id": ws_id}), json!(1)).await;
        match result {
            RpcOutcome::Err(e) => assert_eq!(e.error.code, INVALID_PARAMS),
            RpcOutcome::Ok(_) => panic!("expected invalid params error"),
        }
    }
}
