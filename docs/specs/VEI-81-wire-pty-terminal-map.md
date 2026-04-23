# VEI-81: Wire PTY Output to Terminal State via TerminalMap

## Context

PTY processes spawn and produce output, but the main event loop ignores `StateUpdate::PtyOutput` with a `_ => {}` catch-all in `drain_state_updates()` (main.rs lines 310-313). Terminal state never gets populated.

The `TerminalMap` in `crates/veil/src/terminal_map.rs` is already fully implemented and tested -- it provides a `HashMap<SurfaceId, Box<dyn TerminalWriter>>` with `write_vt()`, `remove()`, `resize()`, and `render_cells()` methods. The `process_state_update()` free function handles routing `PtyOutput` and `SurfaceExited` variants to the map. The `handle_surface_exit()` function manages workspace cleanup and focus shifting. None of this code is wired into `VeilApp`.

This task connects the existing `TerminalMap` infrastructure to the event loop so PTY output bytes flow into terminal state. After this task, `terminal_map.render_cells(surface_id)` returns populated `CellGrid` data. Text still will not render visually -- that is VEI-77's responsibility.

### What exists

- **`TerminalMap`** (`crates/veil/src/terminal_map.rs`) -- `new()`, `insert()`, `remove()`, `write_vt()`, `resize()`, `get()`, `get_mut()`, `len()`, `is_empty()`, `iter_mut()`. Fully implemented and tested with mock `TerminalWriter` instances.
- **`TerminalWriter` trait** -- `write_vt(&mut self, data: &[u8])`, `resize(cols, rows, cell_w, cell_h) -> Result<(), String>`, `cols()`, `rows()`, `render_cells() -> Option<CellGrid>`.
- **`process_state_update()`** -- Routes `StateUpdate::PtyOutput` to `write_vt()` and `StateUpdate::SurfaceExited` to `remove()`. Returns `true` if the update was terminal-related. Fully tested.
- **`handle_surface_exit()`** -- Closes exited pane from workspace layout, shifts focus if needed, returns `SurfaceExitOutcome`. Fully tested.
- **`compute_pane_cells()`** -- Computes `(cols, rows)` from pane pixel dimensions and cell sizes. Fully tested.
- **`VeilApp`** (`crates/veil/src/main.rs`) -- Owns `app_state`, `channels`, `focus`, `pty_manager`, `renderer`, `window`. The `terminal_map` module is declared with `#[allow(dead_code)]` (line 25). No `terminal_map` field exists on `VeilApp`.
- **`drain_state_updates()`** -- Handles `ConfigReloaded` and `ConversationsUpdated`. All other variants fall through to `_ => {}` with a comment.
- **`execute_effect(SpawnPty)`** -- Calls `pty_manager.spawn()` but does not create a terminal entry.
- **`execute_effect(ClosePty)`** -- Calls `pty_manager.close()` but does not remove a terminal entry.
- **`resumed()`** -- Bootstraps workspace, spawns initial PTY, starts config watcher, aggregator, and socket actors. Does not create a terminal for the root pane.
- **`veil_ghostty::Terminal`** -- `new(TerminalConfig)`, `write_vt()`, `resize()`. Requires libghosty FFI. `TerminalConfig::default()` is 80x24 with 10,000 scrollback.
- **`veil_ghostty::RenderState`** -- `new()`, `update(&mut terminal)`, `dirty()`, `cursor()`, `colors()`. Also requires FFI.
- **`StateUpdate::PtyOutput`** -- Already defined in `veil_core::message`. Has `surface_id: SurfaceId` and `data: Vec<u8>`.
- **`StateUpdate::SurfaceExited`** -- Already defined. Has `surface_id: SurfaceId` and `exit_code: Option<i32>`.
- **`DEFAULT_COLS = 80`, `DEFAULT_ROWS = 24`** -- Constants in `frame.rs`. `PtySize::default()` and `TerminalConfig::default()` both use 80x24.

### What's missing

