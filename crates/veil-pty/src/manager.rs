//! PTY manager actor -- owns all active PTY instances and dispatches commands.
//!
//! Bridges between [`AppCommand`] messages from the event loop and the [`Pty`] trait.
//! Forwards PTY events back as [`StateUpdate`] messages.

use std::collections::HashMap;

use veil_core::lifecycle::ShutdownHandle;
use veil_core::message::{AppCommand, StateUpdate};
use veil_core::workspace::SurfaceId;

use crate::error::PtyError;
use crate::types::{PtyConfig, PtySize};
use crate::Pty;

/// Factory function type for creating PTY instances.
type PtyFactory = Box<dyn Fn(PtyConfig) -> Result<Box<dyn Pty>, PtyError> + Send>;

/// A PTY instance with its associated metadata.
struct ManagedPty {
    /// The PTY trait object.
    #[allow(dead_code)]
    pty: Box<dyn Pty>,
    /// The surface this PTY belongs to.
    #[allow(dead_code)]
    surface_id: SurfaceId,
    /// Thread handle for the event bridge task.
    #[allow(dead_code)]
    bridge_handle: Option<std::thread::JoinHandle<()>>,
}

/// Manages the lifecycle of all active PTY instances.
///
/// Runs as a background actor. Receives [`AppCommand`] messages from the event
/// loop and translates them into [`Pty`] trait calls. Forwards PTY events
/// (output, child exit) back to the event loop via [`StateUpdate`] channel.
pub struct PtyManager {
    /// Active PTY instances, keyed by `SurfaceId`.
    ptys: HashMap<SurfaceId, ManagedPty>,
    /// For sending state updates back to the event loop.
    #[allow(dead_code)]
    state_tx: tokio::sync::mpsc::Sender<StateUpdate>,
    /// For observing application shutdown.
    #[allow(dead_code)]
    shutdown: ShutdownHandle,
    /// Factory function for creating PTY instances (allows injection for testing).
    #[allow(dead_code)]
    pty_factory: PtyFactory,
}

impl PtyManager {
    /// Create a new `PtyManager` with no active PTYs.
    pub fn new(state_tx: tokio::sync::mpsc::Sender<StateUpdate>, shutdown: ShutdownHandle) -> Self {
        Self { ptys: HashMap::new(), state_tx, shutdown, pty_factory: Box::new(crate::create_pty) }
    }

    /// Create a `PtyManager` with a custom PTY factory (for testing).
    #[cfg(test)]
    fn with_factory(
        state_tx: tokio::sync::mpsc::Sender<StateUpdate>,
        shutdown: ShutdownHandle,
        factory: PtyFactory,
    ) -> Self {
        Self { ptys: HashMap::new(), state_tx, shutdown, pty_factory: factory }
    }

    /// Spawn a new PTY for the given surface.
    #[allow(clippy::needless_pass_by_value)] // Will consume config in implementation
    pub fn spawn(&mut self, surface_id: SurfaceId, config: PtyConfig) -> Result<(), PtyError> {
        todo!(
            "PtyManager::spawn — create PTY, start event bridge, insert into ptys map. \
             surface_id={surface_id:?}, config={config:?}"
        )
    }

    /// Write bytes to an existing surface's PTY.
    #[allow(clippy::needless_pass_by_value)] // Will forward data to channel in implementation
    pub fn write(&self, surface_id: SurfaceId, data: Vec<u8>) -> Result<(), PtyError> {
        todo!(
            "PtyManager::write — look up surface_id={surface_id:?}, send data ({} bytes)",
            data.len()
        )
    }

    /// Resize an existing surface's PTY.
    pub fn resize(&self, surface_id: SurfaceId, size: PtySize) -> Result<(), PtyError> {
        todo!("PtyManager::resize — look up surface_id={surface_id:?}, resize to {size:?}")
    }

    /// Close and remove a surface's PTY.
    pub fn close(&mut self, surface_id: SurfaceId) -> Result<(), PtyError> {
        todo!("PtyManager::close — look up surface_id={surface_id:?}, shutdown and remove")
    }

    /// Shut down all active PTYs (called on application exit).
    pub fn shutdown_all(&mut self) {
        todo!("PtyManager::shutdown_all — iterate and shutdown each PTY")
    }

    /// Dispatch an [`AppCommand`] to the appropriate method.
    #[allow(clippy::needless_pass_by_value)] // Will destructure cmd in implementation
    pub fn handle_command(&mut self, cmd: AppCommand) {
        todo!("PtyManager::handle_command — match on cmd={cmd:?} and dispatch")
    }

