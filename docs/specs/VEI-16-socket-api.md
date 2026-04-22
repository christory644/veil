# VEI-16: Socket API — JSON-RPC 2.0 Server

## Context

Veil's socket API is the programmatic control plane. It lets AI agents, shell scripts, and the E2E test harness drive the application without a GUI. A running Veil instance exposes a Unix domain socket (macOS/Linux) or named pipe (Windows) at a well-known path. Clients send newline-delimited JSON-RPC 2.0 requests and receive JSON-RPC 2.0 responses.

The socket API also doubles as the E2E test harness: `veil-e2e` already has stub implementations of `VeilTestInstance` and `JsonRpcClient` that reference VEI-20 (now renumbered VEI-16). Those stubs need to be replaced with real implementations once this task is complete.

### What this task covers

VEI-16 builds the complete foundational socket API inside the `veil-socket` crate:

1. **Transport layer** — cross-platform socket listener (Unix domain socket / named pipe abstraction)
2. **JSON-RPC 2.0 protocol layer** — request/response types, framing (newline-delimited), error codes
3. **Method dispatcher** — routes parsed requests to method handlers, returns typed responses
4. **Workspace method handlers** — full implementations of all five workspace methods operating on a shared `Arc<Mutex<AppState>>`
5. **Stub dispatchers for other method groups** — surface, notification, sidebar, session methods return `-32601 Method not found` or a structured "not yet implemented" error so the dispatch table is complete
6. **Socket server actor** — `tokio` task that binds the socket, accepts connections, spawns per-connection tasks, handles graceful shutdown
7. **Integration tests** — launch the server in-process with a temporary socket path, connect a real client, exercise the workspace methods

### What is explicitly out of scope

- **VEI-51**: Wiring the socket server into the winit event loop / real `AppState` — the server in this task runs with its own `Arc<Mutex<AppState>>` and is testable in isolation.
- **VEI-49**: Full surface method implementations (`surface.split`, `surface.focus`, `surface.list`, `surface.send_text`).
- **VEI-50**: Full notification/sidebar method implementations.
- **VEI-52**: Session method implementations (`session.list`, `session.search`, `session.preview`).
- Windows named pipe transport — the platform abstraction is designed for it but only the Unix socket path is implemented now.

### Architecture summary

```
Client (shell / agent / test)
    │ newline-delimited JSON
    ▼
SocketServer (tokio task)
    │ accepts connections
    ▼
ConnectionHandler (one task per client)
    │ reads lines, parses JSON-RPC requests
    ▼
Dispatcher::dispatch(method, params, state)
    │ routes to handler fn
    ▼
WorkspaceHandlers / StubHandlers
    │ mutates / reads Arc<Mutex<AppState>>
    ▼
JSON-RPC response serialized and written back
```

### Design decisions

- **`Arc<Mutex<AppState>>`** for state sharing: the socket server holds a reference to shared state. In the standalone server (this task) the state is owned by the test or caller. In VEI-51 it will be the same `AppState` as the UI thread. The `Mutex` is `tokio::sync::Mutex` so lock acquisition doesn't block the executor.
- **Newline-delimited JSON framing**: requests are one JSON object per line, terminated by `\n`. This matches the system design doc and is the simplest framing that works reliably with `BufReader::lines()`. No length prefix needed.
- **No authentication**: socket file permissions are the security boundary. The socket is created `0600`. Per the system design, no auth token is needed.
- **`thiserror` for errors**: all error types use `thiserror` per project conventions.
- **`tokio` throughout**: the server, connection handlers, and tests all use the tokio async runtime. The crate depends on `tokio = { workspace = true, features = ["full"] }`.
- **Workspace methods operate on real `AppState`**: the five workspace handlers call the existing `AppState` methods (`create_workspace`, `close_workspace`, `set_active_workspace`, `rename_workspace`, workspace lookups). No business logic is duplicated.
- **`serde_json::Value` for params and results**: method handlers receive `serde_json::Value` params and return `serde_json::Value` results. This avoids a combinatorial explosion of param/result types while remaining correct.

## Implementation Units

### Unit 1: JSON-RPC 2.0 types (`veil-socket/src/rpc.rs`)

