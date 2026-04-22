# VEI-19: Observability — Tracing Integration

## Context

Veil currently uses `tracing` macros throughout its crates (`tracing::debug!`, `tracing::warn!`, `tracing::error!`, `tracing::info!`) but has **no subscriber configured**. Every `tracing` call is a no-op at runtime because there is nothing receiving the events. The `main()` function in `crates/veil/src/main.rs` does `println!("veil v{}", ...)` and jumps straight into the winit event loop with no logging initialization.

This task sets up the tracing subscriber infrastructure so that all existing and future `tracing` calls actually produce output. The subscriber stack will support multiple simultaneous outputs:

1. **stderr** -- human-readable, colored format for development
2. **File** -- structured JSON logs to `~/.local/share/veil/logs/` for user-reportable diagnostics
3. **Crash safety** -- panic hook and signal handler to flush buffers before exit

### Why this matters

Without a functioning tracing subscriber, debugging Veil is impossible. Config watcher errors, PTY lifecycle events, adapter failures, render errors -- all of these already emit `tracing` events that silently vanish. This is the foundational observability layer that every other subsystem depends on.

### Scope boundaries

- **In scope**: Tracing subscriber setup, multi-output layering, log directory management, level filtering, panic hook, signal handler, VEIL_LOG environment variable override, init function callable from main.
- **Out of scope**: Debug overlay (egui panel). The debug overlay depends on veil-ui and winit integration which are not yet wired up for overlay rendering. The overlay is deferred to a follow-up issue. This spec does NOT include any egui, rendering, or UI code.

### Where this lives

The tracing subscriber initialization lives in a new `veil-tracing` crate. Rationale: the subscriber setup requires `tracing-subscriber` (with `fmt`, `json`, `env-filter`, `registry` features) plus `tracing-appender` -- these are heavyweight dependencies that only the binary crate needs. Library crates (`veil-core`, `veil-pty`, etc.) already depend on `tracing` for macros and need nothing else. A dedicated crate keeps the dependency graph clean and makes the init function testable in isolation.

The binary crate (`crates/veil/src/main.rs`) calls `veil_tracing::init()` as the very first line of `main()`, before any other initialization.

## Implementation Units

### Unit 1: Create `veil-tracing` crate with log directory management

Create the new crate and implement the log directory resolution logic.

**Files:**
- `crates/veil-tracing/Cargo.toml`
- `crates/veil-tracing/src/lib.rs`

**Cargo.toml:**

```toml
[package]
name = "veil-tracing"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true

[dependencies]
tracing.workspace = true
tracing-subscriber = { version = "0.3", features = ["env-filter", "json", "fmt", "std"] }
tracing-appender = "0.2"
dirs.workspace = true

[dev-dependencies]
tempfile.workspace = true

[lints]
workspace = true
```

**Add to workspace `Cargo.toml`:**
- Add `"crates/veil-tracing"` to `[workspace].members`
- Add `tracing-subscriber = { version = "0.3", features = ["env-filter", "json", "fmt", "std"] }` and `tracing-appender = "0.2"` to `[workspace.dependencies]`

**Types and functions:**

```rust
// crates/veil-tracing/src/lib.rs

use std::path::PathBuf;

/// Resolve the log directory path.
///
/// Returns `~/.local/share/veil/logs/` on macOS/Linux,
/// or the platform equivalent via `dirs::data_dir()`.
/// Creates the directory if it does not exist.
///
/// Returns `None` if the data directory cannot be determined
/// (the caller should fall back to stderr-only logging).
pub fn log_dir() -> Option<PathBuf> {
    let base = dirs::data_dir()?;
    let dir = base.join("veil").join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}
```

**Tests:**

- `log_dir()` returns `Some` on all supported platforms
- The returned path ends with `veil/logs`
- The returned directory exists after the call (it was created)
- Calling `log_dir()` twice is idempotent (does not error on existing directory)

### Unit 2: Subscriber initialization with layered outputs

Build the subscriber stack: stderr fmt layer + file JSON layer + env filter. This is the core of the task.

**Design:**

The subscriber uses `tracing_subscriber::registry()` with two layers:

