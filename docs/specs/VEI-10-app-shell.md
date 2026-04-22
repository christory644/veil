# VEI-10: App Shell — winit Event Loop + State Management

## Context

The app shell is the backbone of the Veil application: window creation, the main event loop, centralized state management, keyboard dispatch, and application lifecycle. Everything else — terminal rendering, sidebar UI, session aggregation — plugs into this shell.

Veil uses a **hybrid architecture** (per the system design doc): centralized `AppState` for UI rendering + actor-based background I/O subsystems communicating via message channels. The UI thread reads `AppState` each frame to render. Background actors (PTY manager, session aggregator, socket API, config watcher) push `StateUpdate` messages to `AppState` via tokio mpsc channels. User events flow back to actors via command channels.

This task establishes the foundational types and wiring. It does NOT include the actual actors (PTY, aggregator, socket API, config watcher), the rendering pipeline (wgpu), or the sidebar UI (egui). Those subsystems will plug into the channel infrastructure and `AppState` defined here.

### Design decision: thin platform shell + fat testable core

All state management, message routing, keyboard dispatch, and focus tracking live in **veil-core** with zero winit/wgpu dependencies. The **veil** binary crate contains only a thin shell: winit window creation, the event loop, and translation from winit events into domain types that veil-core understands. This means:

- All state logic is unit-testable without a display context
- The untestable surface is minimal (~100 lines of event translation in the binary crate)
- The pattern matches the existing codebase (veil-core holds shared types, binary crate is the entry point)

### What already exists

- `veil-core::session` — `SessionId`, `SessionEntry`, `SessionPreview`, `AgentKind`, `SessionStatus`, `SessionSearchResult`
- `veil-core/Cargo.toml` — depends on thiserror, tracing, serde, chrono
- `veil/src/main.rs` — prints version, no real logic yet
- `veil/Cargo.toml` — depends on all crate siblings + anyhow + tracing
- Workspace `Cargo.toml` — tokio (full features) is already a workspace dependency

### What this task creates

New modules in veil-core:
- `workspace.rs` — Workspace, Pane, PaneLayout, SplitDirection types
- `state.rs` — AppState, SidebarState, SidebarTab, NotificationEntry, ConversationIndex
- `message.rs` — StateUpdate, AppCommand enums
- `keyboard.rs` — KeyAction, KeyBinding, KeybindingRegistry, Modifiers
- `focus.rs` — FocusTarget, FocusManager, KeyRoute, route_key_event
- `lifecycle.rs` — ShutdownSignal, ShutdownHandle

Updated modules in veil (binary crate):
- `main.rs` — winit event loop, window creation, domain event translation

## Implementation Units

### Unit 1: Workspace and pane types (`veil-core::workspace`)

Core types for workspaces and their pane layouts. A workspace contains a tree of panes arranged via splits. Each pane holds a surface ID (opaque handle to a terminal surface managed by veil-ghostty/veil-pty later).

**File:** `crates/veil-core/src/workspace.rs`

**Types:**

```rust
/// Unique identifier for a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceId(u64);

/// Unique identifier for a pane within a workspace.
/// Implements Display for error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(u64);

/// Unique identifier for a terminal surface.
/// Opaque handle — the actual surface is managed by veil-ghostty/veil-pty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId(u64);

/// Direction of a pane split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal, // side by side
    Vertical,   // top and bottom
}

/// Tree structure representing pane layout within a workspace.
#[derive(Debug, Clone, PartialEq)]
pub enum PaneNode {
    /// A leaf node containing a single terminal surface.
    Leaf { pane_id: PaneId, surface_id: SurfaceId },
    /// An interior node splitting two children.
    Split {
        direction: SplitDirection,
        /// Fraction of space allocated to the first child (0.0..=1.0).
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

/// A workspace: a named collection of panes with a layout tree.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub working_directory: PathBuf,
    pub layout: PaneNode,
    /// Git branch if applicable (detected, not managed by Veil).
    pub branch: Option<String>,
}
```

**Key behaviors on `PaneNode`:**
- `pane_ids(&self) -> Vec<PaneId>` — collect all pane IDs in the tree
- `surface_ids(&self) -> Vec<SurfaceId>` — collect all surface IDs
- `find_pane(&self, id: PaneId) -> Option<&PaneNode>` — locate a pane by ID
- `pane_count(&self) -> usize` — count leaf nodes

