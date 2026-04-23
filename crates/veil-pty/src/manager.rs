//! PTY manager actor -- owns all active PTY instances and dispatches commands.
//!
//! Bridges between [`AppCommand`] messages from the event loop and the [`Pty`] trait.
//! Forwards PTY events back as [`StateUpdate`] messages.

use std::collections::HashMap;

use veil_core::lifecycle::ShutdownHandle;
use veil_core::message::{AppCommand, StateUpdate};
use veil_core::workspace::SurfaceId;

use crate::error::PtyError;
use crate::types::{PtyConfig, PtyEvent, PtySize};
use crate::Pty;

/// Factory function type for creating PTY instances.
type PtyFactory = Box<dyn Fn(PtyConfig) -> Result<Box<dyn Pty>, PtyError> + Send>;

/// A PTY instance with its bridge thread handle.
struct ManagedPty {
    /// The PTY trait object.
    pty: Box<dyn Pty>,
    /// Thread handle for the event bridge task.
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
    state_tx: tokio::sync::mpsc::Sender<StateUpdate>,
    /// For observing application shutdown.
    #[allow(dead_code)] // Will be used when the manager runs as an async actor
    shutdown: ShutdownHandle,
    /// Factory function for creating PTY instances (allows injection for testing).
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

    /// Start a bridge thread that forwards [`PtyEvent`]s from a PTY's event
    /// channel to [`StateUpdate`] messages on the state channel.
    ///
    /// Returns `None` if the PTY's event receiver has already been taken.
    fn start_bridge(
        pty: &mut Box<dyn Pty>,
        surface_id: SurfaceId,
        state_tx: &tokio::sync::mpsc::Sender<StateUpdate>,
    ) -> Option<std::thread::JoinHandle<()>> {
        let rx = pty.take_event_rx()?;
        let state_tx = state_tx.clone();
        Some(
            std::thread::Builder::new()
                .name(format!("pty-bridge-{surface_id:?}"))
                .spawn(move || Self::bridge_loop(&rx, surface_id, &state_tx))
                .expect("failed to spawn bridge thread"),
        )
    }

    /// Event bridge loop: reads from the PTY event channel and forwards
    /// `ChildExited` events as `StateUpdate::SurfaceExited`.
    fn bridge_loop(
        rx: &std::sync::mpsc::Receiver<PtyEvent>,
        surface_id: SurfaceId,
        state_tx: &tokio::sync::mpsc::Sender<StateUpdate>,
    ) {
        while let Ok(event) = rx.recv() {
            match event {
                PtyEvent::ChildExited { exit_code } => {
                    tracing::debug!(?surface_id, ?exit_code, "child exited, forwarding");
                    let update = StateUpdate::SurfaceExited { surface_id, exit_code };
                    if let Err(e) = state_tx.blocking_send(update) {
                        tracing::warn!(?surface_id, "failed to forward SurfaceExited: {e}");
                    }
                    break;
                }
                PtyEvent::Output(data) => {
                    let update = StateUpdate::PtyOutput { surface_id, data };
                    if let Err(e) = state_tx.blocking_send(update) {
                        tracing::warn!(?surface_id, "failed to forward PtyOutput: {e}");
                    }
                }
            }
        }
    }

    /// Spawn a new PTY for the given surface.
    pub fn spawn(&mut self, surface_id: SurfaceId, config: PtyConfig) -> Result<(), PtyError> {
        if self.ptys.contains_key(&surface_id) {
            return Err(PtyError::Create("surface already exists".to_string()));
        }

        let mut pty = (self.pty_factory)(config)?;
        let bridge_handle = Self::start_bridge(&mut pty, surface_id, &self.state_tx);
        self.ptys.insert(surface_id, ManagedPty { pty, bridge_handle });
        tracing::debug!(?surface_id, count = self.ptys.len(), "spawned PTY");
        Ok(())
    }

    /// Write bytes to an existing surface's PTY.
    pub fn write(&self, surface_id: SurfaceId, data: Vec<u8>) -> Result<(), PtyError> {
        let managed = self.ptys.get(&surface_id).ok_or(PtyError::Closed)?;
        let writer = managed.pty.writer();
        if writer.send(data).is_err() {
            tracing::warn!(?surface_id, "write channel disconnected, data dropped");
        }
        Ok(())
    }

    /// Resize an existing surface's PTY.
    pub fn resize(&self, surface_id: SurfaceId, size: PtySize) -> Result<(), PtyError> {
        let managed = self.ptys.get(&surface_id).ok_or(PtyError::Closed)?;
        managed.pty.resize(size)
    }

    /// Close and remove a surface's PTY.
    pub fn close(&mut self, surface_id: SurfaceId) -> Result<(), PtyError> {
        let mut managed = self.ptys.remove(&surface_id).ok_or(PtyError::Closed)?;
        managed.pty.shutdown()?;
        if let Some(handle) = managed.bridge_handle.take() {
            let _ = handle.join();
        }
        tracing::debug!(?surface_id, count = self.ptys.len(), "closed PTY");
        Ok(())
    }