Define the wire-format types for JSON-RPC 2.0 requests and responses. These types are the boundary between the network and the application.

**File:** `crates/veil-socket/src/rpc.rs`

**Types:**

```rust
/// A JSON-RPC 2.0 request.
///
/// The `id` field is `Option<serde_json::Value>` because JSON-RPC allows
/// string, number, or null IDs. Notifications have no `id`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub id: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 response (success).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub result: serde_json::Value,
    pub id: serde_json::Value,
}

/// A JSON-RPC 2.0 error response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorResponse {
    pub jsonrpc: String,
    pub error: RpcError,
    pub id: serde_json::Value,
}

/// The `error` object inside an error response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
```

**Standard error codes (as constants):**

```rust
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
/// Application-defined: workspace not found.
pub const WORKSPACE_NOT_FOUND: i64 = -32000;
/// Application-defined: method not yet implemented.
pub const NOT_IMPLEMENTED: i64 = -32001;
```

**Helper constructors:**

```rust
impl Response {
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self;
}

impl ErrorResponse {
    pub fn new(id: serde_json::Value, code: i64, message: impl Into<String>) -> Self;
    pub fn parse_error() -> Self;
    pub fn method_not_found(id: serde_json::Value, method: &str) -> Self;
    pub fn invalid_params(id: serde_json::Value, detail: impl Into<String>) -> Self;
    pub fn internal_error(id: serde_json::Value, detail: impl Into<String>) -> Self;
    pub fn workspace_not_found(id: serde_json::Value, ws_id: u64) -> Self;
}
```

**`RpcOutcome` enum** (returned by method handlers; converted to wire bytes by the connection handler):

```rust
/// What a method handler can return.
pub enum RpcOutcome {
    Ok(serde_json::Value),
    Err(ErrorResponse),
}
```

**Tests for Unit 1:**

- `request_round_trip`: serialize a `Request` to JSON, deserialize back — all fields preserved.
- `request_missing_id_deserializes_as_none`: a request with no `id` field deserializes with `id: None`.
- `request_params_defaults_to_null`: a request with no `params` field deserializes with `params: Value::Null`.
- `response_serializes_jsonrpc_field`: `Response::ok(...)` serialized JSON always contains `"jsonrpc":"2.0"`.
- `error_response_serializes_code_and_message`: `ErrorResponse::new(...)` round-trips `code` and `message`.
- `error_data_omitted_when_none`: `RpcError` with `data: None` serializes without `"data"` key.
- `parse_error_has_correct_code`: `ErrorResponse::parse_error()` has code `-32700`.
- `method_not_found_embeds_method_name`: `ErrorResponse::method_not_found(...)` message contains the method string.
- `workspace_not_found_has_application_code`: `ErrorResponse::workspace_not_found(...)` has code `-32000`.

---

### Unit 2: Transport abstraction (`veil-socket/src/transport.rs`)

Cross-platform socket listener that can bind and accept connections. On macOS/Linux this is a Unix domain socket. The abstraction is designed so Windows named pipe support can be added later without changing callers.

**File:** `crates/veil-socket/src/transport.rs`

**Types:**

```rust
/// How to locate the socket on the current platform.
#[derive(Debug, Clone)]
pub enum SocketPath {
    /// Unix domain socket at the given filesystem path.
    Unix(std::path::PathBuf),
    // Windows(String) — named pipe name, deferred.
}

impl SocketPath {
    /// Resolve the default socket path for the current platform.
    ///
    /// macOS/Linux: `$XDG_RUNTIME_DIR/veil.sock` if the env var is set,
    /// otherwise `/tmp/veil.sock`.
    pub fn default_for_platform() -> Self;

    /// Return the filesystem path for Unix sockets (for use in VEIL_SOCKET env var).
    pub fn as_path(&self) -> Option<&std::path::Path>;
}

/// A bound socket listener.
pub struct SocketListener {
    inner: tokio::net::UnixListener,
    path: SocketPath,
}

impl SocketListener {
    /// Bind a new listener at the given path.
    /// Removes any pre-existing socket file before binding (stale socket cleanup).
    pub async fn bind(path: SocketPath) -> Result<Self, SocketError>;

    /// Accept the next incoming connection.
    /// Returns a (reader, writer) pair for the connection.
    pub async fn accept(
        &self,
    ) -> Result<(tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>,
                 tokio::io::BufWriter<tokio::net::unix::OwnedWriteHalf>), SocketError>;

    /// The path this listener is bound to.
    pub fn path(&self) -> &SocketPath;
}

impl Drop for SocketListener {
    /// Remove the socket file on drop.
    fn drop(&mut self);
}

/// Errors from socket transport operations.
#[derive(Debug, thiserror::Error)]
pub enum SocketError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported platform")]
    UnsupportedPlatform,
}
```

