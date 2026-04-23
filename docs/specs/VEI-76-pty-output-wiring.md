# VEI-76: Wire PTY Output to libghosty Terminal State

## Context

PTY processes spawn and their output is read into `PtyEvent::Output(Vec<u8>)` chunks by the background read thread in `PosixPty`. The `PtyManager` bridge loop currently discards this output:

```rust
PtyEvent::Output(_) => {
    // Output routing is handled elsewhere (e.g. libghosty integration).
}
```

Meanwhile, `veil_ghostty::Terminal` instances exist with a `write_vt(data: &[u8])` method that feeds raw bytes through the VT parser and updates terminal state (cells, cursor, colors, scrollback). The frame builder in `crates/veil/src/frame.rs` uses hardcoded `DEFAULT_COLS`/`DEFAULT_ROWS` and static colors because no real terminal state is connected.

This task bridges the gap: PTY output bytes flow into `Terminal::write_vt()`, and the resulting terminal state becomes available for rendering.

### What exists

- **`veil_ghostty::Terminal`** -- `write_vt(&mut self, data: &[u8])`, `resize(&mut self, cols, rows, cell_w, cell_h)`, `cursor_x()`, `cursor_y()`, `cursor_visible()`, `cols()`, `rows()`, `title()`, `pwd()`, `active_screen()`, `reset()`. Requires real libghosty FFI; not usable in unit tests without the C library.
- **`veil_ghostty::RenderState`** -- `update(&mut self, terminal: &mut Terminal)`, `dirty()`, `set_dirty()`, `cursor()`, `colors()`, `cols()`, `rows()`. Also requires FFI.
- **`veil_pty::Pty` trait** -- `take_event_rx() -> Option<Receiver<PtyEvent>>`, `writer() -> Sender<Vec<u8>>`, `resize(PtySize)`, `child_pid()`, `shutdown()`, `is_closed()`.
- **`PtyEvent`** -- `Output(Vec<u8>)` for data, `ChildExited { exit_code: Option<i32> }` for exit.
- **`PtyManager`** (`veil_pty::manager`) -- Keyed by `SurfaceId`. Has `spawn()`, `write()`, `resize()`, `close()`, `shutdown_all()`, `handle_command()`. The `bridge_loop` reads `PtyEvent`s and currently only forwards `ChildExited` as `StateUpdate::SurfaceExited`. `PtyEvent::Output` is ignored.
- **`StateUpdate`** (`veil_core::message`) -- Variants: `ConversationsUpdated`, `NotificationReceived`, `SurfaceExited`, `ConfigReloaded`, `ActorError`, `ErrorOccurred`, `ErrorDismissed`, `UpdateAvailable`. No variant for PTY output or redraw requests.
- **`Channels`** -- `state_tx: tokio::sync::mpsc::Sender<StateUpdate>`, `state_rx`, `command_tx: broadcast::Sender<AppCommand>`.
- **`VeilApp`** (`crates/veil/src/main.rs`) -- Owns `app_state`, `focus`, `pty_manager`, `channels`, `renderer`, `window`. The `window_event` handler processes `RedrawRequested` by calling `build_frame_geometry()` and `renderer.render()`. Currently calls `window.request_redraw()` every frame unconditionally.
- **`frame::build_frame_geometry()`** -- Uses `DEFAULT_COLS`/`DEFAULT_ROWS` and static `BG_COLOR`/`CURSOR_COLOR` constants. No terminal state input.
- **`SurfaceId`** -- Opaque `u64` wrapper. Used as the key for `PtyManager`'s `HashMap<SurfaceId, ManagedPty>` and in focus/workspace structures.

### What's missing

