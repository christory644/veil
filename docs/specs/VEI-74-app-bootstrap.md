# VEI-74: App Bootstrap — Create Default Workspace, Spawn Shell PTY, Set Focus

## Context

The app opens a grey window because `main.rs` creates `AppState::new()` but never calls `create_workspace()`, never spawns a PTY, and never sets focus. The frame builder (`crates/veil/src/frame.rs`) checks `app_state.active_workspace()` and returns empty geometry when it is `None` -- which it always is at startup.

This task wires the existing components together in the `resumed` handler so the app starts with a usable terminal area. Specifically:

1. Create a default workspace with the user's current working directory
2. Set focus to the root pane's surface
3. Spawn a PTY for that surface using the user's default shell
4. Store the PTY handle so it can be shut down on exit

After this, `build_frame_geometry` will find an active workspace with a focused surface and produce: dark cell background quads, a cursor block in the top-left, and a blue focus border. This does NOT wire PTY I/O to the renderer or text rendering -- those come in later issues.

### What exists

- **`AppState::create_workspace(name, cwd)`** -- creates a workspace with a single `PaneNode::Leaf` containing a `PaneId` and `SurfaceId`. Auto-activates the first workspace. Returns `WorkspaceId`.
- **`FocusManager::focus_surface(surface_id)`** -- sets keyboard focus to a surface, which the frame builder uses to draw the cursor and focus border.
- **`veil_pty::create_pty(PtyConfig)`** -- allocates a PTY pair, spawns the child process, starts background read/write threads. Returns `Box<dyn Pty>`.
- **`veil_pty::PtyManager`** -- manages PTY lifecycle by `SurfaceId`, dispatches `AppCommand` messages, bridges PTY events to `StateUpdate` channel. Already has `spawn(surface_id, PtyConfig)`, `shutdown_all()`, and `close(surface_id)`.
- **`build_frame_geometry`** -- already handles the rest: computes layout rects, builds cell background quads, cursor, dividers, focus border. Just needs a non-`None` active workspace and a focused surface.
- **`Channels`** -- provides `state_tx` for PtyManager construction.
- **`ShutdownSignal`** -- provides `handle()` for PtyManager construction.

### What's missing

The `VeilApp::resumed` method creates the window and renderer but does nothing else. The glue code to:
1. Create a workspace in `app_state`
2. Extract the root pane's `SurfaceId`
3. Call `focus.focus_surface(surface_id)`
4. Create a `PtyManager` and call `spawn(surface_id, config)`
5. Shut down the PTY manager on `CloseRequested`

## Implementation Units

### Unit 1: Bootstrap function — `init_default_workspace`

**Location:** `crates/veil/src/main.rs` (new helper function or inline in `resumed`)

**What it does:**

A pure-logic helper function that creates the default workspace, extracts the root surface, and sets focus. Keeping this as a function makes it testable without a window.

```rust
/// Bootstrap a default workspace and set focus to its root pane.
/// Returns the SurfaceId of the root pane for PTY spawning.
fn init_default_workspace(app_state: &mut AppState, focus: &mut FocusManager) -> SurfaceId {
    let cwd = std::env::current_dir().unwrap_or_else(|_| {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"))
    });
    let ws_id = app_state.create_workspace("default".to_string(), cwd);
    let ws = app_state.workspace(ws_id).expect("just created");
    let surface_id = ws.layout.surface_ids()[0];
    focus.focus_surface(surface_id);
    surface_id
}
```

Key decisions:
- **Working directory:** Use `std::env::current_dir()` as the primary source, falling back to `$HOME`, then `/`. This matches how terminal emulators typically behave -- they open in the directory you launched them from.
- **Workspace name:** `"default"` -- simple, descriptive. Users will rename via the sidebar later.
- **No error propagation needed:** `create_workspace` is infallible (it generates IDs internally). The workspace always starts with exactly one leaf, so `surface_ids()[0]` is safe.