**Tests for Unit 2:**

- `bind_creates_socket_file`: `SocketListener::bind(path)` creates a file at the given path.
- `bind_removes_stale_socket`: if a socket file already exists at the path, `bind` removes it and creates a fresh one.
- `drop_removes_socket_file`: dropping the `SocketListener` removes the socket file.
- `accept_receives_connection`: a client `UnixStream::connect` to the bound path succeeds and `accept` returns a connection.
- `default_for_platform_unix_fallback`: when `XDG_RUNTIME_DIR` is not set, `default_for_platform()` returns a path under `/tmp`.
- `default_for_platform_uses_xdg_runtime_dir`: when `XDG_RUNTIME_DIR` is set to a temp dir, `default_for_platform()` uses it.

All transport tests use `tempfile::TempDir` to create an isolated socket path. Tests are `#[tokio::test]`.

---

### Unit 3: Method dispatcher (`veil-socket/src/dispatcher.rs`)

Routes a parsed `Request` to the appropriate handler function. Owns the `Arc<Mutex<AppState>>` reference and passes it to handlers.

**File:** `crates/veil-socket/src/dispatcher.rs`

**Types:**

```rust
/// The central request dispatcher.
///
/// Holds shared state and routes requests to handlers.
pub struct Dispatcher {
    state: std::sync::Arc<tokio::sync::Mutex<veil_core::state::AppState>>,
}

impl Dispatcher {
    /// Create a new dispatcher over the given shared state.
    pub fn new(state: std::sync::Arc<tokio::sync::Mutex<veil_core::state::AppState>>) -> Self;

    /// Dispatch a parsed request and return the outcome.
    ///
    /// Returns `None` for notifications (requests with no `id`).
    pub async fn dispatch(&self, request: Request) -> Option<RpcOutcome>;
}
```

**Routing table** (inside `dispatch`):

```
"workspace.create"  → workspace::create(state, params, id)
"workspace.list"    → workspace::list(state, id)
"workspace.select"  → workspace::select(state, params, id)
"workspace.close"   → workspace::close(state, params, id)
"workspace.rename"  → workspace::rename(state, params, id)
"surface.*"         → stub::not_implemented(id, method)
"notification.*"    → stub::not_implemented(id, method)
"sidebar.*"         → stub::not_implemented(id, method)
"session.*"         → stub::not_implemented(id, method)
(unknown)           → ErrorResponse::method_not_found(id, method)
```

Notifications (requests with `id: None`) are dispatched but return `None` rather than a response.

**Tests for Unit 3:**

- `dispatch_unknown_method_returns_method_not_found`: a request for `"foo.bar"` returns `RpcOutcome::Err` with code `-32601`.
- `dispatch_notification_returns_none`: a request with `id: null` (JSON-RPC notification) returns `None`.
- `dispatch_workspace_list_routes_to_handler`: a `workspace.list` request returns `RpcOutcome::Ok`.
- `dispatch_surface_method_returns_not_implemented`: a `surface.split` request returns `RpcOutcome::Err` with code `-32001`.
- `dispatch_notification_method_returns_not_implemented`: a `notification.create` request returns `RpcOutcome::Err` with code `-32001`.
- `dispatch_session_method_returns_not_implemented`: a `session.list` request returns `RpcOutcome::Err` with code `-32001`.
- `dispatch_sidebar_method_returns_not_implemented`: a `sidebar.set_status` request returns `RpcOutcome::Err` with code `-32001`.

---

### Unit 4: Workspace method handlers (`veil-socket/src/handlers/workspace.rs`)