1. **PTY output forwarding** -- The `PtyManager` bridge loop ignores `PtyEvent::Output`. These bytes need to reach a `Terminal::write_vt()` call for the corresponding surface.
2. **Terminal ownership map** -- There is no `HashMap<SurfaceId, Terminal>` anywhere. Each surface needs its own `Terminal` instance.
3. **Output-to-terminal pipeline** -- A mechanism to receive PTY output events and call `terminal.write_vt(data)` on the correct terminal instance. Must be thread-safe since PTY read loops run on background threads.
4. **Redraw signaling** -- After PTY output updates terminal state, the main thread needs to know it should re-render. Currently `window.request_redraw()` is called unconditionally every frame, but the architecture needs a redraw signal path for when terminal state changes.
5. **Resize propagation** -- When pane layout changes (window resize, split, zoom), both `terminal.resize(cols, rows, cell_w, cell_h)` and `pty.resize(PtySize)` need to be called. Currently only `pty.resize()` is supported via `PtyManager::resize()`.
6. **Exit handling integration** -- When `SurfaceExited` arrives, the terminal should be cleaned up from the map and the pane should be marked as exited or closed.

### Design decisions

**Thread-safe output pipeline via channels, not shared mutexes on Terminal.**

`Terminal` is not `Send` (it wraps a raw FFI pointer). It cannot live behind `Arc<Mutex<Terminal>>` shared across threads. Instead, the PTY bridge thread sends output bytes over a channel, and the main thread (which owns the terminals) drains the channel and calls `write_vt()`.

The existing `StateUpdate` channel is the natural conduit. We add a `StateUpdate::PtyOutput { surface_id, data }` variant. The bridge loop (which already forwards `ChildExited`) also forwards `Output` chunks. The main event loop drains `StateUpdate`s, and for `PtyOutput`, calls `terminal_map[surface_id].write_vt(&data)`.

**Terminal map lives in the binary crate, not veil-pty or veil-core.**

The `Terminal` type requires libghosty FFI, so it belongs in the binary crate (or a thin integration module) alongside the renderer that consumes its state. `veil-core` stays FFI-free. `veil-pty` stays terminal-free.

**Resize propagation through a trait abstraction.**

To make resize logic testable without FFI, we define a `TerminalWriter` trait with `write_vt(&mut self, data: &[u8])` and `resize(&mut self, cols, rows, cell_w, cell_h) -> Result<(), ...>`. The real implementation wraps `veil_ghostty::Terminal`. Tests use a mock. The `TerminalMap` (the `HashMap<SurfaceId, Box<dyn TerminalWriter>>`) is owned by the binary crate's event loop.

## Implementation Units

### Unit 1: Add `StateUpdate::PtyOutput` variant and forward output in bridge loop

**Location:** `crates/veil-core/src/message.rs` (add variant), `crates/veil-pty/src/manager.rs` (modify bridge loop)

**What it does:**

Add a new `StateUpdate::PtyOutput { surface_id: SurfaceId, data: Vec<u8> }` variant to carry PTY output from the bridge thread to the main loop. Modify `PtyManager::bridge_loop()` to forward `PtyEvent::Output` as `StateUpdate::PtyOutput` instead of ignoring it.

**Changes:**

1. In `crates/veil-core/src/message.rs`, add to `StateUpdate`:
   ```rust
   /// PTY output bytes for a terminal surface.
   PtyOutput {
       /// Which surface produced the output.
       surface_id: SurfaceId,
       /// Raw output bytes from the PTY.
       data: Vec<u8>,
   },
   ```

2. In `crates/veil-pty/src/manager.rs`, modify `bridge_loop()`:
   ```rust
   PtyEvent::Output(data) => {
       let update = StateUpdate::PtyOutput { surface_id, data };
       if let Err(e) = state_tx.blocking_send(update) {
           tracing::warn!(?surface_id, "failed to forward PtyOutput: {e}");
       }
   }
   ```

**Test strategy:**

- **Happy path:** Spawn a mock PTY, send `PtyEvent::Output(b"hello")` through the test handle, verify `StateUpdate::PtyOutput { surface_id, data: b"hello" }` arrives on `state_rx`. (Follows the pattern of the existing `child_exit_sends_surface_exited_state_update` test.)
- **Multiple chunks:** Send several `Output` events, verify all arrive as separate `PtyOutput` messages in order.
- **Output followed by exit:** Send `Output` then `ChildExited`, verify both arrive: `PtyOutput` first, then `SurfaceExited`.
- **Channel backpressure:** Fill the `state_tx` channel to capacity, verify the bridge loop logs a warning but does not panic or deadlock (uses `blocking_send` which will block, so this is more of a design note than a test -- the channel buffer should be large enough in practice).
- **Round-trip variant matching:** Verify `StateUpdate::PtyOutput` can be sent and received through `Channels`, matching the existing pattern in `message.rs` tests.