1. **`terminal_map` field on `VeilApp`** -- The map exists as a module but is never instantiated.
2. **Terminal creation on PTY spawn** -- When a PTY is spawned (root pane in `resumed()`, new panes via `SpawnPty`), no `TerminalWriter` is inserted into the map.
3. **`GhosttyTerminalWriter`** -- No struct wraps `veil_ghostty::Terminal` to implement the `TerminalWriter` trait. This is needed for the real (non-mock) code path.
4. **PtyOutput handling** -- The `_ => {}` catch-all in `drain_state_updates()` silently drops `PtyOutput`, `SurfaceExited`, `ActorError`, `ErrorOccurred`, `ErrorDismissed`, `NotificationReceived`, and `UpdateAvailable`.
5. **SurfaceExited cleanup** -- When a surface exits, the terminal is not removed from the map and the pane is not closed from the layout.
6. **Terminal removal on ClosePty** -- `execute_effect(ClosePty)` does not remove the terminal from the map.
7. **`#[allow(dead_code)]` on `mod terminal_map`** -- The dead code annotation suppresses the warning because nothing uses the module.

## Implementation Units

### Unit 1: GhosttyTerminalWriter

**Location:** `crates/veil/src/terminal_map.rs`

**What it does:**

Implements a `GhosttyTerminalWriter` struct that wraps `veil_ghostty::Terminal` (and optionally `veil_ghostty::RenderState`) and implements the `TerminalWriter` trait. This is the real implementation used at runtime. It is gated behind `#[cfg(not(no_libghosty))]` because it depends on libghosty FFI.

**Changes:**

1. Add to `terminal_map.rs`, gated behind `#[cfg(not(no_libghosty))]`:

```rust
/// Factory function to create a real terminal writer backed by libghosty.
/// Returns `None` if terminal creation fails.
pub fn create_ghostty_terminal(cols: u16, rows: u16) -> Option<Box<dyn TerminalWriter>> {
    let config = veil_ghostty::TerminalConfig { cols, rows, max_scrollback: 10_000 };
    match veil_ghostty::Terminal::new(config) {
        Ok(terminal) => {
            let render_state = veil_ghostty::RenderState::new().ok();
            Some(Box::new(GhosttyTerminalWriter { terminal, render_state }))
        }
        Err(e) => {
            tracing::error!("failed to create ghostty terminal: {e}");
            None
        }
    }
}
```

2. The `GhosttyTerminalWriter` struct:

```rust
#[cfg(not(no_libghosty))]
struct GhosttyTerminalWriter {
    terminal: veil_ghostty::Terminal,
    render_state: Option<veil_ghostty::RenderState>,
}
```

3. Implement `TerminalWriter` for `GhosttyTerminalWriter`:
   - `write_vt()` -> delegates to `self.terminal.write_vt(data)`
   - `resize()` -> delegates to `self.terminal.resize()`, mapping `GhosttyError` to `String`
   - `cols()` -> `self.terminal.cols().unwrap_or(80)`
   - `rows()` -> `self.terminal.rows().unwrap_or(24)`
   - `render_cells()` -> updates `self.render_state` from `self.terminal`, then iterates rows/cells to build a `CellGrid`. For MVP (before VEI-77 adds row/cell iteration FFI), this returns `None` since the cell iteration API is not yet wrapped.

4. Add a `#[cfg(no_libghosty)]` stub factory that always returns `None` so the binary compiles without libghosty:

```rust
#[cfg(no_libghosty)]
pub fn create_ghostty_terminal(_cols: u16, _rows: u16) -> Option<Box<dyn TerminalWriter>> {
    tracing::warn!("libghosty not available, terminal emulation disabled");
    None
}
```

**Test strategy:**

- **`#[cfg(not(no_libghosty))]` tests:**
  - Create a `GhosttyTerminalWriter` with default config, verify `cols()` and `rows()` return 80 and 24.
  - Call `write_vt(b"Hello")`, verify no panic and cursor advances (verify via `cols()`/`rows()` still valid).
  - Call `resize(120, 40, 8, 16)`, verify `cols()` returns 120 and `rows()` returns 40.
  - Call `render_cells()`, verify it returns `None` (MVP behavior before row/cell FFI is wired).
- **`create_ghostty_terminal()` happy path:** Call with `(80, 24)`, verify returns `Some`.
- **`create_ghostty_terminal()` with zero dims:** Call with `(0, 0)`, verify returns `None` (Terminal::new rejects zero).
- **`#[cfg(no_libghosty)]` stub:** `create_ghostty_terminal()` returns `None`.

### Unit 2: Wire TerminalMap into VeilApp

**Location:** `crates/veil/src/main.rs`

