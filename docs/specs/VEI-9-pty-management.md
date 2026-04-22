# VEI-9: PTY Management -- Cross-Platform Pseudo-Terminal

## Context

Every terminal pane in Veil needs a pseudo-terminal (PTY) to communicate with its child process (shell, agent, or other command). The PTY is the pipe between user keystrokes and the shell's stdin/stdout. This task implements the `veil-pty` crate: a platform abstraction over PTY creation, process spawning, I/O, resize, and lifecycle management.

The crate provides **raw byte-level I/O** only. It does not parse terminal escape sequences or interact with libghosty -- that integration is a separate concern (VEI-8). From veil-pty's perspective, a PTY is a bidirectional byte stream attached to a child process.

### Architecture in context

Per the system design doc, each terminal surface owns:
- A PTY master/slave pair
- A child process (shell or agent command)
- A read thread (PTY master fd --> byte channel)
- A write path (byte channel --> PTY master fd)

The event loop sends `AppCommand::SpawnSurface`, `AppCommand::SendInput`, `AppCommand::ResizeSurface`, and `AppCommand::CloseSurface` commands. A PTY manager actor receives these commands and delegates to `veil-pty` for the actual platform operations. When a child process exits, veil-pty sends a notification back through the state update channel (`StateUpdate::SurfaceExited`).

### What already exists

- `veil-pty/Cargo.toml` -- depends on `veil-core` and `tracing`, has `#![deny(unsafe_code)]`
- `veil-pty/src/lib.rs` -- empty crate stub with `#![deny(unsafe_code)]` and `#![warn(missing_docs)]`
- `veil-core::workspace::SurfaceId` -- opaque handle used to identify surfaces
- `veil-core::message::AppCommand` -- includes `SpawnSurface`, `SendInput`, `ResizeSurface`, `CloseSurface`
- `veil-core::message::StateUpdate::SurfaceExited` -- notification that a surface's process exited
- `veil-core::lifecycle::ShutdownHandle` -- for observing application shutdown

### Unsafe code strategy

PTY operations on POSIX require direct FFI calls to libc (`posix_openpt`, `grantpt`, `unlockpt`, `ptsname_r`, `login_tty`, `ioctl`, `fork`, `execvp`, `waitpid`, etc.). These are inherently unsafe.

The crate-level `#![deny(unsafe_code)]` will remain on `lib.rs`. The platform-specific modules (`posix.rs`) will use `#![allow(unsafe_code)]` at the module level. Each `unsafe` block will have a `// SAFETY:` comment documenting the invariant that makes the call sound. The goal is to confine unsafety to the smallest possible surface area with clear documentation.

The public API of `veil-pty` is entirely safe Rust. Consumers never see raw file descriptors or pointers.

### Platform focus

macOS arm64 is the current development platform. The POSIX implementation covers both macOS and Linux. Windows ConPTY will be stubbed with a compile error directing to a follow-up issue -- the trait design accommodates it but no Windows code ships in this task.

## Implementation Units

### Unit 1: PTY trait and core types (`veil-pty::types`, `veil-pty::lib`)

The platform-agnostic trait that defines what a PTY can do, plus supporting types. This is the public API contract.

**Files:**
- `crates/veil-pty/src/lib.rs` -- crate root, re-exports
- `crates/veil-pty/src/types.rs` -- shared types (PtySize, PtyConfig, PtyEvent)
- `crates/veil-pty/src/error.rs` -- error types

**Types:**