### Unit 2: `TerminalWriter` trait and `TerminalMap`

**Location:** `crates/veil/src/terminal_map.rs` (new module)

**What it does:**

Defines the `TerminalWriter` trait as an abstraction over `veil_ghostty::Terminal`, and `TerminalMap` as a `HashMap<SurfaceId, Box<dyn TerminalWriter>>` with convenience methods. This is the core data structure that the event loop uses to route PTY output to the correct terminal instance.

**Types:**

```rust
/// Abstraction over terminal state management.
/// Real impl wraps veil_ghostty::Terminal. Tests use a mock.
pub trait TerminalWriter {
    /// Feed VT-encoded bytes to the terminal's parser.
    fn write_vt(&mut self, data: &[u8]);

    /// Resize the terminal to new cell dimensions and pixel sizes.
    fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<(), String>;

    /// Query the terminal's current column count.
    fn cols(&self) -> u16;

    /// Query the terminal's current row count.
    fn rows(&self) -> u16;
}

/// Manages Terminal instances keyed by SurfaceId.
pub struct TerminalMap {
    terminals: HashMap<SurfaceId, Box<dyn TerminalWriter>>,
}

impl TerminalMap {
    pub fn new() -> Self;

    /// Insert a terminal for a surface. Returns the old terminal if one existed.
    pub fn insert(&mut self, surface_id: SurfaceId, terminal: Box<dyn TerminalWriter>)
        -> Option<Box<dyn TerminalWriter>>;

    /// Remove a terminal for a surface.
    pub fn remove(&mut self, surface_id: SurfaceId) -> Option<Box<dyn TerminalWriter>>;

    /// Feed VT data to the terminal for a surface. Returns false if surface not found.
    pub fn write_vt(&mut self, surface_id: SurfaceId, data: &[u8]) -> bool;

    /// Resize the terminal for a surface. Returns Err if surface not found.
    pub fn resize(
        &mut self,
        surface_id: SurfaceId,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<(), String>;

    /// Get a reference to a terminal.
    pub fn get(&self, surface_id: SurfaceId) -> Option<&dyn TerminalWriter>;

    /// Get a mutable reference to a terminal.
    pub fn get_mut(&mut self, surface_id: SurfaceId) -> Option<&mut Box<dyn TerminalWriter>>;

    /// Number of active terminals.
    pub fn len(&self) -> usize;

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool;
}
```

**Test strategy (all use MockTerminalWriter, no FFI):**

- **Insert and retrieve:** Insert a mock terminal for `SurfaceId(1)`, verify `get()` returns it, `len()` is 1.
- **Write_vt routes to correct surface:** Insert two terminals, call `write_vt(SurfaceId(1), b"hello")`, verify only terminal 1's mock recorded the write.
- **Write_vt unknown surface returns false:** Call `write_vt` on a `SurfaceId` not in the map, verify it returns `false`.
- **Remove cleans up:** Insert and remove, verify `get()` returns `None` and `len()` is 0.
- **Resize routes to correct surface:** Insert two terminals, resize one, verify only that terminal's mock recorded the resize.
- **Resize unknown surface returns error:** Resize a `SurfaceId` not in the map, verify `Err`.
- **Insert replaces existing:** Insert twice for the same `SurfaceId`, verify the second terminal replaces the first (returns `Some(old)`).
- **is_empty on fresh map:** Verify `is_empty()` is true on a new map.

### Unit 3: Wire PTY output into terminal map in the event loop

**Location:** `crates/veil/src/main.rs` (modify `VeilApp`)

**What it does:**

Adds `TerminalMap` to `VeilApp`. When the app creates a PTY (in `resumed()` and `execute_effect(SpawnPty)`), it also creates a `Terminal` and inserts it into the `TerminalMap`. Drains `StateUpdate::PtyOutput` messages from the channel and calls `terminal_map.write_vt()` for each. Also handles `StateUpdate::SurfaceExited` by removing the terminal from the map.

