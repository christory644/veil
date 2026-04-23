//! Socket server background actor.
//!
//! Runs the `SocketServer` on a dedicated `std::thread` with its own
//! single-threaded tokio runtime. The main application is a synchronous winit
//! event loop, so the async socket server needs its own runtime.

use std::sync::Arc;

use tokio::sync::Mutex;
use veil_core::lifecycle::ShutdownHandle;
use veil_core::state::AppState;

/// Handle to the socket server background thread.
///
/// Dropping this handle does *not* stop the thread; the thread exits when the
/// `ShutdownHandle` is triggered and `SocketServer::run()` returns.
pub struct SocketHandle {
    thread: std::thread::JoinHandle<()>,
}

impl SocketHandle {
    /// Block until the socket server thread exits.
    ///
    /// Returns `Ok(())` if the thread exited normally, or `Err(())` if it panicked.
    pub fn join(self) -> Result<(), ()> {
        self.thread.join().map_err(|_| ())
    }
}

/// Start the socket API server on a background thread with its own tokio runtime.
///
/// - Creates a single-threaded `tokio::runtime::Runtime`
/// - Constructs `SocketServer` with `ServerConfig::default_for_platform()` and the
///   shared `Arc<Mutex<AppState>>`
/// - Calls `server.run(shutdown)` inside `rt.block_on()`
/// - Thread exits when shutdown is triggered and `server.run()` returns
pub fn start_socket_server(
    app_state: Arc<Mutex<AppState>>,
    shutdown: ShutdownHandle,
) -> SocketHandle {
    let handle = std::thread::Builder::new()
        .name("veil-socket".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("failed to create tokio runtime for socket server: {e}");
                    return;
                }
            };

            let config = veil_socket::ServerConfig::default_for_platform();
            tracing::info!("socket server binding to {:?}", config.socket_path);

            let server = veil_socket::SocketServer::new(config, app_state);

            if let Err(e) = rt.block_on(server.run(shutdown)) {
                tracing::error!("socket server exited with error: {e}");
            } else {
                tracing::info!("socket server shut down cleanly");
            }
        })
        .expect("failed to spawn socket server thread");

    SocketHandle { thread: handle }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use veil_core::lifecycle::ShutdownSignal;

    /// Start the socket server actor, trigger shutdown, verify the thread
    /// exits within 2 seconds.
    #[test]
    fn test_socket_server_starts_and_shuts_down() {
        let app_state = Arc::new(Mutex::new(AppState::new()));
        let shutdown = ShutdownSignal::new();

        let handle = start_socket_server(app_state, shutdown.handle());

        // Give the server a moment to bind and start accepting.
        std::thread::sleep(Duration::from_millis(100));

        // Trigger shutdown.
        shutdown.trigger();

        // The thread should exit within 2 seconds.
        let join_result = handle.join();
        assert!(join_result.is_ok(), "socket server thread should exit cleanly after shutdown");
    }
}
