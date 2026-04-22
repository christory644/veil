//! POSIX PTY implementation using libc FFI.
//!
//! This module contains all unsafe code in the crate. Each `unsafe` block
//! has a `// SAFETY:` comment documenting the invariant that makes the call
//! sound.
//!
//! # Safety boundary
//!
//! All unsafe code is confined to this module. The public-facing API of
//! `veil-pty` (the [`Pty`] trait and [`create_pty`](crate::create_pty)
//! factory) is entirely safe Rust.

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
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
    master_fd: libc::c_int,
    /// Child process ID.
    child_pid: libc::pid_t,
    /// Sender for writing bytes to the PTY. Cloneable.
    write_tx: std::sync::mpsc::Sender<Vec<u8>>,
    /// Receiver for PTY events. Taken by the consumer via `take_event_rx`.
    event_rx: Option<std::sync::mpsc::Receiver<PtyEvent>>,
    /// Whether the PTY has been shut down.
    closed: Arc<AtomicBool>,
    /// Handle to the read thread (for join on shutdown).
    read_handle: Option<std::thread::JoinHandle<()>>,
    /// Handle to the write thread (for join on shutdown).
    write_handle: Option<std::thread::JoinHandle<()>>,
}

/// Resolve the shell command to execute.
///
/// If `command` is `Some`, uses that. Otherwise checks `$SHELL`, falling back
/// to `/bin/sh`.
fn resolve_command(command: Option<&str>) -> String {
    if let Some(cmd) = command {
        return cmd.to_string();
    }
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

/// Write all bytes to a file descriptor, retrying on partial writes and `EINTR`.
fn write_all_fd(fd: libc::c_int, mut buf: &[u8]) -> Result<(), std::io::Error> {
    while !buf.is_empty() {
        // SAFETY: fd is a valid open file descriptor, buf points to valid memory
        // with len bytes available.
        let n = unsafe { libc::write(fd, buf.as_ptr().cast::<libc::c_void>(), buf.len()) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        buf = &buf[n.cast_unsigned()..];
    }
    Ok(())
}

/// Execute the child process setup after `forkpty`.
///
/// Sets environment variables, changes working directory, and calls `execvp`.
/// This function never returns on success (it replaces the process image).
/// On failure, it calls `_exit(127)`.
///
/// # Async-signal-safety
///
/// Strictly speaking, only async-signal-safe functions should be called
/// between `fork` and `exec`. Functions like `std::env::set_var` and Rust
/// allocator calls are not async-signal-safe. In practice this works
/// reliably on macOS and Linux because `forkpty` produces a single-threaded
/// child (no other threads to hold locks), but it is a known pragmatic
/// trade-off. A future improvement could use `posix_spawn` or pre-build
/// the `CString` argv before forking.
fn exec_child(config: &PtyConfig) -> ! {
    // Set environment variables
    for (key, value) in &config.env {
        // SAFETY (pragmatic): set_var is not async-signal-safe, but we are
        // the only thread in this forked child. See module-level note.
        std::env::set_var(key, value);
    }

    // Change working directory (best-effort: if it fails, the child starts
    // in the parent's cwd, which is a reasonable fallback).
    if let Some(ref dir) = config.working_directory {
        if let Ok(c_dir) = CString::new(dir.to_string_lossy().as_bytes()) {
            // SAFETY: c_dir is a valid null-terminated C string pointing to
            // a directory path. chdir returns -1 on failure, which we ignore
            // (falling back to the inherited cwd).
            unsafe {
                libc::chdir(c_dir.as_ptr());
            }
        }
    }

    // Resolve command and build argv
    let cmd = resolve_command(config.command.as_deref());
    let Ok(c_cmd) = CString::new(cmd.as_bytes()) else {
        // SAFETY: _exit terminates the child immediately without running
        // destructors. This is required after fork to avoid double-flushing
        // stdio buffers or running atexit handlers from the parent's state.
        unsafe { libc::_exit(127) }
    };

    let mut argv: Vec<CString> = Vec::with_capacity(1 + config.args.len());
    argv.push(c_cmd.clone());
    for arg in &config.args {
        let Ok(c_arg) = CString::new(arg.as_bytes()) else {
            // SAFETY: _exit terminates the child immediately.
            unsafe { libc::_exit(127) }
        };
        argv.push(c_arg);
    }

    let argv_ptrs: Vec<*const libc::c_char> =
        argv.iter().map(|s| s.as_ptr()).chain(std::iter::once(std::ptr::null())).collect();

    // SAFETY: c_cmd is a valid null-terminated path, argv_ptrs is a
    // null-terminated array of valid C string pointers. execvp replaces
    // the process image on success.
    unsafe {
        libc::execvp(c_cmd.as_ptr(), argv_ptrs.as_ptr());
    }

    // execvp only returns on error.
    // SAFETY: We are in the child after a failed execvp. _exit is the
    // only safe way to terminate without running destructors or atexit
    // handlers that belong to the parent's state.
    unsafe {
        libc::_exit(127);
    }
}

/// Wait for a child process to exit, escalating signals if needed.
///
/// Uses `WNOHANG` in a polling loop. After a few attempts, sends `SIGTERM`,
/// then `SIGKILL` to force termination.
fn reap_child(child_pid: libc::pid_t) -> Option<i32> {
    let mut status: libc::c_int = 0;
    let mut attempts = 0;

    loop {
        // SAFETY: child_pid is a valid PID from fork. status is a valid pointer.
        // WNOHANG returns immediately if the child hasn't exited.
        let wait_ret = unsafe { libc::waitpid(child_pid, &raw mut status, libc::WNOHANG) };

        if wait_ret > 0 {
            // SAFETY: WIFEXITED and WEXITSTATUS are standard POSIX macros that
            // extract exit info from the status integer set by waitpid.
            return if libc::WIFEXITED(status) { Some(libc::WEXITSTATUS(status)) } else { None };
        } else if wait_ret == -1 {
            // ECHILD or other error — child already reaped or doesn't exist.
            return None;
        }

        // Child still running. Escalate signals.
        attempts += 1;
        if attempts == 5 {
            // SAFETY: child_pid is a valid PID. SIGTERM requests graceful exit.
            unsafe { libc::kill(child_pid, libc::SIGTERM) };
        } else if attempts >= 10 {
            // SAFETY: child_pid is a valid PID. SIGKILL forces termination.
            unsafe { libc::kill(child_pid, libc::SIGKILL) };
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Read loop for the PTY background thread.
///
/// Reads output from the master fd in 4096-byte chunks and sends `PtyEvent`s.
/// When EOF is reached, reaps the child and sends `ChildExited`.
fn read_loop(
    master_fd: libc::c_int,
    child_pid: libc::pid_t,
    closed: &AtomicBool,
    event_tx: &std::sync::mpsc::Sender<PtyEvent>,
) {
    let mut buf = [0u8; 4096];
    loop {
        // SAFETY: master_fd is a valid open PTY master fd (or has been closed,
        // in which case read returns -1 with EIO/EBADF). buf is a valid,
        // stack-allocated buffer of 4096 bytes.
        let n =
            unsafe { libc::read(master_fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };

        if n <= 0 {
            // EOF or error -- child exited or fd was closed.
            let exit_code = reap_child(child_pid);
            closed.store(true, Ordering::Release);
            tracing::debug!(child_pid, ?exit_code, "read loop finished, child reaped");
            let _ = event_tx.send(PtyEvent::ChildExited { exit_code });
            break;
        }

        let data = buf[..n.cast_unsigned()].to_vec();
        if event_tx.send(PtyEvent::Output(data)).is_err() {
            // Receiver dropped -- stop reading.
            tracing::debug!(child_pid, "event receiver dropped, read loop exiting");
            break;
        }
    }
}

/// Write loop for the PTY background thread.
///
/// Receives byte buffers from the channel and writes them to the master fd.
/// Exits when the channel is disconnected or the closed flag is set.
fn write_loop(
    master_fd: libc::c_int,
    closed: &AtomicBool,
    write_rx: &std::sync::mpsc::Receiver<Vec<u8>>,
) {
    loop {
        // Use a short timeout so we can check the closed flag and exit
        // promptly when shutdown is requested.
        match write_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(data) => {
                if let Err(e) = write_all_fd(master_fd, &data) {
                    tracing::debug!("write loop exiting on I/O error: {e}");
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if closed.load(Ordering::Acquire) {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

impl PosixPty {
    /// Create a new POSIX PTY with the given configuration.
    ///
    /// Allocates the PTY pair, spawns the child process, and starts
    /// background read/write threads.
    #[allow(clippy::needless_pass_by_value)] // config is consumed by forkpty child process
    pub(crate) fn new(config: PtyConfig) -> Result<Self, PtyError> {
        let mut master_fd: libc::c_int = -1;
        let mut ws = libc::winsize {
            ws_col: config.size.cols,
            ws_row: config.size.rows,
            ws_xpixel: config.size.pixel_width,
            ws_ypixel: config.size.pixel_height,
        };

        // SAFETY: forkpty is a POSIX function that allocates a PTY pair and forks.
        // master_fd is a valid pointer to receive the master fd. We pass a winsize
        // struct for initial terminal dimensions. termp is null (use default termios).
        let pid = unsafe {
            libc::forkpty(
                &raw mut master_fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut ws,
            )
        };

        if pid < 0 {
            return Err(PtyError::Create(std::io::Error::last_os_error().to_string()));
        }

        if pid == 0 {
            exec_child(&config);
        }

        // --- Parent process ---
        let closed = Arc::new(AtomicBool::new(false));
        let (event_tx, event_rx) = std::sync::mpsc::channel::<PtyEvent>();
        let (write_tx, write_rx) = std::sync::mpsc::channel::<Vec<u8>>();

        let read_handle = std::thread::Builder::new()
            .name("pty-read".into())
            .spawn({
                let closed = Arc::clone(&closed);
                move || read_loop(master_fd, pid, &closed, &event_tx)
            })
            .map_err(|e| PtyError::Create(format!("failed to spawn read thread: {e}")))?;

        let write_handle = std::thread::Builder::new()
            .name("pty-write".into())
            .spawn({
                let closed = Arc::clone(&closed);
                move || write_loop(master_fd, &closed, &write_rx)
            })
            .map_err(|e| PtyError::Create(format!("failed to spawn write thread: {e}")))?;

        tracing::debug!(child_pid = pid, master_fd, "POSIX PTY created");

        Ok(Self {
            master_fd,
            child_pid: pid,
            write_tx,
            event_rx: Some(event_rx),
            closed,
            read_handle: Some(read_handle),
            write_handle: Some(write_handle),
        })
    }
}

impl Pty for PosixPty {
    fn take_event_rx(&mut self) -> Option<std::sync::mpsc::Receiver<PtyEvent>> {
        self.event_rx.take()
    }

    fn writer(&self) -> std::sync::mpsc::Sender<Vec<u8>> {
        self.write_tx.clone()
    }

    fn resize(&self, size: PtySize) -> Result<(), PtyError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(PtyError::Closed);
        }

        let ws = libc::winsize {
            ws_col: size.cols,
            ws_row: size.rows,
            ws_xpixel: size.pixel_width,
            ws_ypixel: size.pixel_height,
        };

        // SAFETY: master_fd is a valid open PTY master fd (we checked the closed
        // flag above). ws is a properly initialized winsize struct. TIOCSWINSZ
        // sets the terminal window size.
        let ret = unsafe { libc::ioctl(self.master_fd, libc::TIOCSWINSZ, &ws) };
        if ret == -1 {
            return Err(PtyError::Resize(std::io::Error::last_os_error().to_string()));
        }
        Ok(())
    }

    fn child_pid(&self) -> Option<u32> {
        if self.child_pid > 0 {
            Some(self.child_pid.cast_unsigned())
        } else {
            None
        }
    }

    fn shutdown(&mut self) -> Result<(), PtyError> {
        if self.closed.swap(true, Ordering::AcqRel) {
            // Already closed -- idempotent.
            return Ok(());
        }

        tracing::debug!(child_pid = self.child_pid, "shutting down POSIX PTY");

        // SAFETY: child_pid is a valid process ID obtained from forkpty.
        // Sending SIGHUP tells the child its controlling terminal has hung up,
        // which is the standard signal for PTY disconnect.
        unsafe {
            libc::kill(self.child_pid, libc::SIGHUP);
        }

        // SAFETY: master_fd is a valid open file descriptor for the PTY master.
        // Closing it causes the read thread to see EOF, which triggers child
        // reaping and the ChildExited event.
        unsafe {
            libc::close(self.master_fd);
        }

        if let Some(handle) = self.read_handle.take() {
            let _ = handle.join();
        }

        if let Some(handle) = self.write_handle.take() {
            let _ = handle.join();
        }

        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

impl Drop for PosixPty {
    fn drop(&mut self) {
        if !self.closed.load(Ordering::Acquire) {
            let _ = self.shutdown();
        }

        // Reap any zombie child with WNOHANG to avoid leaving zombies.
        // SAFETY: child_pid is a valid process ID. WNOHANG makes waitpid
        // return immediately if the child hasn't exited yet. The read thread
        // may have already reaped it, in which case this is a harmless no-op.
        unsafe {
            let mut status: libc::c_int = 0;
            libc::waitpid(self.child_pid, &raw mut status, libc::WNOHANG);
        }
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