**Changes:**

1. Add `terminal_map: TerminalMap` field to `VeilApp`.
2. In `resumed()`, after spawning the initial PTY, create a `Terminal` with `TerminalConfig::default()` and insert it.
3. In `execute_effect(SpawnPty)`, after `mgr.spawn()`, create a `Terminal` and insert it.
4. In `execute_effect(ClosePty)`, after `mgr.close()`, remove the terminal from the map.
5. Add a `drain_state_updates()` method (or inline it in `RedrawRequested`) that calls `channels.state_rx.try_recv()` in a loop. For each message:
   - `StateUpdate::PtyOutput { surface_id, data }` -> `self.terminal_map.write_vt(surface_id, &data)`
   - `StateUpdate::SurfaceExited { surface_id, .. }` -> `self.terminal_map.remove(surface_id)`, optionally mark pane as exited in `AppState`
   - Other variants are handled as appropriate (or logged/deferred).
6. The `GhosttyTerminalWriter` struct wraps `veil_ghostty::Terminal` and implements `TerminalWriter`.

**Important:** `GhosttyTerminalWriter::new()` calls `Terminal::new()` which requires libghosty. This is fine in the binary crate but means this specific code path is only compiled/tested when libghosty is available (`#[cfg(not(no_libghosty))]`).

**Test strategy:**

This unit is integration-level wiring. Testing is split:

- The `TerminalMap` itself is tested in Unit 2 with mocks (no FFI needed).
- The `GhosttyTerminalWriter` is a thin wrapper -- tested only with `#[cfg(not(no_libghosty))]` tests verifying that `write_vt()` delegates to `Terminal::write_vt()` and `resize()` delegates to `Terminal::resize()`.
- The state drain loop logic can be tested by extracting it into a free function: `fn process_state_update(update: StateUpdate, terminal_map: &mut TerminalMap, ...) -> Vec<ActionEffect>`. This function takes a mock `TerminalMap` and verifies `PtyOutput` calls `write_vt`, `SurfaceExited` calls `remove`, etc.

**Tests (using mock `TerminalWriter`):**

- **PtyOutput routes to write_vt:** Create a `TerminalMap` with a mock, call the processing function with `StateUpdate::PtyOutput`, verify the mock's `write_vt` was called with the correct data.
- **SurfaceExited removes terminal:** Insert a mock terminal, process `StateUpdate::SurfaceExited`, verify the terminal is removed from the map.
- **PtyOutput for unknown surface is logged, not panic:** Process `PtyOutput` with a `SurfaceId` not in the map, verify no panic (returns false).
- **Multiple PtyOutput messages processed in order:** Process three `PtyOutput` messages, verify the mock received all three write_vt calls in order.

### Unit 4: Resize propagation

**Location:** `crates/veil/src/main.rs` (modify resize handling), `crates/veil/src/terminal_map.rs` (already has `resize`)

**What it does:**

When pane layout changes (window resize, split, zoom), compute new cell dimensions for each pane and propagate them to both the `TerminalMap` (terminal state) and the `PtyManager` (sends SIGWINCH to child). This ensures the terminal's internal col/row state and the PTY's window size stay in sync.

**Changes:**

1. Add a `propagate_resizes()` method to `VeilApp` (or a free function for testability) that:
   - Computes the current `PaneLayout` for the active workspace (using `compute_layout`).
   - For each pane, calculates `(cols, rows)` from the pane rect and cell size.
   - Calls `terminal_map.resize(surface_id, cols, rows, cell_w, cell_h)` for each surface.
   - Calls `pty_manager.resize(surface_id, PtySize { cols, rows, .. })` for each surface.
2. Call `propagate_resizes()` after:
   - `WindowEvent::Resized` (window resize).
   - `ActionEffect::Redraw` when caused by split, close, zoom, or sidebar toggle (layout-changing actions).

**Cell size calculation:**

Given a pane rect (width_px, height_px) and a cell size (e.g., 8x16 pixels -- hardcoded initially, later from font metrics):

```rust
let cols = (rect.width / cell_width_px as f32).floor() as u16;
let rows = (rect.height / cell_height_px as f32).floor() as u16;
```