**Key behaviors on `Workspace`:**
- `new(id, name, working_directory, initial_pane_id, initial_surface_id) -> Self` — create with a single pane
- `split_pane(pane_id, direction, new_pane_id, new_surface_id) -> Result<(), WorkspaceError>` — split an existing pane
- `close_pane(pane_id) -> Result<Option<SurfaceId>, WorkspaceError>` — remove a pane, returns the closed surface ID. If it was the last pane, returns error.
- `pane_ids(&self) -> Vec<PaneId>` — delegate to layout

**ID generation:** `WorkspaceId`, `PaneId`, and `SurfaceId` use `new(u64)` constructors. The caller (AppState or a future ID generator) is responsible for providing unique values. No global counter or atomics in the types themselves.

**Error type:**

```rust
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("pane {0} not found")]
    PaneNotFound(PaneId),
    #[error("cannot close the last pane in workspace")]
    LastPane,
}
```

**Tests:**

- Create a workspace with a single pane; verify `pane_count() == 1`
- Split a pane horizontally; verify `pane_count() == 2` and both surface IDs present
- Split a pane vertically; verify layout structure
- Split a pane that doesn't exist; returns `PaneNotFound`
- Close a pane in a two-pane workspace; verify `pane_count() == 1` and correct surface ID returned
- Close the last pane; returns `LastPane` error
- `pane_ids()` returns all IDs in a nested split tree
- `surface_ids()` returns all surface IDs
- `find_pane()` locates a leaf in a nested tree
- `find_pane()` returns None for nonexistent ID
- Split ratio is clamped or validated to (0.0, 1.0) exclusive
- Deep nesting: split 5+ times, verify tree integrity

---

### Unit 2: AppState and sidebar types (`veil-core::state`)

The central state struct that the UI reads each frame. Single source of truth for rendering.

**File:** `crates/veil-core/src/state.rs`

**Types:**

```rust
use crate::session::SessionEntry;
use crate::workspace::{Workspace, WorkspaceId, PaneId, SurfaceId};

/// Which tab is active in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTab {
    #[default]
    Workspaces,
    Conversations,
}

/// Sidebar display state.
#[derive(Debug, Clone)]
pub struct SidebarState {
    pub visible: bool,
    pub active_tab: SidebarTab,
    pub width_px: u32,
}

/// A notification displayed in the UI.
#[derive(Debug, Clone)]
pub struct NotificationEntry {
    pub id: u64,
    pub workspace_id: WorkspaceId,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub acknowledged: bool,
}

/// Indexed conversation data for the Conversations tab.
/// Holds session entries grouped by agent, ready for UI rendering.
#[derive(Debug, Clone, Default)]
pub struct ConversationIndex {
    pub sessions: Vec<SessionEntry>,
}

/// Central application state — single source of truth for UI rendering.
///
/// Note: Focus management lives in the separate `FocusManager` (Unit 5),
/// not in AppState. The event loop owns both and coordinates between them.
/// AppState tracks *what* exists; FocusManager tracks *where* the keyboard goes.
#[derive(Debug)]
pub struct AppState {
    pub workspaces: Vec<Workspace>,
    pub active_workspace_id: Option<WorkspaceId>,
    pub conversations: ConversationIndex,
    pub notifications: Vec<NotificationEntry>,
    pub sidebar: SidebarState,
    next_id: u64,
}
```

**Key behaviors on `AppState`:**
- `new() -> Self` — create with default sidebar state, no workspaces
- `create_workspace(name, working_directory) -> WorkspaceId` — creates a workspace with one pane, sets it active if no workspace exists
- `close_workspace(id) -> Result<Vec<SurfaceId>, StateError>` — remove workspace, returns surface IDs to clean up. Activates adjacent workspace if closing the active one.
- `active_workspace(&self) -> Option<&Workspace>` — get the active workspace
- `active_workspace_mut(&mut self) -> Option<&mut Workspace>` — mutable access
- `workspace(&self, id: WorkspaceId) -> Option<&Workspace>` — lookup by ID
- `set_active_workspace(id) -> Result<(), StateError>` — switch active workspace
- `split_pane(workspace_id, pane_id, direction) -> Result<(PaneId, SurfaceId), StateError>` — split a specific pane in a workspace, returns new pane/surface IDs
- `close_pane(workspace_id, pane_id) -> Result<Option<SurfaceId>, StateError>` — close a specific pane, returns the removed surface ID
- `toggle_sidebar()` — toggle sidebar visibility
- `set_sidebar_tab(tab)` — switch sidebar tab
- `add_notification(workspace_id, message)` — push a notification
- `acknowledge_notification(id)` — mark a notification as acknowledged
- `update_conversations(sessions: Vec<SessionEntry>)` — replace conversation index
- `next_id(&mut self) -> u64` — monotonically increasing ID generator for workspace/pane/surface IDs

