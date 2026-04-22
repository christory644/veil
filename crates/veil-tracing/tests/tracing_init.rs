//! Integration test for `veil_tracing::init()`.
//!
//! Each integration test file compiles as a separate binary, so we can
//! call `init()` once here without conflicting with other test binaries.

use veil_tracing::{init, log_dir};

/// Test that `init()` returns a valid guard and that tracing events
/// work after initialization. Also verifies the log directory is created.
///
/// These are grouped in one test because `init()` can only be called once
/// per process (it sets the global tracing subscriber).
#[test]
fn init_sets_up_tracing_and_creates_log_directory() {
    // init() should return a TracingGuard without panicking.
    let _guard = init();

    // After init(), tracing macros should route events to the subscriber.
    // With the stub, no subscriber is set, so these are no-ops -- but they
    // should not panic regardless.
    tracing::info!("integration test: info event");
    tracing::warn!("integration test: warn event");
    tracing::error!("integration test: error event");
    tracing::debug!("integration test: debug event");

    // After init(), log_dir() should return Some and the directory should exist.
    let dir = log_dir();
    assert!(dir.is_some(), "log_dir() should return Some after init()");
    let dir = dir.unwrap();
    assert!(dir.exists(), "log directory should exist after init(): {dir:?}");
}
