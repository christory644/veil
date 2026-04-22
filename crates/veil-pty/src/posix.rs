//! POSIX PTY implementation using libc FFI.
//!
//! This module contains all unsafe code in the crate. Each `unsafe` block
//! has a `// SAFETY:` comment documenting the invariant.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::error::PtyError;
use crate::types::{PtyConfig, PtyEvent, PtySize};
use crate::Pty;

/// POSIX implementation of the [`Pty`] trait.
///
/// Owns the master side of a PTY pair and the associated child process.
/// Background threads handle reading output and writing input.
pub(crate) struct PosixPty {
    /// Master file descriptor for the PTY.
    #[allow(dead_code)]
    master_fd: libc::c_int,
    /// Child process ID.
    #[allow(dead_code)]
    child_pid: libc::pid_t,
    /// Sender for writing bytes to the PTY. Cloneable.
    write_tx: std::sync::mpsc::Sender<Vec<u8>>,
    /// Receiver for PTY events. Taken by the consumer via `take_event_rx`.
    event_rx: Option<std::sync::mpsc::Receiver<PtyEvent>>,
    /// Whether the PTY has been shut down.
    closed: Arc<AtomicBool>,
    /// Handle to the read thread (for join on shutdown).
    #[allow(dead_code)]
    read_handle: Option<std::thread::JoinHandle<()>>,
    /// Handle to the write thread (for join on shutdown).
    #[allow(dead_code)]
    write_handle: Option<std::thread::JoinHandle<()>>,
}

impl PosixPty {
    /// Create a new POSIX PTY with the given configuration.
    ///
    /// Allocates the PTY pair, spawns the child process, and starts
    /// background read/write threads.
    pub(crate) fn new(_config: PtyConfig) -> Result<Self, PtyError> {
        todo!("PosixPty::new — implement PTY allocation, fork, and thread spawning")
    }
}

impl Pty for PosixPty {
    fn take_event_rx(&mut self) -> Option<std::sync::mpsc::Receiver<PtyEvent>> {
        self.event_rx.take()
    }

    fn writer(&self) -> std::sync::mpsc::Sender<Vec<u8>> {
        self.write_tx.clone()
    }

    fn resize(&self, _size: PtySize) -> Result<(), PtyError> {
        todo!("PosixPty::resize — implement ioctl TIOCSWINSZ")
    }

    fn child_pid(&self) -> Option<u32> {
        todo!("PosixPty::child_pid — return stored pid")
    }

    fn shutdown(&mut self) -> Result<(), PtyError> {
        todo!("PosixPty::shutdown — send SIGHUP, close master fd, join threads")
    }

    fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Acquire)
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::create_pty;
    use std::path::PathBuf;
    use std::time::Duration;

    fn default_config() -> PtyConfig {
        PtyConfig {
            command: None,
            args: vec![],
            working_directory: Some(PathBuf::from("/tmp")),
            env: vec![],
            size: PtySize::default(),
        }
    }

    fn config_with_command(cmd: &str, args: &[&str]) -> PtyConfig {
        PtyConfig {
            command: Some(cmd.to_string()),
            args: args.iter().map(|s| (*s).to_string()).collect(),
            working_directory: Some(PathBuf::from("/tmp")),
            env: vec![],
            size: PtySize::default(),
        }
    }

    // --- Spawn and output ---

    #[test]
    fn spawn_echo_reads_output_containing_hello() {
        let config = config_with_command("/bin/echo", &["hello"]);
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let rx = pty.take_event_rx().expect("should get event receiver");

        let mut output = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if std::time::Instant::now() > deadline {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(PtyEvent::Output(data)) => output.extend_from_slice(&data),
                Ok(PtyEvent::ChildExited { .. }) | Err(_) => break,
            }
        }
        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("hello"), "expected 'hello' in output, got: {text:?}");
    }

    // --- Echo back (cat) ---

    #[test]
    fn spawn_cat_write_then_read_back() {
        let config = config_with_command("/bin/cat", &[]);
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let rx = pty.take_event_rx().expect("should get event receiver");
        let writer = pty.writer();

        writer.send(b"test\n".to_vec()).expect("write should succeed");

        let mut output = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if std::time::Instant::now() > deadline {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(PtyEvent::Output(data)) => {
                    output.extend_from_slice(&data);
                    let text = String::from_utf8_lossy(&output);
                    if text.contains("test") {
                        break;
                    }
                }
                Ok(PtyEvent::ChildExited { .. }) | Err(_) => break,
            }
        }
        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("test"), "expected 'test' in output, got: {text:?}");

        // Clean up: shutdown to avoid leaked child
        pty.shutdown().ok();
    }

    // --- Exit code ---

    #[test]
    fn child_exit_code_is_captured() {
        let config = config_with_command("/bin/sh", &["-c", "exit 42"]);
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let rx = pty.take_event_rx().expect("should get event receiver");

        let mut exit_code = None;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if std::time::Instant::now() > deadline {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(PtyEvent::ChildExited { exit_code: code }) => {
                    exit_code = Some(code);
                    break;
                }
                Ok(PtyEvent::Output(_)) => {}
                Err(_) => break,
            }
        }
        assert_eq!(exit_code, Some(Some(42)), "expected child to exit with code 42");
    }

    // --- Resize ---

    #[test]
    fn resize_returns_ok_on_valid_pty() {
        let config = default_config();
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let _rx = pty.take_event_rx();

        let result = pty.resize(PtySize { cols: 132, rows: 43, pixel_width: 0, pixel_height: 0 });
        assert!(result.is_ok(), "resize should succeed: {result:?}");

        pty.shutdown().ok();
    }

    // --- Shutdown ---

    #[test]
    fn shutdown_sets_is_closed_true() {
        let config = default_config();
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let _rx = pty.take_event_rx();

        assert!(!pty.is_closed(), "should not be closed initially");
        pty.shutdown().expect("shutdown should succeed");
        assert!(pty.is_closed(), "should be closed after shutdown");
    }

    #[test]
    fn shutdown_causes_child_exited_event() {
        let config = config_with_command("/bin/cat", &[]);
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let rx = pty.take_event_rx().expect("should get event receiver");

        pty.shutdown().expect("shutdown should succeed");

        let mut got_exit = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if std::time::Instant::now() > deadline {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(PtyEvent::ChildExited { .. }) => {
                    got_exit = true;
                    break;
                }
                Ok(PtyEvent::Output(_)) => {}
                Err(_) => break,
            }
        }
        assert!(got_exit, "expected ChildExited event after shutdown");
    }

    #[test]
    fn double_shutdown_is_idempotent() {
        let config = default_config();
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let _rx = pty.take_event_rx();

        pty.shutdown().expect("first shutdown should succeed");
        let result = pty.shutdown();
        assert!(result.is_ok(), "second shutdown should also succeed (idempotent): {result:?}");
    }

    // --- Default shell ---

    #[test]
    fn spawn_with_none_command_uses_default_shell() {
        let config = default_config();
        let mut pty = create_pty(config).expect("create_pty with default shell should succeed");
        let _rx = pty.take_event_rx();
        // If we got here, a shell was spawned successfully.
        assert!(pty.child_pid().is_some(), "should have a child PID");
        pty.shutdown().ok();
    }

    // --- Environment injection ---

    #[test]
    fn custom_env_vars_are_visible_in_child() {
        let config = PtyConfig {
            command: Some("/usr/bin/env".to_string()),
            args: vec![],
            working_directory: Some(PathBuf::from("/tmp")),
            env: vec![
                ("VEIL_TEST_VAR".to_string(), "hello_veil".to_string()),
                ("ANOTHER_VAR".to_string(), "42".to_string()),
            ],
            size: PtySize::default(),
        };
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let rx = pty.take_event_rx().expect("should get event receiver");

        let mut output = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if std::time::Instant::now() > deadline {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(PtyEvent::Output(data)) => output.extend_from_slice(&data),
                Ok(PtyEvent::ChildExited { .. }) | Err(_) => break,
            }
        }
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("VEIL_TEST_VAR=hello_veil"),
            "expected VEIL_TEST_VAR in env output, got: {text:?}"
        );
        assert!(
            text.contains("ANOTHER_VAR=42"),
            "expected ANOTHER_VAR in env output, got: {text:?}"
        );
    }

    // --- Large output ---

    #[test]
    fn large_output_arrives_completely() {
        let config = config_with_command("/bin/sh", &["-c", "yes | head -10000"]);
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let rx = pty.take_event_rx().expect("should get event receiver");

        let mut total_bytes = 0;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if std::time::Instant::now() > deadline {
                break;
            }
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(PtyEvent::Output(data)) => total_bytes += data.len(),
                Ok(PtyEvent::ChildExited { .. }) | Err(_) => break,
            }
        }
        // 10000 lines of "y\n" = 20000 bytes minimum
        assert!(
            total_bytes >= 10000,
            "expected at least 10000 bytes of output, got: {total_bytes}"
        );
    }

    // --- Rapid writes ---

    #[test]
    fn rapid_small_writes_are_delivered() {
        let config = config_with_command("/bin/cat", &[]);
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let rx = pty.take_event_rx().expect("should get event receiver");
        let writer = pty.writer();

        // Send many small writes
        for i in 0..100 {
            let msg = format!("line{i}\n");
            writer.send(msg.into_bytes()).expect("write should succeed");
        }

        let mut output = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if std::time::Instant::now() > deadline {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(PtyEvent::Output(data)) => {
                    output.extend_from_slice(&data);
                    let text = String::from_utf8_lossy(&output);
                    // Check if we've received most of our writes
                    if text.contains("line99") {
                        break;
                    }
                }
                Ok(PtyEvent::ChildExited { .. })
                | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("line99"), "expected to see line99 in output after rapid writes");

        pty.shutdown().ok();
    }

    // --- Drop without explicit shutdown ---

    #[test]
    fn drop_without_shutdown_does_not_leak() {
        let config = config_with_command("/bin/cat", &[]);
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let _rx = pty.take_event_rx();
        let pid = pty.child_pid();
        assert!(pid.is_some(), "should have a child PID");

        // Drop without calling shutdown -- Drop impl should clean up.
        drop(pty);

        // Give the OS a moment to reap the child.
        std::thread::sleep(Duration::from_millis(100));

        // Verify the child process is gone (kill with signal 0 checks existence).
        if let Some(pid) = pid {
            // SAFETY: kill(pid, 0) just checks if the process exists.
            let ret = unsafe { libc::kill(pid.cast_signed(), 0) };
            // ret == -1 with ESRCH means the process no longer exists (good).
            // ret == 0 means it still exists (bad, leaked process).
            assert!(ret != 0, "child process {pid} was not reaped after drop");
        }
    }

    // --- take_event_rx returns None on second call ---

    #[test]
    fn take_event_rx_returns_none_on_second_call() {
        let config = default_config();
        let mut pty = create_pty(config).expect("create_pty should succeed");

        let first = pty.take_event_rx();
        assert!(first.is_some(), "first call should return Some");

        let second = pty.take_event_rx();
        assert!(second.is_none(), "second call should return None");

        pty.shutdown().ok();
    }

    // --- child_pid returns a valid PID ---

    #[test]
    fn child_pid_returns_positive_value() {
        let config = default_config();
        let mut pty = create_pty(config).expect("create_pty should succeed");
        let _rx = pty.take_event_rx();

        let pid = pty.child_pid();
        assert!(pid.is_some(), "child_pid should return Some");
        assert!(pid.unwrap() > 0, "PID should be positive");

        pty.shutdown().ok();
    }
}