**Error type:**

```rust
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("workspace {0:?} not found")]
    WorkspaceNotFound(WorkspaceId),
    #[error("no active workspace")]
    NoActiveWorkspace,
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
}
```

**Tests:**

- `new()` starts with empty workspaces, no active workspace, sidebar visible with Workspaces tab
- `create_workspace` returns a valid ID and workspace appears in list
- First created workspace becomes active automatically
- Second created workspace does NOT change active
- `close_workspace` removes it and returns surface IDs
- Closing active workspace activates the previous one (or next if first)
- Closing nonexistent workspace returns error
- `set_active_workspace` switches active workspace
- `set_active_workspace` with invalid ID returns error
- `split_pane` splits the specified pane, returns new pane and surface IDs
- `split_pane` with nonexistent workspace returns error
- `split_pane` with nonexistent pane returns error
- `close_pane` removes the specified pane and returns its surface ID
- `close_pane` on last pane returns `LastPane` error (via WorkspaceError)
- `toggle_sidebar` flips visibility
- `set_sidebar_tab` changes active tab
- `add_notification` pushes to notification list
- `acknowledge_notification` marks it acknowledged
- `update_conversations` replaces session list
- `next_id` returns monotonically increasing values

---

### Unit 3: Message channel infrastructure (`veil-core::message`)

Defines the message types that flow between actors and AppState. The channel creation and wiring is also defined here, but the actual sending/receiving happens in the event loop (binary crate) and actors (future tasks).

**File:** `crates/veil-core/src/message.rs`

**Types:**

```rust
use crate::session::SessionEntry;
use crate::workspace::{WorkspaceId, PaneId, SurfaceId, SplitDirection};

/// Messages sent from background actors to update AppState.
/// The main event loop receives these and applies them to AppState.
#[derive(Debug)]
pub enum StateUpdate {
    /// Session aggregator discovered/updated conversations.
    ConversationsUpdated(Vec<SessionEntry>),
    /// A notification arrived (from PTY OSC, socket API, etc.).
    NotificationReceived {
        workspace_id: WorkspaceId,
        message: String,
    },
    /// A terminal surface's process exited.
    SurfaceExited {
        surface_id: SurfaceId,
        exit_code: Option<i32>,
    },
    /// Config was reloaded from disk.
    ConfigReloaded(Box<SidebarConfig>),
    /// An actor encountered a non-fatal error worth surfacing.
    ActorError {
        actor_name: String,
        message: String,
    },
}

/// Sidebar-related config that can be hot-reloaded.
/// Uses `SidebarTab` from `crate::state`.
#[derive(Debug, Clone)]
pub struct SidebarConfig {
    pub default_tab: SidebarTab,
    pub width_px: u32,
    pub visible: bool,
}

/// Commands sent from the UI thread to background actors.
/// Clone is required by tokio::sync::broadcast.
#[derive(Debug, Clone)]
pub enum AppCommand {
    /// Request the aggregator to re-scan sessions.
    RefreshConversations,
    /// Send input bytes to a terminal surface's PTY.
    SendInput {
        surface_id: SurfaceId,
        data: Vec<u8>,
    },
    /// Resize a terminal surface.
    ResizeSurface {
        surface_id: SurfaceId,
        cols: u16,
        rows: u16,
    },
    /// Create a new PTY surface (shell spawn).
    SpawnSurface {
        surface_id: SurfaceId,
        working_directory: std::path::PathBuf,
    },
    /// Close a PTY surface.
    CloseSurface {
        surface_id: SurfaceId,
    },
    /// Initiate graceful shutdown.
    Shutdown,
}

/// Bundle of channel endpoints for wiring actors to the main loop.
pub struct Channels {
    /// Sender for actors to push state updates.
    pub state_tx: tokio::sync::mpsc::Sender<StateUpdate>,
    /// Receiver for the main loop to consume state updates.
    pub state_rx: tokio::sync::mpsc::Receiver<StateUpdate>,
    /// Sender for the main loop to push commands to actors.
    pub command_tx: tokio::sync::broadcast::Sender<AppCommand>,
}
```

Note: `AppCommand` uses `broadcast` so multiple actors can subscribe to the command stream and filter for relevant commands. `StateUpdate` uses `mpsc` since there's a single consumer (the main loop).