```rust
/// Terminal dimensions in cells and pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    /// Number of columns (characters).
    pub cols: u16,
    /// Number of rows (characters).
    pub rows: u16,
    /// Width in pixels (optional, used by some applications).
    pub pixel_width: u16,
    /// Height in pixels (optional, used by some applications).
    pub pixel_height: u16,
}

/// Configuration for spawning a new PTY.
#[derive(Debug, Clone)]
pub struct PtyConfig {
    /// Command to execute (e.g., "/bin/zsh"). If None, uses $SHELL or /bin/sh.
    pub command: Option<String>,
    /// Arguments to pass to the command.
    pub args: Vec<String>,
    /// Working directory. Defaults to $HOME if None.
    pub working_directory: Option<PathBuf>,
    /// Additional environment variables to set (key, value).
    /// These are added on top of the inherited environment.
    pub env: Vec<(String, String)>,
    /// Initial terminal size.
    pub size: PtySize,
}

/// Events emitted by the PTY read loop.
#[derive(Debug)]
pub enum PtyEvent {
    /// Output bytes read from the PTY master fd.
    Output(Vec<u8>),
    /// The child process exited.
    ChildExited {
        /// Exit code if the child exited normally.
        exit_code: Option<i32>,
    },
}

/// Errors from PTY operations.
#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    /// Failed to create the PTY pair.
    #[error("failed to create PTY: {0}")]
    Create(String),
    /// Failed to spawn the child process.
    #[error("failed to spawn child process: {0}")]
    Spawn(String),
    /// I/O error on the PTY.
    #[error("PTY I/O error: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },
    /// Failed to resize the PTY.
    #[error("failed to resize PTY: {0}")]
    Resize(String),
    /// The PTY has already been closed.
    #[error("PTY is closed")]
    Closed,
    /// Platform not supported.
    #[error("PTY not supported on this platform")]
    Unsupported,
}
```

**Trait:**

```rust
/// Platform abstraction for a pseudo-terminal.
///
/// A `Pty` owns the master side of a PTY pair and the associated child process.
/// It provides channels for I/O and methods for resize and shutdown.
///
/// The read side is a background thread that sends `PtyEvent`s through a channel.
/// The write side accepts bytes through a channel.
pub trait Pty: Send {
    /// Get the receiver for PTY events (output bytes, child exit).
    ///
    /// This is a std mpsc Receiver. The background read thread sends events
    /// through it. Returns None if the receiver has already been taken.
    fn take_event_rx(&mut self) -> Option<std::sync::mpsc::Receiver<PtyEvent>>;

    /// Get the sender for writing bytes to the PTY.
    ///
    /// Clone this sender to write from multiple places if needed.
    fn writer(&self) -> std::sync::mpsc::Sender<Vec<u8>>;

    /// Resize the PTY to new dimensions.
    fn resize(&self, size: PtySize) -> Result<(), PtyError>;

    /// Get the child process ID, if available.
    fn child_pid(&self) -> Option<u32>;

    /// Request graceful shutdown.
    ///
    /// On POSIX: sends SIGHUP to the child process group, then closes the
    /// master fd. The read thread will observe the close and emit ChildExited.
    ///
    /// This is idempotent -- calling it multiple times is safe.
    fn shutdown(&mut self) -> Result<(), PtyError>;

    /// Check if the PTY has been shut down.
    fn is_closed(&self) -> bool;
}
```

**Factory function:**

```rust
/// Create a new PTY with the given configuration.
///
/// This allocates the PTY pair, spawns the child process, and starts
/// the background read and write threads. Returns a boxed trait object.
pub fn create_pty(config: PtyConfig) -> Result<Box<dyn Pty>, PtyError>;
```

The `create_pty` function dispatches to the platform implementation at compile time via `#[cfg(unix)]` / `#[cfg(windows)]`.

**Why `std::sync::mpsc` instead of `tokio::sync::mpsc`:** The PTY read/write threads are OS threads (not async tasks) because PTY I/O is blocking and we want deterministic, low-latency forwarding. Using std channels avoids requiring a tokio runtime inside veil-pty. The PTY manager actor in the binary crate can bridge std channels to tokio channels using `tokio::task::spawn_blocking` or a dedicated bridging task.

**Tests:**