Full implementations of the five workspace methods. Each operates on a `tokio::sync::MutexGuard<AppState>` and returns `RpcOutcome`.

**File:** `crates/veil-socket/src/handlers/workspace.rs`

**Method signatures (internal, not public API):**

```rust
/// workspace.create
///
/// Params: { "name": string, "working_directory": string }
/// Result: { "id": u64, "name": string, "working_directory": string }
pub(crate) async fn create(
    state: &std::sync::Arc<tokio::sync::Mutex<AppState>>,
    params: serde_json::Value,
    id: serde_json::Value,
) -> RpcOutcome;

/// workspace.list
///
/// Params: {} (ignored)
/// Result: [{ "id": u64, "name": string, "working_directory": string,
///             "active": bool, "branch": string|null }]
pub(crate) async fn list(
    state: &std::sync::Arc<tokio::sync::Mutex<AppState>>,
    id: serde_json::Value,
) -> RpcOutcome;

/// workspace.select
///
/// Params: { "id": u64 }
/// Result: { "id": u64 }
pub(crate) async fn select(
    state: &std::sync::Arc<tokio::sync::Mutex<AppState>>,
    params: serde_json::Value,
    id: serde_json::Value,
) -> RpcOutcome;

/// workspace.close
///
/// Params: { "id": u64 }
/// Result: { "id": u64 }
pub(crate) async fn close(
    state: &std::sync::Arc<tokio::sync::Mutex<AppState>>,
    params: serde_json::Value,
    id: serde_json::Value,
) -> RpcOutcome;

/// workspace.rename
///
/// Params: { "id": u64, "name": string }
/// Result: { "id": u64, "name": string }
pub(crate) async fn rename(
    state: &std::sync::Arc<tokio::sync::Mutex<AppState>>,
    params: serde_json::Value,
    id: serde_json::Value,
) -> RpcOutcome;
```

**Param validation**: each handler extracts required fields from `params` using `serde_json` accessors. If a required field is missing or has the wrong type, it returns `ErrorResponse::invalid_params(...)` without touching state.

**Error mapping**: `StateError::WorkspaceNotFound` maps to `ErrorResponse::workspace_not_found(...)`. Other `StateError` variants map to `ErrorResponse::internal_error(...)`.

**Tests for Unit 4:**

Each test constructs an `Arc<Mutex<AppState>>` directly — no network, no server. Pure unit tests.

`workspace.create`:
- `create_returns_workspace_id_and_name`: create with valid params returns `RpcOutcome::Ok` containing an `"id"` field.
- `create_missing_name_returns_invalid_params`: omitting `"name"` returns code `-32602`.
- `create_missing_working_directory_returns_invalid_params`: omitting `"working_directory"` returns code `-32602`.
- `create_adds_workspace_to_state`: after `create`, the state contains the new workspace.
- `create_returns_active_true_for_first_workspace`: the first workspace created is active.

`workspace.list`:
- `list_empty_state_returns_empty_array`: no workspaces → `result` is `[]`.
- `list_returns_all_workspaces`: after creating two workspaces, both appear in the result.
- `list_marks_active_workspace`: the active workspace has `"active": true`, others have `"active": false`.
- `list_includes_branch_if_set`: a workspace with `branch: Some("main")` serializes `"branch": "main"`.
- `list_branch_null_when_unset`: a workspace with `branch: None` serializes `"branch": null`.

`workspace.select`:
- `select_valid_id_returns_ok`: select an existing workspace returns `RpcOutcome::Ok`.
- `select_updates_active_workspace`: after select, `state.active_workspace_id` is the selected one.
- `select_nonexistent_returns_workspace_not_found`: code `-32000`.
- `select_missing_id_param_returns_invalid_params`: code `-32602`.

`workspace.close`:
- `close_existing_workspace_returns_ok`: close an existing workspace returns `RpcOutcome::Ok`.
- `close_removes_workspace_from_state`: after close, workspace is no longer in state.
- `close_nonexistent_returns_workspace_not_found`: code `-32000`.
- `close_missing_id_param_returns_invalid_params`: code `-32602`.