**Key behaviors:**
- `Channels::new(buffer_size: usize) -> Self` — create the channel pairs
- `Channels::command_subscriber(&self) -> broadcast::Receiver<AppCommand>` — create a new subscriber for the command channel

**Dependencies:** tokio (already a workspace dependency with `full` features, which includes `sync`).

**Tests:**

- `Channels::new` creates valid channel pair
- Send a `StateUpdate` through `state_tx`, receive it from `state_rx`
- Send an `AppCommand` through `command_tx`, receive it from subscriber
- Multiple command subscribers each receive the same message
- `StateUpdate` variants can be pattern-matched and destructured
- `AppCommand` variants can be pattern-matched and destructured
- Channel respects buffer size: sending beyond capacity without receiving returns `TrySendError` (for mpsc) or `SendError` (for broadcast lagged)
- Dropping all senders closes the receiver (recv returns None)
- Dropping the receiver: sender's `send` returns error

---

### Unit 4: Keyboard dispatch (`veil-core::keyboard`)

A configurable keybinding registry that maps key combinations to actions. Platform-agnostic — the binary crate translates winit key events into the domain `KeyInput` type.

**File:** `crates/veil-core/src/keyboard.rs`

**Types:**

```rust
/// Modifier keys held during a key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// Cmd on macOS, Win/Super on Linux/Windows.
    pub logo: bool,
}

/// A physical or logical key identifier.
/// Uses string-based key names for configurability (maps from winit's key codes).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    /// A named key (Enter, Tab, Escape, F1-F12, Arrow keys, etc.)
    Named(String),
    /// A character key.
    Character(char),
}

/// A complete key input event.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyInput {
    pub key: Key,
    pub modifiers: Modifiers,
}

/// Actions that the keyboard dispatch system can trigger.
/// These are application-level actions, not raw key events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    // Workspace actions
    SwitchWorkspace(u8),       // 1-9
    CreateWorkspace,
    CloseWorkspace,

    // Pane actions
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    FocusNextPane,
    FocusPreviousPane,
    ZoomPane,

    // Sidebar actions
    ToggleSidebar,
    SwitchToWorkspacesTab,
    SwitchToConversationsTab,

    // Navigation
    FocusSidebar,
    FocusTerminal,
}

/// A binding from a key combination to an action.
#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub input: KeyInput,
    pub action: KeyAction,
}

/// Registry of keybindings. Supports lookup by key input.
pub struct KeybindingRegistry {
    bindings: Vec<KeyBinding>,
}
```

**Key behaviors on `KeybindingRegistry`:**
- `new() -> Self` — empty registry
- `with_defaults() -> Self` — registry populated with default keybindings (from UI design doc)
- `bind(input, action)` — add or replace a binding
- `unbind(input) -> Option<KeyAction>` — remove a binding, returns the old action
- `lookup(input) -> Option<&KeyAction>` — find the action for a key input
- `all_bindings(&self) -> &[KeyBinding]` — list all bindings
- `clear()` — remove all bindings

**Default bindings** (from UI design doc, using `logo` for Cmd/Super):
- `Logo+1` through `Logo+9` -> `SwitchWorkspace(1..9)`
- `Logo+N` -> `CreateWorkspace`
- `Logo+D` -> `SplitHorizontal`
- `Logo+Shift+D` -> `SplitVertical`
- `Logo+W` -> `ClosePane`
- `Logo+[` -> `FocusPreviousPane`
- `Logo+]` -> `FocusNextPane`
- `Logo+Shift+Enter` -> `ZoomPane`
- `Logo+B` -> `ToggleSidebar`
- `Ctrl+Shift+W` -> `SwitchToWorkspacesTab`
- `Ctrl+Shift+C` -> `SwitchToConversationsTab`

**Tests:**

- Empty registry: `lookup` returns None for any input
- `with_defaults` populates expected bindings
- `bind` + `lookup` round-trip
- `bind` replaces existing binding for same input
- `unbind` returns the old action and removes it
- `unbind` nonexistent key returns None
- `lookup` with wrong modifiers returns None (Ctrl+N != Logo+N)
- `lookup` is case-sensitive for character keys
- `all_bindings` returns all registered bindings
- `clear` empties the registry
- Default bindings include all shortcuts from the UI design doc
- `SwitchWorkspace` bindings cover 1-9
- Named keys (Enter, Escape, etc.) work in lookups
- Character keys work in lookups

---

### Unit 5: Focus management (`veil-core::focus`)