- `PtySize` construction and field access
- `PtySize` default values (80x24, 0 pixel dimensions)
- `PtyConfig` with all fields populated
- `PtyConfig` with None command (default shell behavior documented)
- `PtyConfig` with empty env vec
- `PtyConfig` with multiple env entries
- `PtyError` display strings are informative
- `PtyError::Io` converts from `std::io::Error`
- `PtyEvent::Output` holds arbitrary bytes including NUL
- `PtyEvent::ChildExited` with Some and None exit codes

### Unit 2: POSIX PTY implementation (`veil-pty::posix`)

The actual PTY allocation, process spawning, and I/O threading for macOS and Linux. This module contains all the unsafe code.

**Files:**
- `crates/veil-pty/src/posix.rs` -- `#![allow(unsafe_code)]` at module level

**Internal structure:**

```rust
/// POSIX implementation of the Pty trait.
pub(crate) struct PosixPty {
    /// Master file descriptor for the PTY.
    master_fd: RawFd,
    /// Child process ID.
    child_pid: libc::pid_t,
    /// Sender for writing bytes to the PTY. Cloneable.
    write_tx: std::sync::mpsc::Sender<Vec<u8>>,
    /// Receiver for PTY events. Taken by the consumer via take_event_rx.
    event_rx: Option<std::sync::mpsc::Receiver<PtyEvent>>,
    /// Whether the PTY has been shut down.
    closed: Arc<AtomicBool>,
    /// Handle to the read thread (for join on shutdown).
    read_handle: Option<std::thread::JoinHandle<()>>,
    /// Handle to the write thread (for join on shutdown).
    write_handle: Option<std::thread::JoinHandle<()>>,
}
```

**PTY creation sequence:**

1. Call `posix_openpt(O_RDWR | O_NOCTTY)` to create the master fd
2. Call `grantpt(master_fd)` to set slave ownership/permissions
3. Call `unlockpt(master_fd)` to unlock the slave
4. Call `ptsname_r(master_fd)` (or `ptsname` on macOS with mutex guard) to get the slave device path
5. Open the slave fd
6. Set the initial terminal size via `ioctl(master_fd, TIOCSWINSZ, &winsize)`

**Process spawning sequence:**

1. Call `fork()`
2. In child process:
   a. Call `setsid()` to create a new session
   b. Call `ioctl(slave_fd, TIOCSCTTY, 0)` to set the controlling terminal
   c. Duplicate slave fd to stdin/stdout/stderr
   d. Close the master fd and original slave fd
   e. Set environment variables (inherited env + `PtyConfig.env`)
   f. Call `execvp()` with the command and args
3. In parent process:
   a. Close the slave fd (parent only needs master)
   b. Store child PID
   c. Start background read thread
   d. Start background write thread

Alternatively, we can use `forkpty()` which combines steps 1-2 into a single call. `forkpty()` is available on both macOS and Linux via libc and handles the `setsid`, `ioctl`, and fd duplication. This is the preferred approach for simplicity.

**Read thread:**

```
loop:
  read(master_fd, buffer, 4096) -> n_bytes
  if n_bytes == 0 or error:
    waitpid(child_pid) -> exit_code
    send PtyEvent::ChildExited { exit_code }
    break
  send PtyEvent::Output(buffer[..n_bytes].to_vec())
```

The read thread uses a 4096-byte buffer (matching typical pipe buffer sizes). It runs until EOF on the master fd (which happens when the child exits or the master is closed).

**Write thread:**

```
loop:
  recv from write_rx -> bytes (blocking)
  if channel closed: break
  write_all(master_fd, &bytes)
  if write error: break
```

The write thread blocks on the std channel receiver and writes to the master fd. When the channel is closed (all senders dropped or shutdown), the thread exits.

**Resize:**

```rust
fn resize(&self, size: PtySize) -> Result<(), PtyError> {
    let ws = libc::winsize {
        ws_col: size.cols,
        ws_row: size.rows,
        ws_xpixel: size.pixel_width,
        ws_ypixel: size.pixel_height,
    };
    // SAFETY: master_fd is a valid open file descriptor for a PTY,
    // and ws is a properly initialized winsize struct.
    let ret = unsafe { libc::ioctl(self.master_fd, libc::TIOCSWINSZ, &ws) };
    if ret == -1 {
        return Err(PtyError::Resize(
            std::io::Error::last_os_error().to_string()
        ));
    }
    Ok(())
}
```