1. **stderr layer** (`fmt::Layer`) -- human-readable, with ANSI colors, target names, thread IDs. Active in all builds. Filtered by `VEIL_LOG` env var or default level.
2. **file layer** (`fmt::Layer` with JSON formatter) -- structured JSON, one event per line, written via `tracing_appender::rolling::daily` to the log directory. Includes timestamp, level, target, span context, and all structured fields.

**Level filtering:**

- Default level: `INFO` in release builds, `DEBUG` in debug builds
- Override via `VEIL_LOG` environment variable (parsed as `tracing_subscriber::EnvFilter` directive, e.g., `VEIL_LOG=veil_pty=trace,veil_core::config=debug`)
- If `VEIL_LOG` is set but unparseable, log a warning to stderr and fall back to the default level

**Functions:**

```rust
/// Guards returned by `init()` that must be held for the lifetime of the application.
/// Dropping this flushes and closes the file appender.
pub struct TracingGuard {
    // Holds the tracing_appender::non_blocking::WorkerGuard
    // so the background writer thread stays alive.
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

/// Initialize the tracing subscriber stack.
///
/// This MUST be called exactly once, as early as possible in `main()`.
/// Returns a `TracingGuard` that must be held (not dropped) until
/// the application exits. Dropping the guard flushes pending log writes.
///
/// # Behavior
///
/// - Configures stderr output with human-readable format
/// - Configures file output with JSON format to the log directory
/// - If the log directory cannot be created, falls back to stderr-only
/// - Reads `VEIL_LOG` env var for level filtering (falls back to
///   INFO in release, DEBUG in debug builds)
/// - Installs a panic hook that flushes tracing buffers
///
/// # Panics
///
/// Panics if called more than once (tracing global subscriber is already set).
pub fn init() -> TracingGuard;

/// Initialize tracing for test contexts.
///
/// Uses a stderr-only subscriber with no file output.
/// Safe to call multiple times (uses `try_init` internally).
/// Useful for integration tests that want to see tracing output.
pub fn init_test();
```

**Implementation notes:**

- The file layer uses `tracing_appender::rolling::daily` for automatic daily log rotation.
- The file appender is wrapped in `tracing_appender::non_blocking` to avoid blocking the application on file I/O. The returned `WorkerGuard` is stored in `TracingGuard`.
- ERROR and WARN events on stderr use an unbuffered writer (`std::io::stderr()`) rather than a buffered one. The `fmt::Layer` on stderr is constructed with `.with_writer(std::io::stderr)` which is already unbuffered.
- The JSON file layer includes: `timestamp`, `level`, `target`, `module_path`, `file`, `line`, `fields`, `span` context.

**Tests:**

*Initialization:*
- `init()` succeeds and returns a `TracingGuard`
- After `init()`, `tracing::info!("test")` does not panic (subscriber is installed)
- `init_test()` can be called multiple times without panicking

*Log directory fallback:*
- When log directory creation fails (simulated), `init()` still succeeds with stderr-only output
- `TracingGuard` can be dropped without panicking

*Environment variable:*
- When `VEIL_LOG` is set to a valid filter (e.g., `trace`), the subscriber respects it
- When `VEIL_LOG` is not set, the default level is applied (DEBUG in `#[cfg(debug_assertions)]`, INFO otherwise)

Note: Testing the actual output content of tracing is difficult because the global subscriber can only be set once per process. Tests should focus on "does not panic" and "guard is valid" semantics. A dedicated integration test binary (`tests/tracing_init.rs`) can verify output by capturing stderr.

### Unit 3: Panic hook for buffer flushing

Install a custom panic hook that ensures tracing buffers are flushed before the process terminates. This is critical for crash diagnostics -- without it, the last ERROR/WARN events before a panic may be lost in the non-blocking file writer's buffer.

**Design:**

The panic hook is installed inside `init()`. It:
1. Captures the previous panic hook (the default one)
2. On panic: emits a `tracing::error!` with the panic info (location, message, backtrace)
3. Drops or explicitly flushes the file writer guard (this is handled by the guard's Drop impl, but we want to ensure it happens before the default panic hook's abort)
4. Calls the previous panic hook so the user still sees the standard panic output

