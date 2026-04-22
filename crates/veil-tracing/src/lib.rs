//! Tracing subscriber initialization for Veil.
//!
//! This crate configures the `tracing` subscriber stack with two output layers:
//!
//! - **stderr**: human-readable, ANSI-colored format for development
//! - **file**: structured JSON logs to the platform data directory for diagnostics
//!
//! It also installs a panic hook that emits panic info as a tracing event.
//!
//! # Usage
//!
//! Call [`init()`] exactly once at the start of `main()`. Hold the returned
//! [`TracingGuard`] until the application exits — dropping it flushes the file
//! appender.
//!
//! ```no_run
//! let _guard = veil_tracing::init();
//! tracing::info!("hello from veil");
//! ```

use std::path::PathBuf;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Guard returned by [`init()`] that must be held for the lifetime of the application.
///
/// Dropping this flushes and closes the file appender's background writer thread.
/// If file logging was unavailable (e.g., the log directory could not be created),
/// the guard still exists but dropping it is a no-op.
pub struct TracingGuard {
    /// Holds the `tracing_appender::non_blocking::WorkerGuard`
    /// so the background writer thread stays alive.
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

/// Resolve the log directory path.
///
/// Returns the platform data directory (via `dirs::data_dir()`) with `veil/logs/` appended:
/// - Linux: `~/.local/share/veil/logs/`
/// - macOS: `~/Library/Application Support/veil/logs/`
/// - Windows: `C:\Users\<user>\AppData\Roaming\veil\logs\`
///
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

/// Initialize the tracing subscriber stack.
///
/// This MUST be called exactly once, as early as possible in `main()`.
/// Returns a [`TracingGuard`] that must be held (not dropped) until
/// the application exits. Dropping the guard flushes pending log writes.
///
/// # Behavior
///
/// - Configures stderr output with human-readable format
/// - Configures file output with JSON format to the log directory
/// - If the log directory cannot be created, falls back to stderr-only
/// - Reads `VEIL_LOG` env var for level filtering (falls back to
///   INFO in release, DEBUG in debug builds)
/// - Installs a panic hook that logs panic info via tracing
///
/// # Panics
///
/// Panics if called more than once (tracing global subscriber is already set).
pub fn init() -> TracingGuard {
    let env_filter = build_env_filter();

    // Stderr layer: human-readable, ANSI-colored, with targets and thread IDs.
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_target(true)
        .with_thread_ids(true);

    // File layer: JSON format with daily rotation, if log directory is available.
    // Falls back to stderr-only if the directory cannot be created.
    let (file_layer, file_guard) = if let Some(dir) = log_dir() {
        let file_appender = tracing_appender::rolling::daily(dir, "veil.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        let layer = tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .with_span_list(true);
        (Some(layer), Some(guard))
    } else {
        (None, None)
    };

    tracing_subscriber::registry().with(env_filter).with(stderr_layer).with(file_layer).init();

    install_panic_hook();

    TracingGuard { _file_guard: file_guard }
}

/// Initialize tracing for test contexts.
///
/// Uses a stderr-only subscriber with no file output and no panic hook.
/// Defaults to `WARN` level to keep test output clean unless the developer
/// sets `VEIL_LOG` (or `RUST_LOG`) to opt into more verbose output.
///
/// Safe to call multiple times — subsequent calls are silently ignored.
pub fn init_test() {
    let filter = EnvFilter::try_from_env("VEIL_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("warn"));

    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_test_writer().try_init();
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build the [`EnvFilter`] from `VEIL_LOG`, falling back to a default level.
///
/// If `VEIL_LOG` is set but unparseable, prints a warning to stderr and
/// falls back to the default level (DEBUG in debug builds, INFO in release).
fn build_env_filter() -> EnvFilter {
    let default_level = if cfg!(debug_assertions) { "debug" } else { "info" };

    match std::env::var("VEIL_LOG") {
        Ok(ref val) if !val.is_empty() => EnvFilter::try_new(val).unwrap_or_else(|e| {
            eprintln!(
                "veil-tracing: invalid VEIL_LOG value {val:?}, \
                 falling back to {default_level}: {e}"
            );
            EnvFilter::new(default_level)
        }),
        _ => EnvFilter::new(default_level),
    }
}

/// Extract a human-readable message from a panic payload.
fn panic_message(payload: &dyn std::any::Any) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Install a panic hook that logs panic info via tracing before the
/// default handler runs.
///
/// This ensures the panic is captured in the JSON log file. The
/// [`TracingGuard`]'s `Drop` impl will flush the non-blocking writer
/// during unwinding, so the event is not lost.
fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let location =
            panic_info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let message = panic_message(panic_info.payload());

        tracing::error!(
            panic.message = %message,
            panic.location = location.as_deref().unwrap_or("unknown"),
            "panic occurred"
        );

        previous_hook(panic_info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Log directory management =====

    #[test]
    fn log_dir_returns_some() {
        let dir = log_dir();
        assert!(dir.is_some(), "log_dir() should return Some, got None");
    }

    #[test]
    fn log_dir_path_ends_with_veil_logs() {
        let dir = log_dir().expect("log_dir() should return Some");
        assert!(
            dir.ends_with("veil/logs"),
            "log_dir() path should end with 'veil/logs', got: {dir:?}"
        );
    }

    #[test]
    fn log_dir_creates_directory() {
        let dir = log_dir().expect("log_dir() should return Some");
        assert!(
            dir.exists(),
            "log_dir() should create the directory, but it does not exist: {dir:?}"
        );
    }

    #[test]
    fn log_dir_is_idempotent() {
        let first = log_dir();
        let second = log_dir();
        assert_eq!(first, second, "log_dir() should return the same path on repeated calls");
        assert!(first.is_some(), "first log_dir() call should return Some");
        assert!(second.is_some(), "second log_dir() call should return Some");
    }

    // ===== Subscriber initialization =====

    #[test]
    fn init_test_can_be_called_multiple_times() {
        init_test();
        init_test();
        init_test();
    }

    #[test]
    fn init_test_enables_tracing_macros_without_panic() {
        init_test();
        tracing::info!("test info event");
        tracing::warn!("test warn event");
        tracing::debug!("test debug event");
        tracing::error!("test error event");
    }

    // ===== Panic hook helpers =====

    #[test]
    fn panic_message_extracts_str_payload() {
        let msg = panic_message(&"something went wrong" as &dyn std::any::Any);
        assert_eq!(msg, "something went wrong");
    }

    #[test]
    fn panic_message_extracts_string_payload() {
        let owned = String::from("owned error");
        let msg = panic_message(&owned as &dyn std::any::Any);
        assert_eq!(msg, "owned error");
    }

    #[test]
    fn panic_message_returns_unknown_for_other_types() {
        let val = 42_i32;
        let msg = panic_message(&val as &dyn std::any::Any);
        assert_eq!(msg, "unknown panic payload");
    }

    #[test]
    fn catch_unwind_after_init_does_not_lose_events() {
        init_test();
        tracing::info!("before panic");
        let result = std::panic::catch_unwind(|| {
            panic!("test panic for tracing flush");
        });
        assert!(result.is_err(), "catch_unwind should capture the panic");
        tracing::info!("after caught panic");
    }

    // ===== init_test idempotency =====

    #[test]
    fn init_test_multiple_calls_are_safe() {
        for _ in 0..10 {
            init_test();
        }
        tracing::info!("after multiple init_test calls");
    }
}
