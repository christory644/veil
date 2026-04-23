# VEI-80: Wire Session Aggregator and Socket API as Background Actors

## Context

The session aggregator (`veil-aggregator`) and socket API server (`veil-socket`) are fully implemented as standalone components, but nothing starts them or connects them to the running application. This task wires both into the app lifecycle as background actors following the established pattern set by `ConfigWatcher` in VEI-79.

### Current state of the binary crate (`crates/veil/src/main.rs`)

- `VeilApp` owns `channels: Channels`, `shutdown: ShutdownSignal`, `app_state: AppState`, `app_config: AppConfig`
- `VeilApp::new()` creates `Channels::new(256)` and `ShutdownSignal::new()`
- `resumed()` bootstraps the window, renderer, workspace, PTY, and calls `self.start_config_watcher()`
- `drain_state_updates()` only handles `StateUpdate::ConfigReloaded` -- all other variants are silently dropped
- `CloseRequested` triggers `shutdown.trigger()` and `event_loop.exit()`
- No tokio runtime exists -- the config watcher uses `std::thread` with `blocking_recv()`/`blocking_send()`
- `veil-aggregator` and `veil-socket` are listed as dependencies in `crates/veil/Cargo.toml` but never imported

### What exists in veil-aggregator

- **`SessionStore`** (`store.rs`) -- SQLite (rusqlite) with WAL mode, FTS5 search. Methods: `open(path)`, `open_in_memory()`, `upsert_sessions(&[SessionEntry])`, `list_sessions()`, `search_sessions(query)`, `update_fts()`. **Not `Send`** (rusqlite::Connection is not Send) -- must stay on a single dedicated thread.
- **`AgentAdapter` trait** (`adapter.rs`) -- `Send + Sync`, object-safe. Methods: `name()`, `agent_kind()`, `watch_paths()`, `discover_sessions()`, `session_preview()`.
- **`AdapterRegistry`** (`registry.rs`) -- `new()`, `register(Box<dyn AgentAdapter>)`, `discover_all() -> Vec<SessionEntry>`, `session_preview()`, `all_watch_paths() -> Vec<PathBuf>`.
- **`ClaudeCodeAdapter`** (`claude_code/adapter.rs`) -- `new() -> Option<Self>` (returns None if `~/.claude/projects/` doesn't exist), `with_projects_dir()` for testing. Implements `AgentAdapter`.

### What exists in veil-socket

- **`SocketServer`** (`server.rs`) -- `new(config: ServerConfig, state: Arc<Mutex<AppState>>)`. `run(self, shutdown: ShutdownHandle) -> Result<(), SocketError>` is **async** and requires a tokio runtime.
- **`ServerConfig`** (`server.rs`) -- `default_for_platform()` or `with_path(path)`.
- **`Dispatcher`** (`dispatcher.rs`) -- routes `workspace.*` to real handlers. Routes `session.*`, `surface.*`, `notification.*`, `sidebar.*` to `stub::not_implemented`.
- **`SocketPath::default_for_platform()`** (`transport.rs`) -- `$XDG_RUNTIME_DIR/veil.sock` or `/tmp/veil.sock`.

### What exists in veil-core (already wired)

- **`StateUpdate::ConversationsUpdated(Vec<SessionEntry>)`** (`message.rs:18`) -- defined but nothing ever sends it.
- **`AppCommand::RefreshConversations`** (`message.rs:74`) -- defined but nothing subscribes.
- **`AppState::update_conversations(sessions: Vec<SessionEntry>)`** (`state.rs:257`) -- replaces the conversation index. Exists but never called from drain_state_updates.
- **`Channels { state_tx, state_rx, command_tx }`** (`message.rs:106-113`) -- `state_tx` is cloneable for actors, `command_tx` is broadcast with `command_subscriber()`.
- **`ShutdownSignal`/`ShutdownHandle`** (`lifecycle.rs`) -- `handle()` creates a clone, `wait()` for async, `is_triggered()` for sync polling.

### Reference pattern: ConfigWatcher wiring (VEI-79)

The `start_config_watcher()` method in `main.rs:227-266` establishes the actor wiring pattern:

1. Create a tokio mpsc channel for the actor's events
2. Construct the actor, pass the sender side
3. Call `actor.start(shutdown.handle())`
4. Spawn a `std::thread` bridge that `blocking_recv()`s actor events and `blocking_send()`s them as `StateUpdate` variants through `channels.state_tx`
5. Store the actor in `VeilApp` so it stays alive
6. In `drain_state_updates()`, match on the new `StateUpdate` variant and apply to `AppState`

### Data directory convention

`veil-tracing` uses `dirs::data_dir()` with `veil/` prefix (`~/.local/share/veil/logs/` on Linux, `~/Library/Application Support/veil/logs/` on macOS). The aggregator database should follow the same convention: `dirs::data_dir() / "veil" / "sessions.db"`.

## Implementation Units

### Unit 1: Aggregator actor thread (`crates/veil/src/aggregator_actor.rs`)

Create a new module that encapsulates the aggregator lifecycle on a dedicated `std::thread` (required because `SessionStore` wraps `rusqlite::Connection` which is not `Send`).

**Public function:**

```rust
pub struct AggregatorHandle {
    _thread: std::thread::JoinHandle<()>,
}

/// Start the aggregator actor on a dedicated thread.
///
/// - Opens `SessionStore` at `data_dir / "veil" / "sessions.db"` (or in-memory if
///   `dirs::data_dir()` is unavailable)
/// - Registers `ClaudeCodeAdapter` (if `~/.claude/projects/` exists)
/// - Runs initial `AdapterRegistry::discover_all()`, upserts into store, sends
///   `StateUpdate::ConversationsUpdated` with the full session list
/// - Starts a `notify::RecommendedWatcher` on `registry.all_watch_paths()`
/// - Loops: wait for file-system events or `AppCommand::RefreshConversations`,
///   re-discover, upsert, send updated session list
/// - Exits when `ShutdownHandle::is_triggered()` returns true
///
/// Returns an `AggregatorHandle` that keeps the thread alive.
pub fn start_aggregator(
    state_tx: tokio::sync::mpsc::Sender<StateUpdate>,
    command_rx: tokio::sync::broadcast::Receiver<AppCommand>,
    shutdown: ShutdownHandle,
) -> AggregatorHandle;
```

**Internal structure of the thread function:**

1. Resolve data dir: `dirs::data_dir().map(|d| d.join("veil"))`. Create directory with `fs::create_dir_all`. Open `SessionStore::open(path.join("sessions.db"))`. Fall back to `SessionStore::open_in_memory()` if resolution or open fails.
2. Create `AdapterRegistry::new()`. Call `ClaudeCodeAdapter::new()` -- if `Some`, register it.
3. Run `registry.discover_all()`. Call `store.upsert_sessions(&entries)`. Call `store.list_sessions()` and send `StateUpdate::ConversationsUpdated(sessions)` via `state_tx.blocking_send()`.
4. Collect `registry.all_watch_paths()`. Create a `notify::RecommendedWatcher` watching those paths (recursive). Use `std::sync::mpsc` channel as the notify event receiver.
5. Enter loop:
   - `recv_timeout(100ms)` on the notify channel -- on relevant Create/Modify events, debounce, re-discover, upsert, send updated list
   - `try_recv()` on `command_rx` -- on `AppCommand::RefreshConversations`, re-discover and send
   - Check `shutdown.is_triggered()` -- break if true

**Why a dedicated thread (not tokio task):** `rusqlite::Connection` is `!Send`. The entire store must live on one OS thread. The `notify` crate also works well with `std::sync::mpsc`. This matches the config watcher pattern.

#### Test strategy

- **Unit test: `resolve_data_path` helper** -- test that the path resolution function returns `<data_dir>/veil/sessions.db` or None.
- **Unit test: `run_initial_scan` helper** -- given a `SessionStore` (in-memory) and a mock `AdapterRegistry` with known sessions, verify it upserts and returns the correct session list.
- **Integration test: full actor lifecycle** -- start the aggregator with a mock adapter returning known sessions, verify `StateUpdate::ConversationsUpdated` arrives on the receiver, trigger shutdown, verify the thread joins.
- **Unit test: RefreshConversations command** -- start actor, send `AppCommand::RefreshConversations` via broadcast, verify a new `ConversationsUpdated` is sent.
- **Unit test: graceful shutdown** -- start actor, trigger shutdown, verify thread exits within timeout.

### Unit 2: Socket server actor thread (`crates/veil/src/socket_actor.rs`)

Create a new module that starts the `SocketServer` on a dedicated thread with its own single-threaded tokio runtime.

**Public function:**

```rust
pub struct SocketHandle {
    _thread: std::thread::JoinHandle<()>,
}

/// Start the socket API server on a background thread with its own tokio runtime.
///
/// - Creates a single-threaded `tokio::runtime::Runtime`
/// - Constructs `SocketServer` with `ServerConfig::default_for_platform()` and the
///   shared `Arc<Mutex<AppState>>` (requires wrapping AppState -- see Unit 4)
/// - Calls `server.run(shutdown)` inside `rt.block_on()`
/// - Thread exits when shutdown is triggered and server.run() returns
pub fn start_socket_server(
    app_state: Arc<Mutex<AppState>>,
    shutdown: ShutdownHandle,
) -> SocketHandle;
```

**Why a separate thread with its own runtime:** The main app is a synchronous winit event loop. `SocketServer::run()` is async. Creating a dedicated single-threaded tokio runtime on a background thread is the standard approach for embedding async actors in a sync application. This avoids adding a tokio runtime to the main thread.

**Note on `Arc<Mutex<AppState>>`:** `SocketServer::new()` already takes `Arc<Mutex<AppState>>`. This requires `VeilApp` to share its `AppState` via `Arc<Mutex<>>`. See Unit 4 for this structural change.

#### Test strategy

- **Integration test: server starts and accepts connections** -- start socket actor with a test AppState, connect a Unix socket client, send `workspace.list`, verify response, trigger shutdown, verify thread joins.
- **Unit test: graceful shutdown** -- start actor, immediately trigger shutdown, verify thread exits within timeout.
- **Integration test: server uses default socket path** -- verify the socket file appears at the expected platform path.

### Unit 3: Handle `ConversationsUpdated` in `drain_state_updates()` (`crates/veil/src/main.rs`)

Expand `drain_state_updates()` to handle `StateUpdate::ConversationsUpdated` in addition to the existing `ConfigReloaded` handler.

**Current code (main.rs:269-305):**
```rust
fn drain_state_updates(&mut self) {
    while let Ok(update) = self.channels.state_rx.try_recv() {
        if let StateUpdate::ConfigReloaded { config, delta, warnings } = update {
            // ... existing handler
        }
    }
}
```

**Change:** Replace the `if let` with a `match` that handles both variants:

```rust
fn drain_state_updates(&mut self) {
    while let Ok(update) = self.channels.state_rx.try_recv() {
        match update {
            StateUpdate::ConfigReloaded { config, delta, warnings } => {
                // ... existing handler (unchanged)
            }
            StateUpdate::ConversationsUpdated(sessions) => {
                self.app_state.update_conversations(sessions);
                if let Some(ref window) = self.window {
                    window.request_redraw();
                }
            }
            _ => {
                // Other StateUpdate variants (PtyOutput, SurfaceExited, etc.)
                // will be handled in future wiring tasks.
            }
        }
    }
}
```

This is the minimal change that makes discovered sessions appear in the sidebar. `AppState::update_conversations()` already exists and replaces the `ConversationIndex`. The sidebar's `extract_conversation_groups()` already reads from `AppState.conversations.sessions` and groups/sorts them.

#### Test strategy

Testing this in isolation is not possible without mocking the winit window. However, the individual pieces are already tested:

- `AppState::update_conversations()` -- tested in `state.rs:625-632`
- `extract_conversation_groups()` -- tested in `conversation_list.rs` with real `SessionEntry` data
- The `match` arm itself is trivial (one method call + redraw request)

A **manual smoke test** verifies the end-to-end flow: launch Veil, observe the Conversations tab populating from discovered Claude Code sessions.

### Unit 4: Shared AppState for socket server (`crates/veil/src/main.rs`)

`SocketServer::new()` requires `Arc<Mutex<AppState>>`. Currently `VeilApp` owns `app_state: AppState` directly. Two approaches:

**Option A (recommended): Keep `AppState` owned directly, create a separate `Arc<Mutex<AppState>>` for the socket server.**

The socket server needs access to AppState for workspace operations (create, list, select, etc.). But the main thread also mutates AppState every frame. Sharing a single `Arc<Mutex<AppState>>` would require the main thread to lock/unlock on every frame, which is unacceptable for a 60fps render loop.

Instead, the socket server gets its own `Arc<Mutex<AppState>>` initialized from the same state. Workspace mutations from the socket API would need to be routed through `StateUpdate` messages to stay synchronized. For VEI-80, the socket server's workspace handlers already work against the `Arc<Mutex<AppState>>` they receive. True bidirectional sync between the socket's AppState and the main thread's AppState is a follow-up task (tracked separately).

```rust
// In resumed(), before start_socket_server:
let socket_state = Arc::new(Mutex::new(AppState::new()));
// Bootstrap it with same workspace state:
{
    let mut ss = socket_state.lock().unwrap();
    ss.apply_config(&self.app_config);
}
```

**Option B: Wrap main thread's AppState in `Arc<Mutex<>>`.**

This would require locking on every frame, every input event, every state mutation. Not viable for a real-time renderer.

#### Test strategy

- **Unit test:** Verify that `SocketServer::new()` accepts the `Arc<Mutex<AppState>>` and the server can handle requests against it. This is already tested in `veil-socket` crate tests.
- The structural decision (Option A vs B) is a design choice validated by code review, not unit tests.

### Unit 5: Wire actors in `VeilApp::resumed()` (`crates/veil/src/main.rs`)

Add startup calls in `resumed()` after the existing `start_config_watcher()` call:

```rust
fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    // ... existing window/renderer/workspace/pty setup ...

    self.start_config_watcher();

    // Start aggregator actor
    self.aggregator_handle = Some(aggregator_actor::start_aggregator(
        self.channels.state_tx.clone(),
        self.channels.command_subscriber(),
        self.shutdown.handle(),
    ));

    // Start socket server actor
    let socket_state = Arc::new(tokio::sync::Mutex::new(AppState::new()));
    self.socket_handle = Some(socket_actor::start_socket_server(
        socket_state,
        self.shutdown.handle(),
    ));
}
```

Add fields to `VeilApp`:

```rust
struct VeilApp {
    // ... existing fields ...
    /// Aggregator background actor handle.
    aggregator_handle: Option<aggregator_actor::AggregatorHandle>,
    /// Socket server background actor handle.
    socket_handle: Option<socket_actor::SocketHandle>,
}
```

Add module declarations to `main.rs`:

```rust
mod aggregator_actor;
mod socket_actor;
```

#### Test strategy

- No direct unit test (requires winit event loop). Validated by **compilation** and **manual smoke test**.
- Individual actor startup is tested in Unit 1 and Unit 2.

### Unit 6: Shutdown coordination (`crates/veil/src/main.rs`)

The existing shutdown flow in `CloseRequested` already triggers `self.shutdown.trigger()`. Both actors receive `ShutdownHandle` clones and will observe the trigger:

- **Aggregator thread:** checks `shutdown.is_triggered()` in its poll loop, breaks and returns
- **Socket server:** `SocketServer::run()` uses `tokio::select!` with `shutdown.wait()`, breaks the accept loop, aborts connection tasks

No additional shutdown code is needed beyond what Units 1 and 2 already implement. The `JoinHandle`s stored in `AggregatorHandle` and `SocketHandle` will be dropped when `VeilApp` is dropped, which is fine -- the threads will exit on their own via the shutdown signal. If blocking on join is desired, a `shutdown()` method can be added to the handles, but this is not required for correctness.

#### Test strategy

- **Integration test (aggregator):** start actor, trigger shutdown, verify thread joins within 2 seconds.
- **Integration test (socket):** start actor, trigger shutdown, verify thread joins within 2 seconds.
- **Both already covered in Unit 1 and Unit 2 test strategies.**

### Unit 7: Wire `session.*` handlers in socket dispatcher (DEFERRED)

The `session.list`, `session.search`, and `session.preview` methods in the socket API currently route to `stub::not_implemented`. Wiring them to real implementations requires the `Dispatcher` to hold a reference to the `SessionStore` (or a channel to the aggregator thread). Since `SessionStore` is `!Send` and lives on the aggregator thread, this requires either:

- A channel-based query/response pattern between the socket server and aggregator
- A separate read-only `SessionStore` connection on the socket server's thread

Both approaches add significant complexity beyond "wiring existing pieces." This unit is **deferred to a follow-up task** and noted in Acceptance Criteria as explicitly out of scope.

## Acceptance Criteria

1. **Aggregator starts on app launch:** The aggregator thread starts in `resumed()`, performs initial discovery via `ClaudeCodeAdapter`, and sends `StateUpdate::ConversationsUpdated` with discovered sessions.

2. **Sessions appear in sidebar:** After the aggregator's initial scan, `drain_state_updates()` receives `ConversationsUpdated`, calls `AppState::update_conversations()`, and the Conversations tab renders the discovered sessions grouped by agent.

3. **File watcher re-discovers:** When a new `.jsonl` session file appears in `~/.claude/projects/`, the aggregator's file watcher triggers re-discovery and sends an updated `ConversationsUpdated`.

4. **RefreshConversations command works:** Sending `AppCommand::RefreshConversations` via the broadcast channel causes the aggregator to re-scan and send an updated session list.

5. **Socket server starts:** The socket server binds to the platform-default path and accepts connections. `workspace.list` and `workspace.create` work against the socket server's AppState.

6. **Graceful shutdown:** Triggering `ShutdownSignal` (via window close) causes both the aggregator thread and socket server thread to exit cleanly.

7. **No crash on missing data:** If `~/.claude/projects/` doesn't exist, the aggregator starts with no adapters and sends an empty session list. If `dirs::data_dir()` is unavailable, the aggregator falls back to in-memory SessionStore.

8. **Session database persisted:** Discovered sessions are stored in `<data_dir>/veil/sessions.db` and survive app restarts (second launch shows sessions immediately from the store, then updates from a fresh scan).

9. **Out of scope:**
   - `session.*` socket API handlers remain stubbed (follow-up task)
   - Bidirectional AppState sync between main thread and socket server (follow-up task)
   - Live state resolution in the sidebar (VEI-23 handles this)
   - Session preview loading on conversation selection (follow-up task)

## Dependencies

### Crate dependencies (already in Cargo.toml)

- `veil-aggregator` -- already a dependency of `veil` (`crates/veil/Cargo.toml:20`)
- `veil-socket` -- already a dependency of `veil` (`crates/veil/Cargo.toml:21`)
- `notify` -- already a workspace dependency (`Cargo.toml:47`), used by `veil-core` for config watcher. Will need to be added to `veil-aggregator/Cargo.toml` for the file watcher in the aggregator actor (or the watcher can live in the `veil` crate itself, using the existing `notify` dependency via `veil-core`).
- `dirs` -- already a dependency of `veil-aggregator` (`crates/veil-aggregator/Cargo.toml:23`)
- `tokio` -- already a dependency of `veil` (`crates/veil/Cargo.toml:26`). Needed for `tokio::runtime::Runtime::new()` in the socket actor thread.

### Task dependencies

- **VEI-79 (Wire Config System):** Completed. Provides the `start_config_watcher()` reference pattern and the `drain_state_updates()` infrastructure.
- **VEI-14 (Session Aggregator):** Completed. Provides `SessionStore`, `AgentAdapter`, `AdapterRegistry`.
- **VEI-15 (Claude Code Adapter):** Completed. Provides `ClaudeCodeAdapter`.
- **VEI-16 (Socket API):** Completed. Provides `SocketServer`, `Dispatcher`, handlers, transport.
- **VEI-13 (Conversations Tab):** Completed. Provides `extract_conversation_groups()` and `render_conversations_tab()` which already consume `AppState.conversations.sessions`.
- **VEI-74 (App Bootstrap):** Completed. Provides `init_default_workspace()` and the `resumed()` lifecycle.

### New files created

- `crates/veil/src/aggregator_actor.rs` -- aggregator thread lifecycle (Unit 1)
- `crates/veil/src/socket_actor.rs` -- socket server thread lifecycle (Unit 2)

### Files modified

- `crates/veil/src/main.rs` -- add module declarations, add `VeilApp` fields, wire actors in `resumed()`, expand `drain_state_updates()` (Units 3, 5, 6)
- `crates/veil-aggregator/Cargo.toml` -- add `notify` dependency if the file watcher lives in the aggregator crate (optional, could live in the actor module in `veil` crate instead)
