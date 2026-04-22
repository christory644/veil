//! Cross-platform socket transport abstraction.
//!
//! On macOS/Linux the transport is a Unix domain socket. The abstraction is
//! designed so Windows named pipe support can be added later without changing
//! callers.

use std::path::{Path, PathBuf};

/// How to locate the socket on the current platform.
#[derive(Debug, Clone)]
pub enum SocketPath {
    /// Unix domain socket at the given filesystem path.
    Unix(PathBuf),
    // Windows(String) — named pipe name, deferred.
}

impl SocketPath {
    /// Resolve the default socket path for the current platform.
    ///
    /// macOS/Linux: `$XDG_RUNTIME_DIR/veil.sock` if the env var is set,
    /// otherwise `/tmp/veil.sock`.
    ///
    /// The resolved path is also what the `VEIL_SOCKET` environment variable
    /// should be set to so that clients can discover the server.
    pub fn default_for_platform() -> Self {
        if let Ok(xdg_dir) = std::env::var("XDG_RUNTIME_DIR") {
            SocketPath::Unix(PathBuf::from(xdg_dir).join("veil.sock"))
        } else {
            SocketPath::Unix(PathBuf::from("/tmp/veil.sock"))
        }
    }

    /// Return the filesystem path for Unix sockets.
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            SocketPath::Unix(path) => Some(path.as_path()),
        }
    }
}

/// A bound socket listener.
pub struct SocketListener {
    inner: tokio::net::UnixListener,
    path: SocketPath,
}

impl SocketListener {
    /// Bind a new listener at the given path.
    ///
    /// Removes any pre-existing socket file before binding (stale socket cleanup).
    // The `async` is intentional: the public API is designed for async callers and
    // future platforms (e.g. Windows named pipes) may require `.await` here.
    #[allow(clippy::unused_async)]
    pub async fn bind(path: SocketPath) -> Result<Self, SocketError> {
        use std::os::unix::fs::PermissionsExt;

        if let Some(fs_path) = path.as_path() {
            match std::fs::remove_file(fs_path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(SocketError::Io(e)),
            }
        }

        let fs_path = path.as_path().ok_or(SocketError::UnsupportedPlatform)?;
        let listener = tokio::net::UnixListener::bind(fs_path)?;
        std::fs::set_permissions(fs_path, std::fs::Permissions::from_mode(0o600))?;

        Ok(SocketListener { inner: listener, path })
    }

    /// Accept the next incoming connection.
    ///
    /// Returns a `(reader, writer)` pair for the connection.
    pub async fn accept(
        &self,
    ) -> Result<
        (
            tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>,
            tokio::io::BufWriter<tokio::net::unix::OwnedWriteHalf>,
        ),
        SocketError,
    > {
        let (stream, _addr) = self.inner.accept().await?;
        let (read_half, write_half) = stream.into_split();
        Ok((tokio::io::BufReader::new(read_half), tokio::io::BufWriter::new(write_half)))
    }

    /// The path this listener is bound to.
    pub fn path(&self) -> &SocketPath {
        &self.path
    }
}

impl Drop for SocketListener {
    fn drop(&mut self) {
        // Remove the socket file on drop.
        if let Some(p) = self.path.as_path() {
            std::fs::remove_file(p).ok();
        }
    }
}

/// Errors from socket transport operations.
#[derive(Debug, thiserror::Error)]
pub enum SocketError {
    /// An underlying I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The current platform is not supported.
    #[error("unsupported platform")]
    UnsupportedPlatform,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::net::UnixStream;

    fn temp_socket_path(dir: &TempDir) -> SocketPath {
        SocketPath::Unix(dir.path().join("test.sock"))
    }

    // ── Unit 2: Transport abstraction ─────────────────────────────────────────

    #[tokio::test]
    async fn bind_creates_socket_file() {
        let dir = TempDir::new().expect("tempdir");
        let path = temp_socket_path(&dir);
        let fs_path = path.as_path().unwrap().to_owned();
        let _listener = SocketListener::bind(path).await.expect("bind");
        assert!(fs_path.exists(), "socket file should exist after bind");
    }

    #[tokio::test]
    async fn bind_removes_stale_socket() {
        let dir = TempDir::new().expect("tempdir");
        let sock_path = dir.path().join("stale.sock");
        // Create a stale file at the socket path.
        std::fs::write(&sock_path, b"stale").expect("create stale file");
        assert!(sock_path.exists());

        let path = SocketPath::Unix(sock_path.clone());
        let _listener = SocketListener::bind(path).await.expect("bind over stale");
        // The file should now be a socket, not the stale content.
        assert!(sock_path.exists(), "socket file should exist after stale removal");
    }

    #[tokio::test]
    async fn drop_removes_socket_file() {
        let dir = TempDir::new().expect("tempdir");
        let path = temp_socket_path(&dir);
        let fs_path = path.as_path().unwrap().to_owned();
        {
            let _listener = SocketListener::bind(path).await.expect("bind");
            assert!(fs_path.exists());
        }
        assert!(!fs_path.exists(), "socket file should be removed on drop");
    }

    #[tokio::test]
    async fn accept_receives_connection() {
        let dir = TempDir::new().expect("tempdir");
        let path = temp_socket_path(&dir);
        let fs_path = path.as_path().unwrap().to_owned();
        let listener = SocketListener::bind(path).await.expect("bind");

        // Connect from a client task.
        let connect_task =
            tokio::spawn(async move { UnixStream::connect(&fs_path).await.expect("connect") });

        // Accept the connection.
        let (reader, _writer) = listener.accept().await.expect("accept");
        // If we get here without panic, the connection was accepted.
        drop(reader);
        connect_task.await.expect("connect task");
    }

    #[tokio::test]
    async fn default_for_platform_unix_fallback() {
        // Ensure XDG_RUNTIME_DIR is not set for this test.
        std::env::remove_var("XDG_RUNTIME_DIR");
        let path = SocketPath::default_for_platform();
        let p = path.as_path().expect("should have a path");
        assert!(p.starts_with("/tmp"), "fallback path should be under /tmp, got: {}", p.display());
    }

    #[tokio::test]
    async fn default_for_platform_uses_xdg_runtime_dir() {
        let dir = TempDir::new().expect("tempdir");
        std::env::set_var("XDG_RUNTIME_DIR", dir.path());
        let path = SocketPath::default_for_platform();
        let p = path.as_path().expect("should have a path");
        assert!(p.starts_with(dir.path()), "path should use XDG_RUNTIME_DIR, got: {}", p.display());
        std::env::remove_var("XDG_RUNTIME_DIR");
    }
}
