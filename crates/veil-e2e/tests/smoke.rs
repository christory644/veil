use veil_e2e::{JsonRpcClient, VeilTestInstance};

/// Smoke test demonstrating the E2E test pattern.
///
/// This test is `#[ignore]` because the socket API (VEI-20) is not yet
/// implemented. Once VEI-20 lands, remove the `#[ignore]` attribute and
/// this test will verify the full lifecycle:
///
/// 1. Start a `VeilTestInstance`
/// 2. Connect a `JsonRpcClient`
/// 3. Call `workspace.list`, assert empty
/// 4. Call `workspace.create`, assert success
/// 5. Call `workspace.list`, assert 1 workspace
/// 6. Stop the instance
#[test]
#[ignore = "requires VEI-20 (socket API) to be implemented"]
fn smoke_test_placeholder() {
    // Step 1: Start a test instance
    let mut instance = VeilTestInstance::start().expect("should start test instance");

    // Step 2: Connect a JSON-RPC client
    let client = JsonRpcClient::connect(instance.socket_path()).expect("should connect to socket");

    // Step 3: List workspaces — should be empty initially
    let result = client
        .call("workspace.list", serde_json::json!({}))
        .expect("workspace.list should succeed");
    let workspaces = result.as_array().expect("should be an array");
    assert!(workspaces.is_empty(), "should start with no workspaces");

    // Step 4: Create a workspace
    let result = client
        .call(
            "workspace.create",
            serde_json::json!({"name": "test-ws", "working_directory": "/tmp"}),
        )
        .expect("workspace.create should succeed");
    assert!(result.get("id").is_some(), "should return workspace id");

    // Step 5: List workspaces again — should have 1
    let result = client
        .call("workspace.list", serde_json::json!({}))
        .expect("workspace.list should succeed");
    let workspaces = result.as_array().expect("should be an array");
    assert_eq!(workspaces.len(), 1, "should have 1 workspace after create");

    // Step 6: Stop the instance
    instance.stop().expect("should stop cleanly");
}
