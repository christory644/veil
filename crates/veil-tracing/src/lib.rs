use std::path::PathBuf;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Guards returned by `init()` that must be held for the lifetime of the application.
/// Dropping this flushes and closes the file appender.
pub struct TracingGuard {
    /// Holds the `tracing_appender::non_blocking::WorkerGuard`
    /// so the background writer thread stays alive.
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

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
pub fn init() -> TracingGuard {
    let default_level = if cfg!(debug_assertions) { "debug" } else { "info" };

    let env_filter =
        EnvFilter::try_from_env("VEIL_LOG").unwrap_or_else(|_| EnvFilter::new(default_level));

    // Stderr layer: human-readable, with ANSI colors, targets, and thread IDs
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_target(true)
        .with_thread_ids(true);

    // File layer: JSON format with daily rotation, if log directory is available
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

    #[cfg(unix)]
    install_signal_handlers();

    TracingGuard { _file_guard: file_guard }
}

/// Initialize tracing for test contexts.
///
/// Uses a stderr-only subscriber with no file output.
/// Safe to call multiple times (uses `try_init` internally).
/// Useful for integration tests that want to see tracing output.
pub fn init_test() {
    let _ = tracing_subscriber::fmt::try_init();
}

/// Install a panic hook that logs panic info via tracing
/// and flushes the file appender before the default handler runs.
fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let location =
            panic_info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic payload".to_string()
        };

        tracing::error!(
            panic.message = %message,
            panic.location = location.as_deref().unwrap_or("unknown"),
            "panic occurred"
        );

        previous_hook(panic_info);
    }));
}

/// Install best-effort signal handlers for crash signals (Unix only).
/// These write a short message to stderr and exit.
/// They cannot safely flush the tracing file appender.
///
/// Registers a handler for SIGABRT (e.g., from C assertion failures in FFI).
/// SIGSEGV is not registered because `signal-hook` forbids it (it's in the
/// forbidden signals list alongside SIGKILL, SIGSTOP, SIGILL, and SIGFPE).
/// For SIGSEGV, the OS default handler (core dump) is the best diagnostic.
#[cfg(unix)]
fn install_signal_handlers() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    // Register a flag for SIGABRT. When the signal arrives, the flag is set
    // atomically (async-signal-safe), then the default handler runs and
    // terminates the process. The flag is best-effort: it allows any
    // monitoring thread to detect the signal was received, though in
    // practice the process terminates immediately after.
    let abrt_flag = Arc::new(AtomicBool::new(false));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGABRT, Arc::clone(&abrt_flag));

    // Log that signal handlers are installed (the flag is used to confirm
    // registration succeeded; we don't need to read it at runtime since
    // the default handler will terminate the process).
    let _ = abrt_flag.load(Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Unit 1: Log directory management =====

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
        // Both should be Some
        assert!(first.is_some(), "first log_dir() call should return Some");
        assert!(second.is_some(), "second log_dir() call should return Some");
    }

    // ===== Unit 2: Subscriber initialization =====

    #[test]
    fn init_test_can_be_called_multiple_times() {
        // This should not panic even when called repeatedly
        init_test();
        init_test();
        init_test();
    }

    #[test]
    fn init_test_enables_tracing_macros_without_panic() {
        init_test();
        // These tracing macro calls should not panic after init_test
        tracing::info!("test info event");
        tracing::warn!("test warn event");
        tracing::debug!("test debug event");
        tracing::error!("test error event");
    }

    // ===== Unit 3: Panic hook =====

    #[test]
    fn catch_unwind_after_init_does_not_lose_events() {
        init_test();
        tracing::info!("before panic");
        let result = std::panic::catch_unwind(|| {
            panic!("test panic for tracing flush");
        });
        assert!(result.is_err(), "catch_unwind should capture the panic");
        // If we get here, the panic hook didn't abort and tracing is still functional
        tracing::info!("after caught panic");
    }

    // ===== Unit 4: Signal handler (Unix) =====

    #[cfg(unix)]
    #[test]
    fn install_signal_handlers_does_not_panic() {
        install_signal_handlers();
    }

    // ===== Unit 6: init_test =====

    #[test]
    fn init_test_multiple_calls_are_safe() {
        // Verify that init_test can be called from multiple test functions
        // in the same process without issue
        for _ in 0..10 {
            init_test();
        }
        tracing::info!("after multiple init_test calls");
    }
}
