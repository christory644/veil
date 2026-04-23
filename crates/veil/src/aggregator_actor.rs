//! Aggregator background actor.
//!
//! Runs the session aggregator on a dedicated `std::thread` (required because
//! `SessionStore` wraps `rusqlite::Connection` which is `!Send`). Performs
//! initial discovery, watches for file-system changes, and responds to
//! `AppCommand::RefreshConversations`.

use veil_core::lifecycle::ShutdownHandle;
use veil_core::message::{AppCommand, StateUpdate};

/// Handle to the aggregator background thread.
///
/// Dropping this handle does *not* stop the thread; the thread exits when the
/// `ShutdownHandle` is triggered.
pub struct AggregatorHandle {
    thread: std::thread::JoinHandle<()>,
}

impl AggregatorHandle {
    /// Block until the aggregator thread exits.
    ///
    /// Returns `Ok(())` if the thread exited normally, or `Err(())` if it panicked.
    pub fn join(self) -> Result<(), ()> {
        self.thread.join().map_err(|_| ())
    }
}

/// Start the aggregator actor on a dedicated thread.
///
/// - Opens `SessionStore` at `data_dir / "veil" / "sessions.db"` (or in-memory
///   if `dirs::data_dir()` is unavailable)
/// - Registers `ClaudeCodeAdapter` (if `~/.claude/projects/` exists)
/// - Runs initial `AdapterRegistry::discover_all()`, upserts into store, sends
///   `StateUpdate::ConversationsUpdated` with the full session list
/// - Loops: wait for file-system events or `AppCommand::RefreshConversations`,
///   re-discover, upsert, send updated session list
/// - Exits when `ShutdownHandle::is_triggered()` returns true
///
/// Returns an `AggregatorHandle` that keeps the thread alive.
pub fn start_aggregator(
    _state_tx: tokio::sync::mpsc::Sender<StateUpdate>,
    _command_rx: tokio::sync::broadcast::Receiver<AppCommand>,
    _shutdown: ShutdownHandle,
) -> AggregatorHandle {
    let handle = std::thread::spawn(move || {
        // TODO: implement aggregator actor loop
    });
    AggregatorHandle { thread: handle }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use veil_core::lifecycle::ShutdownSignal;
    use veil_core::message::Channels;

    /// Poll `state_rx` for up to `timeout`, returning the first message received.
    fn recv_timeout(
        state_rx: &mut tokio::sync::mpsc::Receiver<StateUpdate>,
        timeout: Duration,
    ) -> Option<StateUpdate> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if let Ok(update) = state_rx.try_recv() {
                return Some(update);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }

    /// Start aggregator, verify it sends at least one `ConversationsUpdated`
    /// on the state channel (even if the session list is empty).
    #[test]
    fn test_aggregator_sends_initial_sessions() {
        let channels = Channels::new(256);
        let shutdown = ShutdownSignal::new();

        let _handle = start_aggregator(
            channels.state_tx.clone(),
            channels.command_subscriber(),
            shutdown.handle(),
        );

        // The aggregator thread should send ConversationsUpdated after initial
        // discovery. Wait up to 2 seconds for the message.
        let mut state_rx = channels.state_rx;
        let received = recv_timeout(&mut state_rx, Duration::from_secs(2));

        // Clean up.
        shutdown.trigger();

        let update = received.expect("should receive at least one StateUpdate within 2 seconds");
        assert!(
            matches!(update, StateUpdate::ConversationsUpdated(_)),
            "first message should be ConversationsUpdated, got: {update:?}"
        );
    }

    /// Start aggregator, trigger shutdown, verify the thread exits within 2 seconds.
    #[test]
    fn test_aggregator_graceful_shutdown() {
        let channels = Channels::new(256);
        let shutdown = ShutdownSignal::new();

        let handle = start_aggregator(
            channels.state_tx.clone(),
            channels.command_subscriber(),
            shutdown.handle(),
        );

        // Trigger shutdown immediately.
        shutdown.trigger();

        // The thread should exit within 2 seconds.
        let join_result = handle.join();
        assert!(join_result.is_ok(), "aggregator thread should exit cleanly after shutdown");
    }

    /// Send `AppCommand::RefreshConversations` via broadcast, verify a
    /// `ConversationsUpdated` arrives on the state channel in response.
    #[test]
    fn test_aggregator_refresh_command() {
        let channels = Channels::new(256);
        let shutdown = ShutdownSignal::new();

        let _handle = start_aggregator(
            channels.state_tx.clone(),
            channels.command_subscriber(),
            shutdown.handle(),
        );

        let mut state_rx = channels.state_rx;

        // Wait for the initial ConversationsUpdated from startup discovery.
        let initial = recv_timeout(&mut state_rx, Duration::from_secs(2));
        assert!(initial.is_some(), "should receive initial ConversationsUpdated within 2 seconds");

        // Now send a RefreshConversations command.
        channels
            .command_tx
            .send(AppCommand::RefreshConversations)
            .expect("broadcast send should succeed");

        // Wait for the refresh response (with timeout).
        let refresh_result = recv_timeout(&mut state_rx, Duration::from_secs(2));

        // Clean up.
        shutdown.trigger();

        let update = refresh_result
            .expect("should receive ConversationsUpdated after RefreshConversations command");
        assert!(
            matches!(update, StateUpdate::ConversationsUpdated(_)),
            "response to RefreshConversations should be ConversationsUpdated, got: {update:?}"
        );
    }
}