    /// Shut down all active PTYs (called on application exit).
    pub fn shutdown_all(&mut self) {
        let count = self.ptys.len();
        if count == 0 {
            return;
        }
        tracing::debug!(count, "shutting down all PTYs");
        for (sid, mut managed) in self.ptys.drain() {
            if let Err(e) = managed.pty.shutdown() {
                tracing::warn!(?sid, "failed to shutdown PTY: {e}");
            }
            if let Some(handle) = managed.bridge_handle.take() {
                let _ = handle.join();
            }
        }
    }

    /// Dispatch an [`AppCommand`] to the appropriate method.
    pub fn handle_command(&mut self, cmd: AppCommand) {
        match cmd {
            AppCommand::SpawnSurface { surface_id, working_directory } => {
                let config = PtyConfig {
                    command: None,
                    args: vec![],
                    working_directory: Some(working_directory),
                    env: vec![],
                    size: PtySize::default(),
                };
                if let Err(e) = self.spawn(surface_id, config) {
                    tracing::warn!(?surface_id, "failed to spawn surface: {e}");
                }
            }
            AppCommand::SendInput { surface_id, data } => {
                if let Err(e) = self.write(surface_id, data) {
                    tracing::warn!(?surface_id, "failed to send input: {e}");
                }
            }
            AppCommand::ResizeSurface { surface_id, cols, rows } => {
                let size = PtySize { cols, rows, pixel_width: 0, pixel_height: 0 };
                if let Err(e) = self.resize(surface_id, size) {
                    tracing::warn!(?surface_id, "failed to resize surface: {e}");
                }
            }
            AppCommand::CloseSurface { surface_id } => {
                if let Err(e) = self.close(surface_id) {
                    tracing::warn!(?surface_id, "failed to close surface: {e}");
                }
            }
            AppCommand::Shutdown | AppCommand::RefreshConversations => {}
        }
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
        event_rx: Option<std::sync::mpsc::Receiver<PtyEvent>>,
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
    struct MockPtyHandles {
        event_tx: std::sync::mpsc::Sender<PtyEvent>,
        #[allow(dead_code)] // Available for future write-verification tests
        write_rx: std::sync::mpsc::Receiver<Vec<u8>>,
        #[allow(dead_code)]
        closed: Arc<AtomicBool>,
        #[allow(dead_code)]
        shutdown_called: Arc<AtomicBool>,
        #[allow(dead_code)]
        resize_called: Arc<AtomicBool>,
        #[allow(dead_code)]
        last_resize: Arc<std::sync::Mutex<Option<PtySize>>>,
    }

    impl Pty for MockPty {
        fn take_event_rx(&mut self) -> Option<std::sync::mpsc::Receiver<PtyEvent>> {
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
        let (mut manager, _rx, _signal) = make_test_manager();
        manager.spawn(SurfaceId::new(1), default_pty_config()).expect("spawn");
        // The Shutdown command is handled at the app level, not by PtyManager.
        manager.handle_command(AppCommand::Shutdown);
        // PTYs remain active -- the app calls shutdown_all separately.
        assert_eq!(manager.active_count(), 1);
    }

    // --- Event bridging ---

    #[tokio::test]
    async fn child_exit_sends_surface_exited_state_update() {
        let signal = ShutdownSignal::new();
        let (state_tx, mut state_rx) = tokio::sync::mpsc::channel(64);
        let surface_id = SurfaceId::new(42);

        // Create a mock PTY whose event channel we control from the test side.
        let (mock, handles) = MockPty::new();

        // Use a factory that returns this specific mock.
        let mock_cell = std::sync::Mutex::new(Some(mock));
        let mut manager = PtyManager::with_factory(
            state_tx,
            signal.handle(),
            Box::new(move |_config| {
                let pty = mock_cell.lock().unwrap().take().expect("factory called more than once");
                Ok(Box::new(pty) as Box<dyn Pty>)
            }),
        );

        // Spawn sets up the bridge thread that reads from the mock's event channel.
        manager.spawn(surface_id, default_pty_config()).expect("spawn should succeed");

        // Simulate the child exiting by sending an event through the test handle.
        handles
            .event_tx
            .send(PtyEvent::ChildExited { exit_code: Some(0) })
            .expect("send should succeed");

        // The bridge thread should forward this as a StateUpdate.
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

    // --- Event bridging: PtyOutput forwarding ---

    #[tokio::test]
    async fn pty_output_sends_pty_output_state_update() {
        let signal = ShutdownSignal::new();
        let (state_tx, mut state_rx) = tokio::sync::mpsc::channel(64);
        let surface_id = SurfaceId::new(10);

        let (mock, handles) = MockPty::new();
        let mock_cell = std::sync::Mutex::new(Some(mock));
        let mut manager = PtyManager::with_factory(
            state_tx,
            signal.handle(),
            Box::new(move |_config| {
                let pty = mock_cell.lock().unwrap().take().expect("factory called more than once");
                Ok(Box::new(pty) as Box<dyn Pty>)
            }),
        );

        manager.spawn(surface_id, default_pty_config()).expect("spawn should succeed");

        // Send output through the test handle.
        handles.event_tx.send(PtyEvent::Output(b"hello".to_vec())).expect("send should succeed");

        // Then send exit to make the bridge loop terminate.
        handles
            .event_tx
            .send(PtyEvent::ChildExited { exit_code: Some(0) })
            .expect("send should succeed");

        // The bridge thread should forward output as StateUpdate::PtyOutput.
        let update = tokio::time::timeout(std::time::Duration::from_secs(2), state_rx.recv())
            .await
            .expect("should receive state update within timeout")
            .expect("channel should not be closed");

        match update {
            StateUpdate::PtyOutput { surface_id: sid, data } => {
                assert_eq!(sid, surface_id);
                assert_eq!(data, b"hello");
            }
            other => panic!("expected PtyOutput, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn multiple_output_chunks_arrive_in_order() {
        let signal = ShutdownSignal::new();
        let (state_tx, mut state_rx) = tokio::sync::mpsc::channel(64);
        let surface_id = SurfaceId::new(20);

        let (mock, handles) = MockPty::new();
        let mock_cell = std::sync::Mutex::new(Some(mock));
        let mut manager = PtyManager::with_factory(
            state_tx,
            signal.handle(),
            Box::new(move |_config| {
                let pty = mock_cell.lock().unwrap().take().expect("factory called more than once");
                Ok(Box::new(pty) as Box<dyn Pty>)
            }),
        );

        manager.spawn(surface_id, default_pty_config()).expect("spawn should succeed");

        // Send three output chunks.
        handles.event_tx.send(PtyEvent::Output(b"chunk1".to_vec())).expect("send should succeed");
        handles.event_tx.send(PtyEvent::Output(b"chunk2".to_vec())).expect("send should succeed");
        handles.event_tx.send(PtyEvent::Output(b"chunk3".to_vec())).expect("send should succeed");
        handles
            .event_tx
            .send(PtyEvent::ChildExited { exit_code: Some(0) })
            .expect("send should succeed");

        let mut received_data: Vec<Vec<u8>> = Vec::new();
        let timeout = std::time::Duration::from_secs(2);
        // Drain all PtyOutput messages (expect 3, then SurfaceExited).
        loop {
            let update = tokio::time::timeout(timeout, state_rx.recv())
                .await
                .expect("should receive within timeout")
                .expect("channel should not be closed");
            match update {
                StateUpdate::PtyOutput { data, .. } => received_data.push(data),
                StateUpdate::SurfaceExited { .. } => break,
                other => panic!("unexpected state update: {other:?}"),
            }
        }

        assert_eq!(received_data.len(), 3, "should receive all 3 output chunks");
        assert_eq!(received_data[0], b"chunk1");
        assert_eq!(received_data[1], b"chunk2");
        assert_eq!(received_data[2], b"chunk3");
    }

    #[tokio::test]
    async fn output_followed_by_exit_arrives_in_order() {
        let signal = ShutdownSignal::new();
        let (state_tx, mut state_rx) = tokio::sync::mpsc::channel(64);
        let surface_id = SurfaceId::new(30);

        let (mock, handles) = MockPty::new();
        let mock_cell = std::sync::Mutex::new(Some(mock));
        let mut manager = PtyManager::with_factory(
            state_tx,
            signal.handle(),
            Box::new(move |_config| {
                let pty = mock_cell.lock().unwrap().take().expect("factory called more than once");
                Ok(Box::new(pty) as Box<dyn Pty>)
            }),
        );

        manager.spawn(surface_id, default_pty_config()).expect("spawn should succeed");

        // Send output then exit.
        handles
            .event_tx
            .send(PtyEvent::Output(b"final output".to_vec()))
            .expect("send should succeed");
        handles
            .event_tx
            .send(PtyEvent::ChildExited { exit_code: Some(42) })
            .expect("send should succeed");

        let timeout = std::time::Duration::from_secs(2);

        // First message should be PtyOutput.
        let first = tokio::time::timeout(timeout, state_rx.recv())
            .await
            .expect("should receive within timeout")
            .expect("channel should not be closed");
        match first {
            StateUpdate::PtyOutput { surface_id: sid, data } => {
                assert_eq!(sid, surface_id);
                assert_eq!(data, b"final output");
            }
            other => panic!("expected PtyOutput first, got: {other:?}"),
        }

        // Second message should be SurfaceExited.
        let second = tokio::time::timeout(timeout, state_rx.recv())
            .await
            .expect("should receive within timeout")
            .expect("channel should not be closed");
        match second {
            StateUpdate::SurfaceExited { surface_id: sid, exit_code } => {
                assert_eq!(sid, surface_id);
                assert_eq!(exit_code, Some(42));
            }
            other => panic!("expected SurfaceExited second, got: {other:?}"),
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