Clamp cols and rows to at least 1 to avoid zero-dimension errors.

**Test strategy (free function, mock TerminalWriter, no FFI):**

- **Single pane resize:** Given a window of 800x600 with sidebar visible (250px), compute expected cols/rows for the single pane, verify the mock terminal and mock PTY manager received those dimensions.
- **Two-pane resize:** After a horizontal split, verify each pane gets approximately half the width, with correct cols/rows for each.
- **Zoomed pane resize:** When a pane is zoomed, verify only the zoomed pane gets the full terminal area dimensions.
- **Minimum dimensions:** With a very small window, verify cols and rows are clamped to >= 1.
- **Sidebar toggle resize:** After toggling sidebar off, verify panes get wider (more cols).

### Unit 5: Surface exit handling in AppState

**Location:** `crates/veil/src/main.rs` (modify state update processing)

**What it does:**

When `StateUpdate::SurfaceExited` arrives, the current code does nothing with `AppState`. This unit adds handling: mark the pane as exited or close it from the workspace, clean up focus if needed.

**Design:**

For MVP, when a surface exits:
1. Remove the terminal from `TerminalMap` (done in Unit 3).
2. Find the workspace and pane for the exited `SurfaceId`.
3. If the workspace has more than one pane, close the exited pane (remove from layout tree, shift focus to a sibling).
4. If the workspace has only one pane, leave it showing (future: show "[exited]" indicator).
5. Emit `ClosePty` effect if PTY cleanup is needed (though the PTY may already be closed by this point).

**Test strategy (no FFI):**

- **Exit with multiple panes:** Process `SurfaceExited` for a surface in a two-pane workspace, verify the pane is removed from the layout tree and focus moves to the remaining pane.
- **Exit with single pane:** Process `SurfaceExited` for the only surface in a workspace, verify the workspace remains (not removed) and the pane is retained in the tree.
- **Exit for unknown surface:** Process `SurfaceExited` for a `SurfaceId` not in any workspace, verify no panic (log and return).
- **Focus shift after exit:** If the exited surface was focused, verify focus moves to a remaining surface in the same workspace.

## Acceptance Criteria

1. **PTY output reaches terminal state:** After spawning a shell, `PtyEvent::Output` bytes are forwarded as `StateUpdate::PtyOutput` and fed into `Terminal::write_vt()` via the `TerminalMap`. The terminal's cursor position and content update (verifiable via `Terminal::cursor_x()` / `cursor_y()` in integration tests with libghosty).

2. **Terminal state updates as PTY produces output:** Multiple chunks of PTY output are processed sequentially, accumulating terminal state changes. The `TerminalMap.write_vt()` method is called for each chunk.

3. **Resize propagates to both PTY and Terminal:** When the window is resized or a pane split/close/zoom occurs, both `terminal_map.resize()` and `pty_manager.resize()` are called for every affected surface with consistent dimensions.

4. **PTY exit is handled gracefully:** When a child process exits, the terminal is removed from the `TerminalMap`. In multi-pane workspaces, the exited pane is closed and focus shifts. In single-pane workspaces, the workspace remains. No panics on exit events for unknown surfaces.

5. **No FFI in unit tests:** All core logic (output routing, terminal map management, resize propagation, exit handling) is testable via the `TerminalWriter` trait and mock implementations. Only thin `GhosttyTerminalWriter` glue requires `#[cfg(not(no_libghosty))]`.

## Dependencies

- **veil-ghostty** crate: Already exists with `Terminal` and `RenderState` types. No changes needed to this crate.
- **veil-pty** crate: Modification to `bridge_loop()` in `manager.rs` (Unit 1). No new dependencies.
- **veil-core** crate: Add `StateUpdate::PtyOutput` variant to `message.rs` (Unit 1). No new dependencies.
- **veil binary crate**: New `terminal_map.rs` module (Unit 2). Modifications to `main.rs` (Units 3, 4, 5). No new external dependencies.
- **tokio**: Already a dependency. Used for `mpsc` channels and `try_recv()` in the drain loop.
- **tracing**: Already a dependency. Used for logging output routing events.