**Shutdown:**

1. Set `closed` flag to true (atomic)
2. Send `SIGHUP` to the child process group: `kill(-child_pid, SIGHUP)`
3. Close the master fd: `close(master_fd)` -- this causes the read thread to see EOF
4. Drop the write channel sender (which closes the write thread's receiver)
5. Join the read and write thread handles with a timeout

**Drop impl:**

`PosixPty::drop` calls `shutdown()` if not already closed. This ensures cleanup even if the consumer forgets to call shutdown explicitly.

**Environment variables:**

The child process inherits the parent's environment, with these additions from `PtyConfig.env`. The system design doc specifies these Veil-specific variables:
- `VEIL_WORKSPACE_ID` -- set by the caller via `PtyConfig.env`
- `VEIL_SURFACE_ID` -- set by the caller via `PtyConfig.env`
- `VEIL_SOCKET` -- set by the caller via `PtyConfig.env`
- `TERM_PROGRAM=ghostty` -- set by the caller via `PtyConfig.env`
- `TERM=xterm-ghostty` -- set by the caller via `PtyConfig.env`

The PTY crate itself does not hardcode these -- it injects whatever is in `PtyConfig.env`. The PTY manager actor (in the binary crate) is responsible for populating the correct values.

**Default shell resolution:**

When `PtyConfig.command` is `None`:
1. Check `$SHELL` environment variable
2. Fall back to `/bin/sh`

**Tests:**

These tests require a real PTY and spawn actual processes. They are integration tests gated behind `#[cfg(unix)]`.

- Spawn `/bin/echo hello` via PTY, read output, verify it contains "hello"
- Spawn `/bin/cat` via PTY, write "test\n", read back "test"
- Spawn `/bin/sh -c 'exit 42'`, verify `ChildExited { exit_code: Some(42) }` event
- Spawn a shell, resize to 132x43, verify no error
- Spawn a shell, shutdown, verify `is_closed()` returns true
- Spawn a shell, shutdown, verify read thread exits and sends `ChildExited`
- Double shutdown is idempotent (no error on second call)
- Default shell: spawn with `command: None`, verify a shell starts (we get a prompt or at least no spawn error)
- Environment injection: spawn `/usr/bin/env`, verify custom env vars appear in output
- Large output: spawn `yes | head -10000`, verify all output arrives via events
- Rapid writes: send many small writes in quick succession, verify they arrive at the child
- Drop without explicit shutdown: verify cleanup happens (no leaked fds, child reaped)

### Unit 3: PTY manager actor (`veil-pty::manager`)

The actor that owns all active PTY instances and bridges between `AppCommand` messages and the `Pty` trait. It listens for commands from the event loop and forwards PTY events back as `StateUpdate` messages.

**Files:**
- `crates/veil-pty/src/manager.rs`

**Structure:**

```rust
/// Manages the lifecycle of all active PTY instances.
///
/// Runs as a background actor. Receives AppCommand messages from the event
/// loop and translates them into Pty trait calls. Forwards PTY events
/// (output, child exit) back to the event loop via StateUpdate channel.
pub struct PtyManager {
    /// Active PTY instances, keyed by SurfaceId.
    ptys: HashMap<SurfaceId, ManagedPty>,
    /// For sending state updates back to the event loop.
    state_tx: tokio::sync::mpsc::Sender<StateUpdate>,
    /// For observing application shutdown.
    shutdown: ShutdownHandle,
}

/// A PTY instance with its associated metadata.
struct ManagedPty {
    pty: Box<dyn Pty>,
    surface_id: SurfaceId,
    /// Thread handle for the event bridge task.
    bridge_handle: Option<std::thread::JoinHandle<()>>,
}
```

**Key behaviors:**

- `PtyManager::new(state_tx, shutdown) -> Self`
- `spawn(&mut self, surface_id, config) -> Result<(), PtyError>` -- creates a PTY, starts event bridging
- `write(&self, surface_id, data) -> Result<(), PtyError>` -- sends bytes to a PTY's write channel
- `resize(&self, surface_id, size) -> Result<(), PtyError>` -- resizes a PTY
- `close(&mut self, surface_id) -> Result<(), PtyError>` -- shuts down and removes a PTY
- `shutdown_all(&mut self)` -- shuts down all PTYs (called on app exit)
- `handle_command(&mut self, cmd: AppCommand)` -- dispatches an AppCommand to the appropriate method

**Event bridge:**

When a PTY is spawned, the manager takes the `event_rx` from the PTY and spawns a std thread that:
1. Reads `PtyEvent`s from the std channel
2. For `PtyEvent::Output`, forwards the bytes (the PTY manager actor does not process output -- it sends it to whoever needs it, in practice the libghosty integration layer)
3. For `PtyEvent::ChildExited`, sends `StateUpdate::SurfaceExited` via the tokio sender

The bridge between std::sync::mpsc and tokio::sync::mpsc is handled by calling `state_tx.blocking_send()` from the std thread.

**Default environment variables:**

The `PtyManager::spawn` method builds the `PtyConfig.env` list with:
- Any caller-provided env vars
- `TERM=xterm-ghostty`
- `TERM_PROGRAM=ghostty`

The `VEIL_WORKSPACE_ID`, `VEIL_SURFACE_ID`, and `VEIL_SOCKET` vars are populated by the caller (the binary crate's command handling logic) since the PTY manager doesn't know workspace topology or socket paths.

**Tests:**

The PtyManager can be tested with a mock Pty implementation since it depends on the `Pty` trait.

- `new()` creates a manager with no active PTYs
- `spawn` adds a PTY to the active set
- `spawn` with duplicate surface_id returns error (or replaces -- define the policy)
- `write` to an existing surface succeeds
- `write` to a nonexistent surface returns `PtyError::Closed`
- `resize` to an existing surface succeeds
- `resize` to a nonexistent surface returns error
- `close` removes the PTY and calls shutdown
- `close` a nonexistent surface returns error
- `shutdown_all` closes all active PTYs
- `shutdown_all` on empty manager is a no-op
- `handle_command` dispatches `SpawnSurface` to `spawn`
- `handle_command` dispatches `SendInput` to `write`
- `handle_command` dispatches `ResizeSurface` to `resize`
- `handle_command` dispatches `CloseSurface` to `close`
- `handle_command` ignores irrelevant commands (`RefreshConversations`, etc.)
- Event bridge: when child exits, `StateUpdate::SurfaceExited` is sent via state_tx

For the mock Pty, define a `MockPty` in the test module that implements the `Pty` trait with configurable behavior (channels that the test controls).

### Unit 4: Windows stub (`veil-pty::windows`)

A compile-time stub for Windows that returns `PtyError::Unsupported`. This ensures the crate compiles on Windows without panicking at runtime.

**Files:**
- `crates/veil-pty/src/windows.rs`

**Implementation:**

```rust
/// Windows ConPTY implementation -- not yet implemented.
///
/// TODO(VEI-XX): Implement Windows ConPTY support.
/// References: wezterm's portable-pty, alacritty's tty::windows module.
pub(crate) struct WindowsPty;

impl Pty for WindowsPty {
    fn take_event_rx(&mut self) -> Option<std::sync::mpsc::Receiver<PtyEvent>> { None }
    fn writer(&self) -> std::sync::mpsc::Sender<Vec<u8>> {
        // Create a disconnected channel -- writes will fail
        let (tx, _rx) = std::sync::mpsc::channel();
        tx
    }
    fn resize(&self, _size: PtySize) -> Result<(), PtyError> { Err(PtyError::Unsupported) }
    fn child_pid(&self) -> Option<u32> { None }
    fn shutdown(&mut self) -> Result<(), PtyError> { Ok(()) }
    fn is_closed(&self) -> bool { true }
}
```

The `create_pty` function on `#[cfg(windows)]` returns `Err(PtyError::Unsupported)`.

**Tests:**

- On non-Windows: no tests (this module is cfg-gated)
- The trait implementation exists and compiles -- verified by `cargo build --target x86_64-pc-windows-msvc` in CI (if available), otherwise just by code review

## Test Strategy Summary

| Unit | Test Type | Requires PTY | Key Coverage |
|------|----------|-------------|--------------|
| 1. Trait + types | `#[test]` | No | Type construction, error Display, config defaults |
| 2. POSIX impl | `#[test]` + `#[cfg(unix)]` | Yes | Spawn, I/O, resize, shutdown, env vars, edge cases |
| 3. PTY manager | `#[test]` + `#[tokio::test]` | No (mock) | Command dispatch, lifecycle, event bridging |
| 4. Windows stub | Compile check | No | Compiles, returns Unsupported |

All unit tests for Units 1 and 3 run without a real PTY (using mocks or pure type tests). Unit 2 tests are integration tests that spawn real processes -- these run on macOS and Linux CI but are skipped on Windows.

## Acceptance Criteria

1. `cargo build -p veil-pty` succeeds on macOS
2. `cargo test -p veil-pty` passes all tests on macOS
3. `cargo clippy --all-targets --all-features -- -D warnings` passes
4. `cargo fmt --check` passes
5. A PTY can be created, a shell spawned, input written, output read, and the child process reaped -- all via the public API
6. `PtyConfig.env` injects environment variables into the child process
7. `resize()` changes the terminal dimensions without error
8. `shutdown()` sends SIGHUP, closes the master fd, and the read thread exits cleanly
9. Dropping a `PosixPty` cleans up (no leaked file descriptors, child process reaped)
10. Double shutdown is idempotent
11. The `Pty` trait is object-safe and works as `Box<dyn Pty>`
12. The `PtyManager` correctly dispatches `AppCommand`s to the appropriate PTY
13. The `PtyManager` forwards `ChildExited` events as `StateUpdate::SurfaceExited`
14. No `unsafe` code exists outside `posix.rs`
15. Every `unsafe` block has a `// SAFETY:` comment
16. `#![deny(unsafe_code)]` remains on `lib.rs`; `#![allow(unsafe_code)]` is only on `posix.rs`

## Dependencies

**New dependencies for `veil-pty/Cargo.toml`:**

| Dependency | Version | Cfg | Reason |
|-----------|---------|-----|--------|
| `libc` | 0.2 | `[target.'cfg(unix)'.dependencies]` | POSIX FFI: `posix_openpt`, `forkpty`, `ioctl`, `fork`, `execvp`, `waitpid`, signal handling |
| `thiserror` | workspace | all | Error type derivation |
| `tokio` | workspace, features = ["sync"] | all | `tokio::sync::mpsc::Sender` for the PtyManager's state_tx |

**Already available (no changes needed):**

| Dependency | Where | Reason |
|-----------|-------|--------|
| `tracing` | already in veil-pty Cargo.toml | Logging in read/write threads and manager |
| `veil-core` | already in veil-pty Cargo.toml | `SurfaceId`, `StateUpdate`, `AppCommand`, `ShutdownHandle` |

**No new system dependencies.** `libc` is a pure Rust crate that provides FFI bindings. No C compiler or system library installation needed.

**Crate structure after implementation:**

```
crates/veil-pty/
  Cargo.toml
  src/
    lib.rs          -- #![deny(unsafe_code)], re-exports, create_pty factory
    types.rs        -- PtySize, PtyConfig, PtyEvent
    error.rs        -- PtyError
    posix.rs        -- #![allow(unsafe_code)], PosixPty, all POSIX FFI
    windows.rs      -- WindowsPty stub (cfg(windows))
    manager.rs      -- PtyManager actor
```
