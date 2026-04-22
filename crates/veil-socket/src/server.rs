//! Socket server actor.
#![allow(dead_code)]
//!
//! Binds the socket, accepts connections in a loop, spawns per-connection
//! tasks, and handles graceful shutdown.

use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use veil_core::state::AppState;

use crate::connection::handle_connection;
use crate::dispatcher::Dispatcher;
use crate::transport::{SocketError, SocketListener, SocketPath};

/// Configuration for the socket server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Where to bind the socket.
    pub socket_path: SocketPath,
}

impl ServerConfig {
    /// Create a config using the platform default socket path.
    pub fn default_for_platform() -> Self {
        Self { socket_path: SocketPath::default_for_platform() }
    }

    /// Create a config with a specific path (used in tests).
    pub fn with_path(path: impl Into<std::path::PathBuf>) -> Self {
        Self { socket_path: SocketPath::Unix(path.into()) }
    }
}

/// The socket API server.
///
/// Binds the socket, accepts connections, spawns connection handlers,
/// and coordinates graceful shutdown.
pub struct SocketServer {
    config: ServerConfig,
    state: Arc<Mutex<AppState>>,
}

impl SocketServer {
    /// Create a new server over the given shared state.
    pub fn new(config: ServerConfig, state: Arc<Mutex<AppState>>) -> Self {
        Self { config, state }
    }