**What it does:**

Adds a `terminal_map: TerminalMap` field to `VeilApp`, removes the `#[allow(dead_code)]` annotation on `mod terminal_map`, and wires terminal creation into the PTY spawn points. Also wires `drain_state_updates()` to handle `PtyOutput` and `SurfaceExited` using the existing `process_state_update()` and `handle_surface_exit()` functions.

**Changes:**

1. Remove `#[allow(dead_code)]` from `mod terminal_map` (line 25).

2. Add `terminal_map: terminal_map::TerminalMap` field to `VeilApp`.

3. Initialize `terminal_map: terminal_map::TerminalMap::new()` in `VeilApp::new()`.

4. In `resumed()`, after `pty_manager.spawn(surface_id, ...)`, create a terminal:

```rust
if let Some(terminal) = terminal_map::create_ghostty_terminal(DEFAULT_COLS, DEFAULT_ROWS) {
    self.terminal_map.insert(surface_id, terminal);
} else {
    tracing::warn!(?surface_id, "no terminal created (libghosty unavailable)");
}
```

Use `DEFAULT_COLS`/`DEFAULT_ROWS` (80/24) from `frame.rs`, or define local constants. These match `PtySize::default()` and `TerminalConfig::default()`.

5. In `execute_effect(SpawnPty)`, after successful `mgr.spawn()`, create and insert a terminal:

```rust
ActionEffect::SpawnPty { surface_id, working_directory } => {
    if let Some(ref mut mgr) = self.pty_manager {
        if let Err(e) = mgr.spawn(surface_id, default_pty_config(working_directory)) {
            tracing::error!(?surface_id, "failed to spawn PTY: {e}");
        } else if let Some(terminal) = terminal_map::create_ghostty_terminal(
            DEFAULT_COLS, DEFAULT_ROWS,
        ) {
            self.terminal_map.insert(surface_id, terminal);
        }
    }
}
```

6. In `execute_effect(ClosePty)`, after `mgr.close()`, remove the terminal:

```rust
ActionEffect::ClosePty { surface_id } => {
    if let Some(ref mut mgr) = self.pty_manager {
        if let Err(e) = mgr.close(surface_id) {
            tracing::warn!(?surface_id, "failed to close PTY: {e}");
        }
    }
    self.terminal_map.remove(surface_id);
}
```

7. Replace the `_ => {}` catch-all in `drain_state_updates()` with explicit handling:

```rust
fn drain_state_updates(&mut self) {
    while let Ok(update) = self.channels.state_rx.try_recv() {
        match update {
            StateUpdate::ConfigReloaded { config, delta, warnings } => {
                self.handle_config_reloaded(config, &delta, &warnings);
            }
            StateUpdate::ConversationsUpdated(sessions) => {
                tracing::info!(count = sessions.len(), "conversations updated");
                self.app_state.update_conversations(sessions);
                self.request_redraw();
            }
            StateUpdate::PtyOutput { surface_id, data } => {
                if !self.terminal_map.write_vt(surface_id, &data) {
                    tracing::debug!(?surface_id, "PtyOutput for unknown surface");
                }
                self.request_redraw();
            }
            StateUpdate::SurfaceExited { surface_id, exit_code } => {
                tracing::info!(?surface_id, ?exit_code, "surface exited");
                self.terminal_map.remove(surface_id);
                let outcome = terminal_map::handle_surface_exit(
                    surface_id,
                    &mut self.app_state,
                    &mut self.focus,
                );
                tracing::debug!(?surface_id, ?outcome, "surface exit handled");
                self.request_redraw();
            }
            StateUpdate::ActorError { actor_name, message } => {
                tracing::error!(actor = %actor_name, "actor error: {message}");
            }
            StateUpdate::ErrorOccurred(report) => {
                let id = self.app_state.add_error(report);
                tracing::debug!(?id, "error tracked");
                self.request_redraw();
            }
            StateUpdate::ErrorDismissed { error_id } => {
                self.app_state.dismiss_error(error_id);
                self.request_redraw();
            }
            StateUpdate::NotificationReceived { workspace_id, message } => {
                self.app_state.add_notification(workspace_id, message);
                self.request_redraw();
            }
            StateUpdate::UpdateAvailable(notification) => {
                tracing::info!(?notification, "update available");
            }
        }
    }
}
```