Note: `dirs` is NOT currently a dependency. The fallback chain should use `std::env::var("HOME")` instead, or simply fall back to `PathBuf::from("/")` if `current_dir()` fails. This avoids adding a new dependency for a single call.

Revised approach without `dirs`:
```rust
let cwd = std::env::current_dir().unwrap_or_else(|_| {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/"))
});
```

**Tests:**

1. **Happy path:** Call `init_default_workspace` on fresh `AppState` + `FocusManager`. Assert:
   - `app_state.active_workspace()` is `Some`
   - `app_state.active_workspace().name` is `"default"`
   - `focus.focused_surface()` returns `Some(surface_id)` matching the workspace's root surface
   - Returned `SurfaceId` matches what's in the workspace layout

2. **Working directory fallback:** Mock `current_dir` failure is hard in unit tests (it's a syscall). Instead, test the logic by extracting the cwd resolution into its own function `resolve_startup_cwd() -> PathBuf` and testing that independently with env var manipulation.

3. **Idempotency guard:** Verify that calling `init_default_workspace` when a workspace already exists does not crash (it creates a second workspace). This isn't the intended usage, but shouldn't panic. The `resumed` handler already guards against double-init via `if self.window.is_some() { return; }`.

### Unit 2: PTY manager integration in `VeilApp`

**Location:** `crates/veil/src/main.rs`

**What it does:**

Add a `PtyManager` field to `VeilApp`, initialize it in `resumed` after workspace creation, and shut it down on close.

**Changes to `VeilApp` struct:**

```rust
struct VeilApp {
    // ... existing fields ...
    /// PTY manager -- owns all active PTY instances.
    pty_manager: Option<PtyManager>,
}
```

Using `Option<PtyManager>` because `PtyManager::new` requires a `state_tx` from `Channels`, which is already owned by `VeilApp`. The manager is created in `resumed` alongside the window.

**Changes to `VeilApp::new`:**

```rust
fn new() -> Self {
    Self {
        // ... existing fields ...
        pty_manager: None,
    }
}
```

**Changes to `resumed`:**

After window and renderer creation:

```rust
fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    if self.window.is_some() {
        return;
    }
    // ... existing window + renderer creation ...

    // Bootstrap default workspace and focus
    let surface_id = init_default_workspace(&mut self.app_state, &mut self.focus);

    // Create PTY manager and spawn shell for the root pane
    let mut pty_manager = PtyManager::new(
        self._channels.state_tx.clone(),
        self.shutdown.handle(),
    );
    let cwd = self.app_state.active_workspace()
        .expect("just created")
        .working_directory
        .clone();
    let config = veil_pty::PtyConfig {
        command: None, // uses $SHELL or /bin/sh
        args: vec![],
        working_directory: Some(cwd),
        env: vec![],
        size: veil_pty::PtySize::default(),
    };
    if let Err(e) = pty_manager.spawn(surface_id, config) {
        tracing::error!("failed to spawn initial PTY: {e}");
    }
    self.pty_manager = Some(pty_manager);
}
```

Key decisions:
- **`PtyConfig::command: None`** -- this causes the PTY to use `$SHELL` (falling back to `/bin/sh`), matching the existing `resolve_command` logic in `posix.rs`.
- **`PtySize::default()`** -- 80x24, matching `DEFAULT_COLS`/`DEFAULT_ROWS` in `frame.rs`. A later issue will wire resize events to update both the PTY and the grid dimensions.
- **Error handling:** Log and continue. A failed PTY spawn should not crash the app -- the user sees the terminal area with cursor but no shell. This is a degraded but recoverable state.

**Changes to `CloseRequested`:**

```rust
WindowEvent::CloseRequested => {
    if let Some(ref mut mgr) = self.pty_manager {
        mgr.shutdown_all();
    }
    self.shutdown.trigger();
    event_loop.exit();
}
```

This ensures all child processes are reaped before the app exits. `shutdown_all` sends SIGHUP, closes master fds, and joins background threads.

**Tests:**

Testing `VeilApp` directly is difficult because it requires a winit event loop and GPU. The testable surface is:

1. **`init_default_workspace` tests** (Unit 1) -- cover the state setup logic.
2. **`PtyManager::spawn` tests** -- already exist in `crates/veil-pty/src/manager.rs`, covering spawn, duplicate detection, and shutdown.
3. **Integration test (manual):** `cargo run` shows dark terminal area, cursor, focus border. No automated test for this because it requires a display.

### Unit 3: Resolve startup working directory

**Location:** `crates/veil/src/main.rs` (new helper function)

**What it does:**

Extracts the cwd resolution logic into a testable function.

```rust
/// Resolve the working directory for the initial workspace.
///
/// Prefers the process's current directory. Falls back to $HOME, then `/`.
fn resolve_startup_cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/"))
    })
}
```

**Tests:**

1. **Happy path:** In a normal test environment, `current_dir()` succeeds and returns a real path. Assert the returned path exists and is a directory.
2. **Non-empty:** Assert the returned path is not empty (edge case: some environments have weird cwd states).

Note: Testing the `$HOME` fallback in a unit test is fragile because it requires making `current_dir()` fail, which is process-global state. The function is simple enough that code review + the happy path test suffice. The fallback logic mirrors `veil-pty`'s `resolve_command` pattern.

## File Changes Summary

| File | Change |
|------|--------|
| `crates/veil/src/main.rs` | Add `pty_manager: Option<PtyManager>` field to `VeilApp`. Add `resolve_startup_cwd()` and `init_default_workspace()` functions. Call them in `resumed`. Shut down PTY manager in `CloseRequested`. Add `use` for `veil_pty::{PtyManager, PtyConfig, PtySize}` and `std::path::PathBuf`. |

This is a single-file change. No new modules, no new crates, no new dependencies.

## Acceptance Criteria

1. **`cargo run` shows dark terminal area** -- cell background quads filling the terminal area (dark gray, not the lighter gray of the empty window background).
2. **Cursor block visible** -- white/light cursor block in the top-left corner of the terminal area.
3. **Focus border visible** -- blue semi-transparent border around the single pane.
4. **No crashes on resize** -- resizing the window redraws correctly with the new dimensions.
5. **No crashes on close** -- closing the window shuts down PTYs cleanly, no zombie processes.
6. **`cargo clippy --all-targets --all-features -- -D warnings`** passes.
7. **`cargo test`** passes (existing tests plus new unit tests for `init_default_workspace` and `resolve_startup_cwd`).
8. **Shell process is spawned** -- `ps` shows a child shell process while the app is running. Closing the app reaps it.

## Dependencies

- **No new crate dependencies.** `veil` already depends on `veil-pty` and `veil-core`.
- **No new workspace dependencies.**
- **Existing APIs used as-is:** `AppState::create_workspace`, `FocusManager::focus_surface`, `PtyManager::new`/`spawn`/`shutdown_all`, `PtyConfig`, `PtySize`.

## Risks and Edge Cases

1. **`current_dir()` fails** -- happens if the launching directory was deleted. Fallback to `$HOME` then `/` handles this.
2. **`$SHELL` not set** -- `PosixPty::resolve_command` falls back to `/bin/sh`. No action needed.
3. **PTY spawn fails** -- logged as error, app continues with visual terminal area but no working shell. User sees cursor but no prompt. Acceptable degraded state for this bootstrapping issue.
4. **macOS app sandbox** -- if run from a sandboxed context, PTY spawn may fail due to sandbox restrictions. Out of scope for this issue.
5. **Double `resumed` calls** -- the existing guard `if self.window.is_some() { return; }` prevents double initialization. The bootstrap code runs after window creation, inside the same guard.