`workspace.rename`:
- `rename_valid_returns_new_name`: result contains the updated name.
- `rename_updates_state`: `state.workspace(id).name` matches the new name.
- `rename_nonexistent_returns_workspace_not_found`: code `-32000`.
- `rename_missing_id_returns_invalid_params`: code `-32602`.
- `rename_missing_name_returns_invalid_params`: code `-32602`.

---

### Unit 5: Stub handlers (`veil-socket/src/handlers/stub.rs`)

Returns a structured "not yet implemented" error for all unimplemented method groups.

**File:** `crates/veil-socket/src/handlers/stub.rs`

```rust
/// Return a NOT_IMPLEMENTED error for methods not yet implemented.
pub(crate) fn not_implemented(id: serde_json::Value, method: &str) -> RpcOutcome;
```

The returned `ErrorResponse` has:
- `code`: `-32001` (`NOT_IMPLEMENTED`)
- `message`: `"Method not yet implemented: <method>"`
- `data`: `None`

**Tests for Unit 5:**

- `not_implemented_has_correct_code`: returned outcome has code `-32001`.
- `not_implemented_embeds_method_name`: the message contains the method name.
- `not_implemented_is_err_outcome`: the outcome variant is `RpcOutcome::Err`.

---

### Unit 6: Connection handler (`veil-socket/src/connection.rs`)

Manages the I/O loop for a single client connection. Reads newline-delimited JSON from the client, dispatches each line to the `Dispatcher`, and writes the response back.

**File:** `crates/veil-socket/src/connection.rs`

**Types:**

```rust
/// Handle a single client connection to completion.
///
/// Reads newline-delimited JSON from `reader`, dispatches each request
/// through `dispatcher`, and writes responses back to `writer`.
/// Returns when the client disconnects or `shutdown` is triggered.
pub async fn handle_connection(
    reader: tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::io::BufWriter<tokio::net::unix::OwnedWriteHalf>,
    dispatcher: std::sync::Arc<Dispatcher>,
    mut shutdown: veil_core::lifecycle::ShutdownHandle,
);
```

**Behavior:**

- Each line from the reader is parsed as JSON. Parse failure → write a `parse_error` response and continue (do not close the connection).
- A valid JSON object that is not a valid `Request` → write an `invalid_request` response and continue.
- A valid request → dispatch, get optional `RpcOutcome`, write response (or nothing for notifications).
- Client EOF → return cleanly.
- Shutdown triggered → return cleanly (mid-flight requests complete).
- All responses are serialized as compact JSON followed by `\n`.

**Tests for Unit 6:**

Tests use `tokio::io::duplex` to create in-memory byte pipes — no real sockets needed.

- `invalid_json_returns_parse_error`: send `"not json\n"` → receive a response with code `-32700`.
- `valid_json_non_request_returns_invalid_request`: send `"42\n"` → receive a response with code `-32600`.
- `workspace_list_request_returns_result`: send a well-formed `workspace.list` request → receive a response with `"result"` key.
- `notification_request_produces_no_response`: send a request with no `id` → no response is written.
- `multiple_requests_all_handled`: send three requests back-to-back, all three receive responses in order.
- `client_disconnect_returns_cleanly`: close the write end of the duplex pipe → `handle_connection` returns without panic.
- `response_is_newline_terminated`: every response ends with `\n`.
- `response_preserves_request_id`: the response `id` matches the request `id`.

---

### Unit 7: Socket server actor (`veil-socket/src/server.rs`)

The top-level async task that binds the socket, accepts connections in a loop, and spawns per-connection tasks.

**File:** `crates/veil-socket/src/server.rs`

**Types:**