Tracks which UI element has keyboard focus. Focus determines where key events are routed: to the keybinding registry (global shortcuts), to the sidebar (navigation keys), or pass-through to a terminal surface.

**File:** `crates/veil-core/src/focus.rs`

**Types:**

```rust
use crate::workspace::SurfaceId;

/// Where keyboard focus currently lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    /// A terminal surface (keys pass through to PTY).
    Surface(SurfaceId),
    /// The sidebar (keys navigate the sidebar UI).
    Sidebar,
}

/// Manages keyboard focus state.
#[derive(Debug)]
pub struct FocusManager {
    current: Option<FocusTarget>,
}
```

**Key behaviors:**
- `new() -> Self` — no focus initially
- `current(&self) -> Option<FocusTarget>` — get current focus target
- `focus_surface(id: SurfaceId)` — set focus to a terminal surface
- `focus_sidebar()` — set focus to the sidebar
- `is_surface_focused(&self) -> bool` — true if focus is on any surface
- `focused_surface(&self) -> Option<SurfaceId>` — get the focused surface ID, if any
- `clear()` — clear focus (e.g., during workspace transitions)

Focus routing logic (pure function, used by the event loop):

```rust
/// Determine how a key event should be handled based on current focus.
#[derive(Debug, PartialEq, Eq)]
pub enum KeyRoute {
    /// Dispatch as a global action.
    Action(KeyAction),
    /// Forward to the focused terminal surface as raw input.
    ForwardToSurface(SurfaceId),
    /// Forward to the sidebar for navigation.
    ForwardToSidebar,
    /// No focus target; drop the event.
    Unhandled,
}

/// Route a key event: check global shortcuts first, then forward to focus target.
pub fn route_key_event(
    input: &KeyInput,
    registry: &KeybindingRegistry,
    focus: &FocusManager,
) -> KeyRoute;
```

Global shortcuts take priority over focus target forwarding. If the key matches a global binding, it becomes an `Action`. Otherwise, it's forwarded to whatever has focus.

**Tests:**

- No focus: all non-global keys are `Unhandled`
- Surface focused + non-global key: `ForwardToSurface(id)`
- Sidebar focused + non-global key: `ForwardToSidebar`
- Global shortcut takes priority over surface focus (Logo+N -> Action even when surface focused)
- Global shortcut takes priority over sidebar focus
- `focus_surface` then `focused_surface` returns correct ID
- `focus_sidebar` then `is_surface_focused` returns false
- `clear` then `current` returns None
- Focus transitions: surface -> sidebar -> surface tracks correctly

---

### Unit 6: Application lifecycle (`veil-core::lifecycle`)

Coordinates graceful startup and shutdown. On shutdown: signal all actors to stop, flush logs, optionally save state.

**File:** `crates/veil-core/src/lifecycle.rs`

**Types:**

```rust
/// Coordinates graceful shutdown across the application.
///
/// The main event loop creates this and passes clones of the shutdown signal
/// to each actor. When shutdown is triggered, all actors observe it and
/// clean up before the process exits.
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    sender: tokio::sync::watch::Sender<bool>,
    receiver: tokio::sync::watch::Receiver<bool>,
}

/// A handle that actors hold to observe shutdown.
#[derive(Debug, Clone)]
pub struct ShutdownHandle {
    receiver: tokio::sync::watch::Receiver<bool>,
}
```

Note: uses `tokio::sync::watch` (single-producer, multi-consumer) because shutdown is a one-shot broadcast signal. `watch` is lighter than broadcast for a single value that changes once.

**Key behaviors on `ShutdownSignal`:**
- `new() -> Self` — create a new signal (initially false)
- `trigger(&self)` — signal shutdown (sets to true)
- `handle(&self) -> ShutdownHandle` — create a handle for an actor to observe
- `is_triggered(&self) -> bool` — check if shutdown has been triggered

**Key behaviors on `ShutdownHandle`:**
- `is_triggered(&self) -> bool` — check if shutdown has been signaled
- `wait(&mut self) -> impl Future` — async wait until shutdown is triggered (wraps `watch::Receiver::changed`)

**Tests:**

- New signal is not triggered
- After `trigger()`, `is_triggered()` returns true
- Handle observes shutdown after trigger
- Multiple handles all observe the same trigger
- `wait()` resolves after trigger (async test with tokio::test)
- `wait()` does not resolve before trigger (use `tokio::time::timeout` to verify)
- Cloning a handle works independently

---

### Unit 7: Thin winit shell (`veil/src/main.rs`)

