//! Error types for PTY operations.

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
        /// The underlying I/O error.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_error_create_display_is_informative() {
        let err = PtyError::Create("posix_openpt failed".to_string());
        let msg = err.to_string();
        assert_eq!(msg, "failed to create PTY: posix_openpt failed");
    }

    #[test]
    fn pty_error_spawn_display_is_informative() {
        let err = PtyError::Spawn("execvp failed: No such file".to_string());
        let msg = err.to_string();
        assert!(msg.contains("failed to spawn child process"));
        assert!(msg.contains("execvp failed"));
    }

    #[test]
    fn pty_error_io_converts_from_std_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broke");
        let pty_err: PtyError = io_err.into();
        let msg = pty_err.to_string();
        assert!(msg.contains("PTY I/O error"));
        assert!(msg.contains("pipe broke"));
    }

    #[test]
    fn pty_error_resize_display_is_informative() {
        let err = PtyError::Resize("ioctl TIOCSWINSZ failed".to_string());
        let msg = err.to_string();
        assert!(msg.contains("failed to resize PTY"));
        assert!(msg.contains("TIOCSWINSZ"));
    }

    #[test]
    fn pty_error_closed_display() {
        let err = PtyError::Closed;
        assert_eq!(err.to_string(), "PTY is closed");
    }

    #[test]
    fn pty_error_unsupported_display() {
        let err = PtyError::Unsupported;
        assert_eq!(err.to_string(), "PTY not supported on this platform");
    }

    #[test]
    fn pty_error_debug_format_exists() {
        let err = PtyError::Create("test".to_string());
        let debug = format!("{err:?}");
        assert!(debug.contains("Create"));
    }

    #[test]
    fn pty_error_io_preserves_error_kind() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no access");
        let pty_err: PtyError = io_err.into();
        match pty_err {
            PtyError::Io { source } => {
                assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
            }
            other => panic!("expected Io variant, got: {other:?}"),
        }
    }
}