    /// Run the server until shutdown is signaled.
    ///
    /// Binds the socket, then loops accepting connections. Each connection is
    /// handed to `handle_connection` in a spawned task. Returns when `shutdown`
    /// is triggered and all connection tasks complete.
    pub async fn run(
        self,
        mut shutdown: veil_core::lifecycle::ShutdownHandle,
    ) -> Result<(), SocketError> {
        let listener = SocketListener::bind(self.config.socket_path).await?;
        let dispatcher = Arc::new(Dispatcher::new(self.state));
        let mut tasks: JoinSet<()> = JoinSet::new();

        loop {
            tokio::select! {
                () = shutdown.wait() => break,
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((reader, writer)) => {
                            let dispatcher = Arc::clone(&dispatcher);
                            let conn_shutdown = shutdown.clone();
                            tasks.spawn(async move {
                                handle_connection(reader, writer, dispatcher, conn_shutdown).await;
                            });
                        }
                        Err(e) => {
                            tracing::error!("accept error: {e}");
                        }
                    }
                }
            }
        }

        // Abort all remaining connection tasks — they observe shutdown via their
        // own ShutdownHandle clones, so this is belt-and-suspenders cleanup.
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}

        Ok(())
    }

    /// Returns the socket path this server is (or will be) bound to.
    pub fn socket_path(&self) -> &SocketPath {
        &self.config.socket_path
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;
    use veil_core::lifecycle::ShutdownSignal;

    fn make_server(dir: &TempDir) -> (SocketServer, ShutdownSignal, std::path::PathBuf) {
        let sock_path = dir.path().join("server.sock");
        let config = ServerConfig::with_path(sock_path.clone());
        let state = Arc::new(Mutex::new(AppState::new()));
        let signal = ShutdownSignal::new();
        let server = SocketServer::new(config, state);
        (server, signal, sock_path)
    }

    async fn connect_and_exchange(sock_path: &std::path::Path, request: &str) -> serde_json::Value {
        let stream = UnixStream::connect(sock_path).await.expect("connect");
        let (read_half, write_half) = stream.into_split();
        let mut writer = tokio::io::BufWriter::new(write_half);
        let mut reader = BufReader::new(read_half);

        let line = format!("{request}\n");
        writer.write_all(line.as_bytes()).await.expect("write");
        writer.flush().await.expect("flush");

        let mut resp_line = String::new();
        reader.read_line(&mut resp_line).await.expect("read");
        serde_json::from_str(resp_line.trim()).expect("parse response")
    }

    async fn wait_for_socket(sock_path: &std::path::Path) {
        for _ in 0..50 {
            if sock_path.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("socket file did not appear within timeout");
    }

    // ── Unit 7: Socket server actor ───────────────────────────────────────────

    #[tokio::test]
    async fn server_binds_and_accepts_connection() {
        let dir = TempDir::new().expect("tempdir");
        let (server, signal, sock_path) = make_server(&dir);
        let shutdown = signal.handle();

        tokio::spawn(async move {
            server.run(shutdown).await.expect("server run");
        });

        wait_for_socket(&sock_path).await;

        // Connect a client — this should succeed without panicking.
        let _stream = UnixStream::connect(&sock_path).await.expect("connect");

        signal.trigger();
    }

    #[tokio::test]
    async fn server_processes_workspace_list_request() {
        let dir = TempDir::new().expect("tempdir");
        let (server, signal, sock_path) = make_server(&dir);
        let shutdown = signal.handle();

        tokio::spawn(async move {
            server.run(shutdown).await.expect("server run");
        });

        wait_for_socket(&sock_path).await;

        let req = json!({"jsonrpc":"2.0","method":"workspace.list","id":1}).to_string();
        let resp = connect_and_exchange(&sock_path, &req).await;

        assert_eq!(resp["jsonrpc"], "2.0");
        assert!(resp.get("result").is_some(), "should have result key");
        let arr = resp["result"].as_array().expect("result should be array");
        assert!(arr.is_empty(), "fresh state should return empty list");

        signal.trigger();
    }

    #[tokio::test]
    async fn server_processes_workspace_create_then_list() {
        let dir = TempDir::new().expect("tempdir");
        let (server, signal, sock_path) = make_server(&dir);
        let shutdown = signal.handle();

        tokio::spawn(async move {
            server.run(shutdown).await.expect("server run");
        });

        wait_for_socket(&sock_path).await;

        let create_req = json!({
            "jsonrpc": "2.0",
            "method": "workspace.create",
            "params": {"name": "myws", "working_directory": "/tmp"},
            "id": 1
        })
        .to_string();

        let create_resp = connect_and_exchange(&sock_path, &create_req).await;
        assert!(create_resp.get("result").is_some(), "create should return result");

        let list_req = json!({"jsonrpc":"2.0","method":"workspace.list","id":2}).to_string();
        let list_resp = connect_and_exchange(&sock_path, &list_req).await;

        let arr = list_resp["result"].as_array().expect("should be array");
        assert_eq!(arr.len(), 1, "should have one workspace after create");
        assert_eq!(arr[0]["name"], "myws");

        signal.trigger();
    }

    #[tokio::test]
    async fn server_handles_concurrent_clients() {
        let dir = TempDir::new().expect("tempdir");
        let (server, signal, sock_path) = make_server(&dir);
        let shutdown = signal.handle();

        tokio::spawn(async move {
            server.run(shutdown).await.expect("server run");
        });

        wait_for_socket(&sock_path).await;

        let sock_path1 = sock_path.clone();
        let sock_path2 = sock_path.clone();

        let req = json!({"jsonrpc":"2.0","method":"workspace.list","id":1}).to_string();

        let t1 = tokio::spawn({
            let req = req.clone();
            async move { connect_and_exchange(&sock_path1, &req).await }
        });
        let t2 = tokio::spawn({
            let req = req.clone();
            async move { connect_and_exchange(&sock_path2, &req).await }
        });

        let (r1, r2) = tokio::join!(t1, t2);
        let r1 = r1.expect("client 1 task");
        let r2 = r2.expect("client 2 task");

        assert!(r1.get("result").is_some(), "client 1 should get result");
        assert!(r2.get("result").is_some(), "client 2 should get result");

        signal.trigger();
    }

    #[tokio::test]
    async fn server_shuts_down_cleanly() {
        let dir = TempDir::new().expect("tempdir");
        let (server, signal, sock_path) = make_server(&dir);
        let shutdown = signal.handle();

        let server_task = tokio::spawn(async move {
            server.run(shutdown).await.expect("server run");
        });

        wait_for_socket(&sock_path).await;
        signal.trigger();

        tokio::time::timeout(Duration::from_secs(5), server_task)
            .await
            .expect("server should shut down within 5 seconds")
            .expect("server task panicked");
    }

    #[tokio::test]
    async fn server_invalid_request_does_not_crash() {
        let dir = TempDir::new().expect("tempdir");
        let (server, signal, sock_path) = make_server(&dir);
        let shutdown = signal.handle();

        tokio::spawn(async move {
            server.run(shutdown).await.expect("server run");
        });

        wait_for_socket(&sock_path).await;

        // Send malformed JSON.
        let bad_resp = connect_and_exchange(&sock_path, "not json at all!!!").await;
        assert_eq!(bad_resp["error"]["code"], -32700_i64);

        // Server should still accept new connections.
        let good_req = json!({"jsonrpc":"2.0","method":"workspace.list","id":2}).to_string();
        let good_resp = connect_and_exchange(&sock_path, &good_req).await;
        assert!(good_resp.get("result").is_some(), "server should still be alive");

        signal.trigger();
    }

    #[tokio::test]
    async fn server_sets_socket_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().expect("tempdir");
        let (server, signal, sock_path) = make_server(&dir);
        let shutdown = signal.handle();

        tokio::spawn(async move {
            server.run(shutdown).await.expect("server run");
        });

        wait_for_socket(&sock_path).await;

        let metadata = std::fs::metadata(&sock_path).expect("socket metadata");
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "socket should be owner-only (0600), got {mode:o}");

        signal.trigger();
    }
}