```rust
/// Configuration for the socket server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Where to bind the socket.
    pub socket_path: SocketPath,
}

impl ServerConfig {
    /// Create a config using the platform default socket path.
    pub fn default_for_platform() -> Self;

    /// Create a config with a specific path (used in tests).
    pub fn with_path(path: impl Into<std::path::PathBuf>) -> Self;
}

/// The socket API server.
///
/// Binds the socket, accepts connections, spawns connection handlers,
/// and coordinates graceful shutdown.
pub struct SocketServer {
    config: ServerConfig,
    state: std::sync::Arc<tokio::sync::Mutex<veil_core::state::AppState>>,
}

impl SocketServer {
    /// Create a new server over the given shared state.
    pub fn new(
        config: ServerConfig,
        state: std::sync::Arc<tokio::sync::Mutex<veil_core::state::AppState>>,
    ) -> Self;

    /// Run the server until shutdown is signaled.
    ///
    /// Binds the socket, then loops accepting connections.
    /// Each connection is handed to `handle_connection` in a spawned task.
    /// Returns when `shutdown` is triggered and all connection tasks complete.
    pub async fn run(
        self,
        shutdown: veil_core::lifecycle::ShutdownHandle,
    ) -> Result<(), SocketError>;

    /// Returns the socket path this server is (or will be) bound to.
    pub fn socket_path(&self) -> &SocketPath;
}
```

**Tests for Unit 7:**

These are integration-style tests run within a single process. They are `#[tokio::test]` tests inside `server.rs` or a dedicated `tests/` file. They use `tempfile::TempDir` for isolated socket paths.

- `server_binds_and_accepts_connection`: start the server with a temp socket path, connect a `UnixStream`, verify the connection is accepted (server does not panic or return).
- `server_processes_workspace_list_request`: connect, send `workspace.list`, receive a valid JSON-RPC response with an empty array.
- `server_processes_workspace_create_then_list`: create a workspace, list it back — the list contains the created workspace.
- `server_handles_concurrent_clients`: spawn two clients simultaneously, each sends `workspace.list` — both receive valid responses.
- `server_shuts_down_cleanly`: trigger `ShutdownSignal`, verify `run` returns without hanging.
- `server_invalid_request_does_not_crash`: send malformed JSON to a live server — server continues accepting new connections.
- `server_sets_socket_file_permissions`: the socket file is only accessible by owner (`0o600`).

---

### Unit 8: Cargo.toml and crate wiring (`veil-socket/Cargo.toml`)

Update the `veil-socket` crate's `Cargo.toml` and `src/lib.rs` to pull in the new dependencies and expose the public surface.

**File changes:**

`crates/veil-socket/Cargo.toml`:

```toml
[package]
name = "veil-socket"
# ... workspace fields unchanged ...

[dependencies]
veil-core = { path = "../veil-core" }
tracing.workspace = true
thiserror.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio = { workspace = true, features = ["full"] }

[dev-dependencies]
mockall.workspace = true
tempfile.workspace = true
tokio = { workspace = true, features = ["full"] }
```

`crates/veil-socket/src/lib.rs` public surface:

```rust
pub mod rpc;
pub mod transport;
pub mod server;
mod connection;
mod dispatcher;
mod handlers;

pub use server::{ServerConfig, SocketServer};
pub use transport::{SocketError, SocketListener, SocketPath};
pub use rpc::{ErrorResponse, Request, Response, RpcError, RpcOutcome};
```

**Tests for Unit 8** — verify the crate compiles and the public API is accessible:
- `cargo build -p veil-socket` passes.
- `cargo test -p veil-socket` passes.

---

### Unit 9: Update `veil-e2e` client stubs to real implementations

Replace the `NotImplemented` stubs in `veil-e2e` with real implementations that use the socket API built in this task.

**File:** `crates/veil-e2e/src/lib.rs`

**Changes:**

`VeilTestInstance::start()`:
- Create a `tempfile::TempDir` for the socket.
- Construct an `Arc<Mutex<AppState>>` and a `SocketServer` with a temp path.
- Spawn the server on a `tokio::Runtime` via `tokio::spawn`.
- Wait for the socket file to appear (poll with a short timeout).
- Return `Self` with the socket path.

`VeilTestInstance::stop()`:
- Trigger `ShutdownSignal`, join the server task.
- Drop the `TempDir`.

`JsonRpcClient::connect(path)`:
- Connect a `tokio::net::UnixStream` to the path.
- Return `Self` with a `BufReader`/`BufWriter` over the stream.

`JsonRpcClient::call(method, params)`:
- Serialize a `Request` with a monotonically incrementing integer `id`.
- Write the request as newline-delimited JSON.
- Read the next line from the server.
- Deserialize as `Response` or `ErrorResponse`.
- Return `result` or an error.