```rust
/// Install a panic hook that logs panic info via tracing
/// and flushes the file appender before the default handler runs.
fn install_panic_hook();
```

**Implementation note:** The `TracingGuard` returned by `init()` is what keeps the non-blocking writer alive. On panic, the guard's `Drop` will run during unwinding, flushing the buffer. The panic hook's job is to emit the panic as a tracing event *before* that happens, so it gets captured in the log file.

**Tests:**

- After `init()`, the panic hook is installed (verify by checking `std::panic::set_hook` behavior -- this is hard to test directly; instead, test that `init()` doesn't panic and the returned guard is valid)
- A caught panic (via `std::panic::catch_unwind`) after `init()` does not lose tracing events emitted before the panic (integration test)

### Unit 4: Signal handler for crash safety (Unix only)

Install signal handlers for SIGSEGV and SIGABRT that perform best-effort flush of tracing buffers. This handles cases where the process crashes due to FFI (libghosty) issues rather than Rust panics.

**Design:**

This is Unix-only (`#[cfg(unix)]`). Windows crash handling is deferred.

Uses the `signal-hook` crate (lightweight, safe signal handler registration) to register handlers for:
- `SIGSEGV` -- segmentation fault (likely from FFI)
- `SIGABRT` -- abort (e.g., from C assertion failure)

The handler is minimal and async-signal-safe: it writes a fixed message to stderr and calls `_exit(128 + signal_number)`. It cannot flush the tracing file appender (file I/O is not async-signal-safe), but the non-blocking appender's background thread may have already flushed recent events.

**Alternative considered:** Using `libc::signal` directly. Rejected in favor of `signal-hook` for safety and portability.

**Functions:**

```rust
/// Install best-effort signal handlers for crash signals (Unix only).
/// These write a short message to stderr and exit.
/// They cannot safely flush the tracing file appender.
#[cfg(unix)]
fn install_signal_handlers();
```

**Dependencies:**
- Add `signal-hook = "0.3"` to workspace and veil-tracing dependencies, gated behind `#[cfg(unix)]`.

**Cargo.toml addition for veil-tracing:**

```toml
[target.'cfg(unix)'.dependencies]
signal-hook = "0.3"
```

**Tests:**

Signal handlers are inherently difficult to test. The tests verify:
- `install_signal_handlers()` does not panic
- The function is a no-op on non-Unix platforms (compile-time gating)
- Integration test: send SIGABRT to a child process that has tracing initialized; verify it exits with the expected code (128 + 6 = 134)

### Unit 5: Wire `init()` into `main.rs`

Update the binary crate to call `veil_tracing::init()` as the first operation in `main()`, and hold the guard for the application lifetime.

**Changes:**

```rust
// crates/veil/src/main.rs

fn main() -> anyhow::Result<()> {
    let _tracing_guard = veil_tracing::init();

    tracing::info!("veil v{}", env!("CARGO_PKG_VERSION"));

    let event_loop = EventLoop::new()?;
    let mut app = VeilApp::new();
    event_loop.run_app(&mut app)?;

    Ok(())
}
```

**Changes to `crates/veil/Cargo.toml`:**
- Add `veil-tracing = { path = "../veil-tracing" }` to `[dependencies]`

**Remove:**
- The `println!("veil v{}", ...)` line is replaced by `tracing::info!`

**Tests:**

- `cargo build -p veil` succeeds
- `cargo run -p veil` (if display is available) shows tracing output on stderr
- The version line appears as an INFO-level tracing event rather than a bare println

### Unit 6: `init_test()` for test harnesses

Provide a no-file-output subscriber initializer that test code across all crates can use to see tracing output during test runs.

**Design:**

`init_test()` uses `tracing_subscriber::fmt::try_init()` which is safe to call multiple times (subsequent calls silently succeed). It configures:
- stderr output only (no file)
- `RUST_LOG` or `VEIL_LOG` env var for filtering (defaults to `WARN` to keep test output clean unless the developer opts in)
- No panic hook (tests have their own panic handling)

This function is already declared in Unit 2 but called out separately because every crate's test suite benefits from it.

**Usage pattern (any crate's tests):**

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn some_test() {
        veil_tracing::init_test();
        // ... test code that emits tracing events ...
    }
}
```

**Note:** Adding `veil-tracing` as a dev-dependency to every crate is optional and can be done incrementally. The function exists and is available; crates adopt it as needed.

**Tests:**

- `init_test()` can be called from multiple tests in the same process without panicking
- After `init_test()`, `tracing::warn!("test")` does not panic
- `init_test()` with `VEIL_LOG=trace` shows trace-level output

## Acceptance Criteria

1. `cargo build -p veil-tracing` succeeds
2. `cargo test -p veil-tracing` passes all tests
3. `cargo build -p veil` succeeds with the new `veil-tracing` dependency
4. `cargo clippy --all-targets --all-features -- -D warnings` passes
5. `cargo fmt --check` passes
6. Running `cargo run -p veil` (or the binary directly) shows human-readable tracing output on stderr, including the startup version line as an INFO event
7. A `~/.local/share/veil/logs/` directory is created on first run, containing a JSON log file with structured events
8. Setting `VEIL_LOG=trace` increases the verbosity of both stderr and file output
9. Setting `VEIL_LOG=error` reduces output to errors only
10. An invalid `VEIL_LOG` value (e.g., `VEIL_LOG=???`) falls back to the default level without crashing
11. The panic hook is installed: a `panic!()` in debug builds produces a tracing ERROR event in the log file before the standard panic output
12. On Unix: signal handlers are registered for SIGSEGV and SIGABRT
13. The `TracingGuard` drop flushes the file appender (no lost events on clean shutdown)
14. `init_test()` is available for test harnesses and can be called multiple times safely
15. No egui, rendering, or UI code exists in this task (debug overlay is deferred)
16. The `println!` in `main()` is replaced with `tracing::info!`
17. All existing `tracing::*` calls throughout the codebase now produce visible output when the application runs

## Dependencies

**New workspace crate:**

| Crate | Purpose |
|-------|---------|
| `veil-tracing` | Tracing subscriber setup, log directory, panic/signal hooks |

**New crate dependencies:**

| Location | Dependency | Version | Reason |
|----------|-----------|---------|--------|
| workspace `Cargo.toml` | `tracing-subscriber` | `0.3` (features: `env-filter`, `json`, `fmt`, `std`) | Subscriber registry, formatting layers, environment filter |
| workspace `Cargo.toml` | `tracing-appender` | `0.2` | Non-blocking file appender with daily rotation |
| workspace `Cargo.toml` | `signal-hook` | `0.3` | Safe signal handler registration (Unix) |
| veil-tracing `Cargo.toml` | `tracing` | (workspace) | Core tracing macros |
| veil-tracing `Cargo.toml` | `tracing-subscriber` | (workspace) | Subscriber layers |
| veil-tracing `Cargo.toml` | `tracing-appender` | (workspace) | File appender |
| veil-tracing `Cargo.toml` | `dirs` | (workspace) | Platform data directory resolution |
| veil-tracing `Cargo.toml` (unix) | `signal-hook` | (workspace) | Signal handlers |
| veil `Cargo.toml` | `veil-tracing` | `{ path = "../veil-tracing" }` | Init call from main |

**Existing dependencies already available:**
- `tracing` 0.1 -- already in workspace dependencies, used by all crates
- `dirs` -- already in workspace dependencies, used by veil-core
- `tempfile` -- already in workspace dev-dependencies (for tests)

**No new external tools or software required.**

## Deferred Work

The following items from the VEI-19 task description are explicitly deferred to follow-up issues:

- **Debug overlay** (egui-rendered panel with AppState tree, actor status, frame time graph, channel depths, SQLite stats, PTY throughput) -- depends on veil-ui overlay rendering infrastructure that does not exist yet
- **`--debug` CLI flag** -- will be added when the debug overlay is implemented
- **`debug-overlay` compile feature** -- same as above
- **F12 toggle shortcut** -- same as above
- **Live tracing feed to debug overlay** -- requires a custom `tracing::Layer` that buffers events for the overlay; will be implemented alongside the overlay UI