    /// Returns the number of active PTYs.
    pub fn active_count(&self) -> usize {
        self.ptys.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use veil_core::lifecycle::ShutdownSignal;

    // --- MockPty ---

    /// A mock PTY for testing the manager's dispatch logic.
    struct MockPty {
        event_rx: Option<std::sync::mpsc::Receiver<crate::types::PtyEvent>>,
        write_tx: std::sync::mpsc::Sender<Vec<u8>>,
        closed: Arc<AtomicBool>,
        shutdown_called: Arc<AtomicBool>,
        resize_called: Arc<AtomicBool>,
        last_resize: Arc<std::sync::Mutex<Option<PtySize>>>,
    }

    impl MockPty {
        fn new() -> (Self, MockPtyHandles) {
            let (event_tx, event_rx) = std::sync::mpsc::channel();
            let (write_tx, write_rx) = std::sync::mpsc::channel();
            let closed = Arc::new(AtomicBool::new(false));
            let shutdown_called = Arc::new(AtomicBool::new(false));
            let resize_called = Arc::new(AtomicBool::new(false));
            let last_resize = Arc::new(std::sync::Mutex::new(None));

            let mock = Self {
                event_rx: Some(event_rx),
                write_tx,
                closed: Arc::clone(&closed),
                shutdown_called: Arc::clone(&shutdown_called),
                resize_called: Arc::clone(&resize_called),
                last_resize: Arc::clone(&last_resize),
            };

            let handles = MockPtyHandles {
                event_tx,
                write_rx,
                closed,
                shutdown_called,
                resize_called,
                last_resize,
            };

            (mock, handles)
        }
    }

    /// Test-side handles for observing and controlling a `MockPty`.
    #[allow(dead_code)]
    struct MockPtyHandles {
        event_tx: std::sync::mpsc::Sender<crate::types::PtyEvent>,
        write_rx: std::sync::mpsc::Receiver<Vec<u8>>,
        closed: Arc<AtomicBool>,
        shutdown_called: Arc<AtomicBool>,
        resize_called: Arc<AtomicBool>,
        last_resize: Arc<std::sync::Mutex<Option<PtySize>>>,
    }

    impl Pty for MockPty {
        fn take_event_rx(&mut self) -> Option<std::sync::mpsc::Receiver<crate::types::PtyEvent>> {
            self.event_rx.take()
        }

        fn writer(&self) -> std::sync::mpsc::Sender<Vec<u8>> {
            self.write_tx.clone()
        }

        fn resize(&self, size: PtySize) -> Result<(), PtyError> {
            self.resize_called.store(true, Ordering::Release);
            *self.last_resize.lock().unwrap() = Some(size);
            Ok(())
        }

        fn child_pid(&self) -> Option<u32> {
            Some(12345)
        }

        fn shutdown(&mut self) -> Result<(), PtyError> {
            self.shutdown_called.store(true, Ordering::Release);
            self.closed.store(true, Ordering::Release);
            Ok(())
        }

        fn is_closed(&self) -> bool {
            self.closed.load(Ordering::Acquire)
        }
    }

    fn make_test_manager() -> (PtyManager, tokio::sync::mpsc::Receiver<StateUpdate>, ShutdownSignal)
    {
        let signal = ShutdownSignal::new();
        let (state_tx, state_rx) = tokio::sync::mpsc::channel(64);
        let manager = PtyManager::with_factory(
            state_tx,
            signal.handle(),
            Box::new(|_config| {
                let (mock, _handles) = MockPty::new();
                Ok(Box::new(mock) as Box<dyn Pty>)
            }),
        );
        (manager, state_rx, signal)
    }

    fn default_pty_config() -> PtyConfig {
        PtyConfig {
            command: Some("/bin/sh".to_string()),
            args: vec![],
            working_directory: Some(PathBuf::from("/tmp")),
            env: vec![],
            size: PtySize::default(),
        }
    }

    // --- PtyManager::new ---

    #[test]
    fn new_manager_has_no_active_ptys() {
        let (manager, _rx, _signal) = make_test_manager();
        assert_eq!(manager.active_count(), 0);
    }

    // --- PtyManager::spawn ---

    #[test]
    fn spawn_adds_pty_to_active_set() {
        let (mut manager, _rx, _signal) = make_test_manager();
        let surface_id = SurfaceId::new(1);
        let config = default_pty_config();

        manager.spawn(surface_id, config).expect("spawn should succeed");

        assert_eq!(manager.active_count(), 1);
    }

    #[test]
    fn spawn_duplicate_surface_id_returns_error() {
        let (mut manager, _rx, _signal) = make_test_manager();
        let surface_id = SurfaceId::new(1);

        manager.spawn(surface_id, default_pty_config()).expect("first spawn should succeed");

        let result = manager.spawn(surface_id, default_pty_config());
        assert!(result.is_err(), "spawning duplicate surface_id should return an error");
    }

    #[test]
    fn spawn_multiple_surfaces() {
        let (mut manager, _rx, _signal) = make_test_manager();

        manager.spawn(SurfaceId::new(1), default_pty_config()).expect("spawn 1 should succeed");
        manager.spawn(SurfaceId::new(2), default_pty_config()).expect("spawn 2 should succeed");
        manager.spawn(SurfaceId::new(3), default_pty_config()).expect("spawn 3 should succeed");

        assert_eq!(manager.active_count(), 3);
    }

    // --- PtyManager::write ---

    #[test]
    fn write_to_existing_surface_succeeds() {
        let (mut manager, _rx, _signal) = make_test_manager();
        let surface_id = SurfaceId::new(1);

        manager.spawn(surface_id, default_pty_config()).expect("spawn should succeed");

        let result = manager.write(surface_id, b"hello".to_vec());
        assert!(result.is_ok(), "write to existing surface should succeed");
    }

    #[test]
    fn write_to_nonexistent_surface_returns_closed_error() {
        let (manager, _rx, _signal) = make_test_manager();
        let result = manager.write(SurfaceId::new(999), b"hello".to_vec());
        assert!(result.is_err(), "write to nonexistent surface should fail");
        match result.unwrap_err() {
            PtyError::Closed => {}
            other => panic!("expected PtyError::Closed, got: {other:?}"),
        }
    }

    // --- PtyManager::resize ---

    #[test]
    fn resize_existing_surface_succeeds() {
        let (mut manager, _rx, _signal) = make_test_manager();
        let surface_id = SurfaceId::new(1);

        manager.spawn(surface_id, default_pty_config()).expect("spawn should succeed");

        let result = manager
            .resize(surface_id, PtySize { cols: 132, rows: 43, pixel_width: 0, pixel_height: 0 });
        assert!(result.is_ok(), "resize existing surface should succeed");
    }

    #[test]
    fn resize_nonexistent_surface_returns_error() {
        let (manager, _rx, _signal) = make_test_manager();
        let result = manager.resize(SurfaceId::new(999), PtySize::default());
        assert!(result.is_err(), "resize nonexistent surface should fail");
    }

    // --- PtyManager::close ---

    #[test]
    fn close_removes_pty_from_active_set() {
        let (mut manager, _rx, _signal) = make_test_manager();
        let surface_id = SurfaceId::new(1);

        manager.spawn(surface_id, default_pty_config()).expect("spawn should succeed");
        assert_eq!(manager.active_count(), 1);

        manager.close(surface_id).expect("close should succeed");
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn close_nonexistent_surface_returns_error() {
        let (mut manager, _rx, _signal) = make_test_manager();
        let result = manager.close(SurfaceId::new(999));
        assert!(result.is_err(), "close nonexistent surface should fail");
    }

    // --- PtyManager::shutdown_all ---

    #[test]
    fn shutdown_all_closes_all_ptys() {
        let (mut manager, _rx, _signal) = make_test_manager();

        manager.spawn(SurfaceId::new(1), default_pty_config()).expect("spawn 1");
        manager.spawn(SurfaceId::new(2), default_pty_config()).expect("spawn 2");
        assert_eq!(manager.active_count(), 2);

        manager.shutdown_all();
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn shutdown_all_on_empty_manager_is_noop() {
        let (mut manager, _rx, _signal) = make_test_manager();
        assert_eq!(manager.active_count(), 0);
        manager.shutdown_all(); // Should not panic
        assert_eq!(manager.active_count(), 0);
    }

    // --- PtyManager::handle_command ---

    #[test]
    fn handle_command_dispatches_spawn_surface() {
        let (mut manager, _rx, _signal) = make_test_manager();
        let surface_id = SurfaceId::new(10);

        manager.handle_command(AppCommand::SpawnSurface {
            surface_id,
            working_directory: PathBuf::from("/tmp"),
        });

        assert_eq!(manager.active_count(), 1);
    }

    #[test]
    fn handle_command_dispatches_send_input() {
        let (mut manager, _rx, _signal) = make_test_manager();
        let surface_id = SurfaceId::new(1);

        manager.spawn(surface_id, default_pty_config()).expect("spawn");

        // Should not panic
        manager.handle_command(AppCommand::SendInput { surface_id, data: b"ls\n".to_vec() });
    }

    #[test]
    fn handle_command_dispatches_resize_surface() {
        let (mut manager, _rx, _signal) = make_test_manager();
        let surface_id = SurfaceId::new(1);

        manager.spawn(surface_id, default_pty_config()).expect("spawn");

        // Should not panic
        manager.handle_command(AppCommand::ResizeSurface { surface_id, cols: 100, rows: 50 });
    }

    #[test]
    fn handle_command_dispatches_close_surface() {
        let (mut manager, _rx, _signal) = make_test_manager();
        let surface_id = SurfaceId::new(1);

        manager.spawn(surface_id, default_pty_config()).expect("spawn");
        assert_eq!(manager.active_count(), 1);

        manager.handle_command(AppCommand::CloseSurface { surface_id });
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn handle_command_ignores_refresh_conversations() {
        let (mut manager, _rx, _signal) = make_test_manager();
        // Should not panic or change state
        manager.handle_command(AppCommand::RefreshConversations);
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn handle_command_ignores_shutdown_command() {
        // The Shutdown command is handled at the app level, not by PtyManager
        let (mut manager, _rx, _signal) = make_test_manager();
        manager.spawn(SurfaceId::new(1), default_pty_config()).expect("spawn");
        // handle_command for Shutdown should not panic
        manager.handle_command(AppCommand::Shutdown);
        // PTY manager doesn't handle Shutdown directly (the app calls shutdown_all)
    }

    // --- Event bridging ---

    #[tokio::test]
    async fn child_exit_sends_surface_exited_state_update() {
        let signal = ShutdownSignal::new();
        let (state_tx, mut state_rx) = tokio::sync::mpsc::channel(64);
        let surface_id = SurfaceId::new(42);

        // Create a mock PTY whose event channel we control
        let (mock, handles) = MockPty::new();

        let mut manager = PtyManager::with_factory(
            state_tx,
            signal.handle(),
            Box::new(|_config| {
                // This factory won't be called -- we insert the mock directly
                unreachable!("factory should not be called in this test")
            }),
        );

        // Manually insert the mock PTY
        manager.ptys.insert(
            surface_id,
            ManagedPty { pty: Box::new(mock), surface_id, bridge_handle: None },
        );

        // After spawn() is implemented, it starts a bridge thread.
        // For now, we're testing the intent: when a ChildExited event
        // comes from a PTY, the manager should forward it as StateUpdate::SurfaceExited.
        //
        // This test calls spawn() which will start the bridge. Since spawn()
        // is unimplemented (todo!), this test will fail, which is correct RED state.
        //
        // Instead, we simulate what spawn's bridge would do: send ChildExited
        // on the event channel and verify the state_tx receives SurfaceExited.

        // Send a ChildExited event from the "PTY"
        handles
            .event_tx
            .send(crate::types::PtyEvent::ChildExited { exit_code: Some(0) })
            .expect("send should succeed");

        // In the real implementation, a bridge thread reads from event_rx
        // and forwards to state_tx. Since that bridge is part of spawn() (which
        // is todo!()), we test that the manager has the right state_tx and
        // can forward by calling spawn which will fail.

        // To make this test actually test the bridge, we call spawn which
        // should set up the bridge thread. This will fail (todo!) -- RED state.
        let result = manager.spawn(
            SurfaceId::new(100),
            PtyConfig {
                command: Some("/bin/true".to_string()),
                args: vec![],
                working_directory: Some(PathBuf::from("/tmp")),
                env: vec![],
                size: PtySize::default(),
            },
        );
        // spawn is todo!(), so this will panic. We expect this test to fail.
        assert!(result.is_ok(), "spawn should succeed to set up bridge");

        // Wait for the bridge to forward the event
        let update = tokio::time::timeout(std::time::Duration::from_secs(2), state_rx.recv())
            .await
            .expect("should receive state update within timeout")
            .expect("channel should not be closed");

        match update {
            StateUpdate::SurfaceExited { surface_id: sid, exit_code } => {
                assert_eq!(sid, surface_id);
                assert_eq!(exit_code, Some(0));
            }
            other => panic!("expected SurfaceExited, got: {other:?}"),
        }
    }

    // --- Pty trait is object-safe ---

    #[test]
    fn pty_trait_is_object_safe() {
        // This test verifies the Pty trait can be used as Box<dyn Pty>.
        // If it compiles, the trait is object-safe.
        let (mock, _handles) = MockPty::new();
        let boxed: Box<dyn Pty> = Box::new(mock);
        assert!(boxed.child_pid().is_some());
    }
}