**Tests for Unit 9** (in `crates/veil-e2e/tests/smoke.rs`, remove the `#[ignore]`):

- `smoke_test_workspace_lifecycle`: start instance → connect client → list (empty) → create → list (1 entry) → stop.
- `smoke_test_unimplemented_method_returns_error`: call `surface.split` → response contains an error with code `-32001`.
- `smoke_test_invalid_method_returns_method_not_found`: call `foo.bar` → response contains code `-32601`.

---

## Test Strategy Summary

| Unit | Test type | Key scenarios |
|------|-----------|---------------|
| 1 (RPC types) | `#[test]` | Serde round-trips, field presence, error codes |
| 2 (Transport) | `#[tokio::test]` | Bind, stale cleanup, drop, accept, XDG path |
| 3 (Dispatcher) | `#[tokio::test]` | Route table completeness, unknown method, notifications |
| 4 (Workspace handlers) | `#[tokio::test]` | Happy path + all error cases per method |
| 5 (Stub handlers) | `#[test]` | Error code, method name embedding |
| 6 (Connection) | `#[tokio::test]` | `tokio::io::duplex` — parse errors, multi-request, EOF |
| 7 (Server) | `#[tokio::test]` | Real socket in temp dir — bind, request, shutdown, concurrent |
| 8 (Cargo wiring) | build/test gate | Crate compiles and all tests pass |
| 9 (E2E) | `#[tokio::test]` | Full lifecycle, unimplemented methods, unknown method |

All tests in units 1–7 run without a winit event loop, a GPU, or a real PTY. They are pure library-level tests.

## Acceptance Criteria

1. `cargo test -p veil-socket` passes — all unit and integration tests green.
2. `cargo test -p veil-e2e` passes — the smoke tests in `tests/smoke.rs` pass with the `#[ignore]` removed.
3. `cargo clippy --all-targets --all-features -- -D warnings` passes with no warnings.
4. `cargo fmt --check` passes.
5. `workspace.list` called against a fresh `AppState` returns an empty array.
6. `workspace.create` followed by `workspace.list` returns an array with one entry containing correct `id`, `name`, `working_directory`, `active: true`, and `branch: null`.
7. `workspace.select` with a nonexistent ID returns an error response with code `-32000`.
8. `surface.split` returns an error response with code `-32001`.
9. `foo.bar` returns an error response with code `-32601`.
10. Invalid JSON sent to the server produces a `-32700` response without crashing the server.
11. Two concurrent clients can both send requests and receive correct independent responses.
12. The socket file is removed when the `SocketListener` is dropped.
13. The `VEIL_SOCKET` environment variable path is documented in `SocketPath::default_for_platform()`.

## Dependencies

### New crate dependencies (add to `veil-socket/Cargo.toml`)

| Dependency | Version | Purpose |
|-----------|---------|---------|
| `tokio` | `workspace` with `features = ["full"]` | Async runtime, UnixListener, I/O |
| `serde` | `workspace` with `derive` | JSON serialization |
| `serde_json` | `workspace` | JSON parsing and value manipulation |
| `thiserror` | `workspace` | Structured error types |

All versions are already in `[workspace.dependencies]` in the root `Cargo.toml`.

### Dev dependencies (add to `veil-socket/Cargo.toml` dev-dependencies)

| Dependency | Version | Purpose |
|-----------|---------|---------|
| `tempfile` | `workspace` | Isolated socket paths in tests |
| `tokio` (full) | `workspace` | Test runtime macros |

### New `veil-e2e` dependencies

`veil-e2e/Cargo.toml` already lists `veil-socket`, `tokio`, and `serde_json`. No new dependencies needed.

### Module structure

```
crates/veil-socket/src/
├── lib.rs          (public API surface)
├── rpc.rs          (Unit 1: JSON-RPC types)
├── transport.rs    (Unit 2: SocketListener)
├── dispatcher.rs   (Unit 3: Dispatcher)
├── connection.rs   (Unit 6: handle_connection)
├── server.rs       (Unit 7: SocketServer)
└── handlers/
    ├── mod.rs
    ├── workspace.rs (Unit 4: workspace handlers)
    └── stub.rs      (Unit 5: not_implemented)
```