The binary crate entry point: creates the window, runs the winit event loop, translates platform events into domain types, and wires everything together.

**File:** `crates/veil/src/main.rs`

This unit is **not unit-testable** (requires a display context). It is validated by manual testing and eventually by E2E socket API tests (VEI-11). The code should be kept minimal and auditable.

**Responsibilities:**
1. Initialize tracing subscriber
2. Create `AppState`
3. Create `Channels`
4. Create `ShutdownSignal`
5. Create `KeybindingRegistry::with_defaults()`
6. Create `FocusManager`
7. Create winit `EventLoop` and `Window` (title: "Veil", DPI-aware, initial size from config or 1280x800 default)
8. Run the event loop:
   - `WindowEvent::KeyboardInput` -> translate to `KeyInput` -> `route_key_event` -> dispatch action or forward
   - `WindowEvent::CloseRequested` -> trigger shutdown
   - `WindowEvent::Resized` -> store new size (for future renderer)
   - `WindowEvent::ScaleFactorChanged` -> store new DPI
   - Poll `state_rx` for `StateUpdate` messages -> apply to `AppState`
   - On action dispatch: modify `AppState` and/or send `AppCommand`
9. On shutdown: trigger `ShutdownSignal`, drain channels, exit

**winit key translation:** Map `winit::keyboard::Key` and `winit::event::Modifiers` to `veil_core::keyboard::KeyInput`. This is a straightforward mapping function that can be extracted into a module in the binary crate for readability, though it won't be unit-tested.

**New dependencies for veil binary crate:**
- `winit` (add to workspace dependencies)
- `tokio` (already available)

**No tests for this unit.** Validated by:
- `cargo build -p veil` succeeds
- Manual: running `cargo run` opens a window titled "Veil"
- Future: E2E tests via socket API (VEI-11)

## Test Strategy Summary

| Unit | Test Type | Display Required | Key Coverage |
|------|----------|-----------------|--------------|
| 1. Workspace/pane types | `#[test]` | No | Tree operations, split/close, error cases |
| 2. AppState | `#[test]` | No | CRUD, focus, sidebar, notifications |
| 3. Message channels | `#[tokio::test]` | No | Send/recv, broadcast, backpressure |
| 4. Keyboard dispatch | `#[test]` | No | Lookup, defaults, bind/unbind |
| 5. Focus management | `#[test]` | No | Routing, priority, transitions |
| 6. Lifecycle | `#[tokio::test]` | No | Signal propagation, async wait |
| 7. winit shell | Manual / E2E | Yes | Window creation, event loop |

All unit tests run in CI without a display server. Only Unit 7 requires a window.

## Acceptance Criteria

1. `cargo build -p veil-core` succeeds with all new modules
2. `cargo build -p veil` succeeds with winit integration
3. `cargo test -p veil-core` passes all state, workspace, message, keyboard, focus, and lifecycle tests
4. `cargo clippy --all-targets --all-features -- -D warnings` passes
5. `cargo fmt --check` passes
6. `AppState` can be created, workspaces added/removed, and state queried — all without a display context
7. `KeybindingRegistry::with_defaults()` contains all shortcuts from the UI design doc
8. `route_key_event` correctly prioritizes global shortcuts over focus-target forwarding
9. `ShutdownSignal` propagates to all handles
10. `Channels::new()` creates working mpsc + broadcast channel pairs
11. `PaneNode` tree operations (split, close, find) work correctly on nested layouts
12. `cargo run` opens a window titled "Veil" (manual verification)
13. No winit or wgpu dependency in veil-core

## Dependencies

**New workspace dependencies (add to root `Cargo.toml`):**

| Dependency | Version | Features | Reason |
|-----------|---------|----------|--------|
| winit | 0.30 | — | Window creation and event loop |

winit 0.30 is the current stable release with the new `ApplicationHandler` trait API.

**Crate dependency changes:**

| Crate | Add | Reason |
|-------|-----|--------|
| veil-core | `tokio` (workspace, features = ["sync"]) | watch and mpsc channels for message infrastructure |
| veil (binary) | `winit` (workspace) | Window creation and event loop |
| veil (binary) | `tokio` (workspace) | Async runtime for channel polling |

Note: veil-core only needs tokio's `sync` feature (channels), not the full runtime. The binary crate needs the full runtime. Since `tokio` is already a workspace dep with `features = ["full"]`, veil-core can depend on it with just `["sync"]` and Cargo will unify features at build time.

**No new external tools or system dependencies required.**