This eliminates the `_ => {}` catch-all entirely. Every `StateUpdate` variant is explicitly handled.

8. Define `DEFAULT_COLS` and `DEFAULT_ROWS` constants in the `main.rs` module (or import from `frame.rs` if they are made `pub`). Since `frame.rs` already defines them as `const` (not `pub`), either make them `pub(crate)` in `frame.rs` or duplicate them in `main.rs`. Prefer making them `pub(crate)` in `frame.rs` to avoid duplication.

**Test strategy:**

This unit is pure wiring in the binary crate's event loop. The underlying logic (`process_state_update`, `handle_surface_exit`, `TerminalMap` operations) is already comprehensively tested in `terminal_map.rs`. The wiring itself is not independently unit-testable without a full winit event loop, but correctness is validated by:

- **Existing `terminal_map.rs` tests pass** -- All 30+ tests covering `TerminalMap`, `process_state_update`, `handle_surface_exit`, `compute_pane_cells`, and mock `TerminalWriter`s.
- **`cargo test --workspace` passes** -- No regressions.
- **`cargo clippy --all-targets --all-features -- -D warnings` passes** -- No dead code warnings for `terminal_map`, no unused import warnings.
- **Compile-time verification** -- Removing `#[allow(dead_code)]` means the compiler verifies the module is actually used. Any unused public items will trigger warnings (promoted to errors by `-D warnings`).

### Unit 3: Remove dead_code annotations and expose constants

**Location:** `crates/veil/src/main.rs`, `crates/veil/src/frame.rs`

**What it does:**

Clean up the `#[allow(dead_code)]` annotation on `mod terminal_map` and expose the `DEFAULT_COLS`/`DEFAULT_ROWS` constants so they can be shared between `frame.rs` and `main.rs`.

**Changes:**

1. In `main.rs`, change:
   ```rust
   #[allow(dead_code)]
   mod terminal_map;
   ```
   to:
   ```rust
   mod terminal_map;
   ```

2. In `frame.rs`, change:
   ```rust
   const DEFAULT_COLS: u16 = 80;
   const DEFAULT_ROWS: u16 = 24;
   ```
   to:
   ```rust
   pub(crate) const DEFAULT_COLS: u16 = 80;
   pub(crate) const DEFAULT_ROWS: u16 = 24;
   ```

3. In `main.rs`, import and use `crate::frame::{DEFAULT_COLS, DEFAULT_ROWS}` where needed (in `resumed()` and `execute_effect`).

**Test strategy:**

- **Compile-time**: `cargo build` succeeds without dead_code warnings.
- **Existing tests**: All frame.rs tests continue to pass since the constants are unchanged in value.

## Acceptance Criteria

1. `StateUpdate::PtyOutput` is handled in `drain_state_updates()` -- not ignored by a catch-all.
2. PTY output bytes are fed to a ghostty `Terminal` via `TerminalMap::write_vt()`.
3. `terminal_map.get_mut(surface_id).and_then(|t| t.render_cells())` returns `None` for now (real cell data requires VEI-77), but the terminal exists in the map and `write_vt()` has been called.
4. The `_ => {}` catch-all no longer silently drops any `StateUpdate` variant. Every variant is explicitly matched.
5. `StateUpdate::SurfaceExited` removes the terminal from the map and handles workspace/focus cleanup.
6. `execute_effect(SpawnPty)` creates a terminal entry alongside the PTY spawn.
7. `execute_effect(ClosePty)` removes the terminal entry alongside the PTY close.
8. `#[allow(dead_code)]` is removed from `mod terminal_map` in `main.rs`.
9. All existing tests pass (`cargo test --workspace`).
10. `cargo clippy --all-targets --all-features -- -D warnings` passes.

## Dependencies

- **veil-ghostty** crate: Already exists. `Terminal`, `TerminalConfig`, `RenderState`, `CellGrid`, `CellData` types are all available. No changes needed.
- **veil-core** crate: `StateUpdate::PtyOutput` and `StateUpdate::SurfaceExited` variants already exist in `message.rs`. No changes needed.
- **veil-pty** crate: `PtyManager` bridge loop already forwards `PtyEvent::Output` as `StateUpdate::PtyOutput`. No changes needed.
- **tracing** crate: Already a dependency. Used for logging in the new match arms.
- **No new external crate dependencies are required.**
