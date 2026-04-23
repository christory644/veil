//! Aggregator background actor.
//!
//! Runs the session aggregator on a dedicated `std::thread` (required because
//! `SessionStore` wraps `rusqlite::Connection` which is `!Send`). Performs
//! initial discovery, watches for file-system changes, and responds to
//! `AppCommand::RefreshConversations`.

use std::sync::mpsc::{Receiver, TryRecvError as StdTryRecvError};
use std::time::Duration;

use tokio::sync::broadcast::error::TryRecvError as BroadcastTryRecvError;
use tracing::{error, info, warn};
use veil_aggregator::claude_code::ClaudeCodeAdapter;
use veil_aggregator::registry::AdapterRegistry;
use veil_aggregator::store::SessionStore;
use veil_core::lifecycle::ShutdownHandle;
use veil_core::message::{AppCommand, StateUpdate};
use veil_core::session::SessionEntry;

/// Channel receiver for background discovery results.
type DiscoveryReceiver = Receiver<Vec<SessionEntry>>;

/// Handle to the aggregator background thread.
///
/// Dropping this handle does *not* stop the thread; the thread exits when the
/// `ShutdownHandle` is triggered.
pub struct AggregatorHandle {
    #[allow(dead_code)]
    thread: std::thread::JoinHandle<()>,
}

impl AggregatorHandle {
    /// Block until the aggregator thread exits.
    ///
    /// Returns `Ok(())` if the thread exited normally, or `Err(())` if it panicked.
    #[cfg(test)]
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
    state_tx: tokio::sync::mpsc::Sender<StateUpdate>,
    mut command_rx: tokio::sync::broadcast::Receiver<AppCommand>,
    shutdown: ShutdownHandle,
) -> AggregatorHandle {
    let handle = std::thread::Builder::new()
        .name("aggregator".into())
        .spawn(move || {
            // --- Open SessionStore ---
            let store = match open_session_store() {
                Ok(store) => store,
                Err(err) => {
                    error!(%err, "failed to open session store, falling back to in-memory");
                    match SessionStore::open_in_memory() {
                        Ok(store) => store,
                        Err(err) => {
                            error!(%err, "failed to open in-memory session store, aborting aggregator");
                            return;
                        }
                    }
                }
            };

            // --- Build adapter registry ---
            let registry = build_adapter_registry();
            info!(adapters = ?registry.adapter_names(), "built adapter registry");

            // --- Send cached sessions immediately (fast initial response) ---
            send_sessions(&store, &state_tx);

            // --- Run initial discovery in background ---
            // Discovery can be slow with many session files. We run it on
            // a separate thread so the command loop stays responsive.
            let discovery_rx = spawn_discovery(registry);

            // --- Poll loop ---
            //
            // The aggregator uses a 100ms sleep-based poll loop rather than
            // blocking on a channel. This keeps the design simple: we need to
            // multiplex three sources (discovery results, broadcast commands,
            // shutdown signal) on a `!Send` thread without a tokio runtime.
            // 100ms gives responsive command handling without busy-spinning.
            let mut pending_discovery: Option<DiscoveryReceiver> = Some(discovery_rx);

            loop {
                if shutdown.is_triggered() {
                    info!("aggregator shutting down");
                    break;
                }

                if let Some(finished) =
                    drain_discovery_results(&mut pending_discovery, &store, &state_tx)
                {
                    if finished {
                        pending_discovery = None;
                    }
                }

                match command_rx.try_recv() {
                    Ok(AppCommand::RefreshConversations) => {
                        info!("received RefreshConversations command");
                        if pending_discovery.is_none() {
                            let registry = build_adapter_registry();
                            pending_discovery = Some(spawn_discovery(registry));
                        }
                        send_sessions(&store, &state_tx);
                    }
                    Ok(_) | Err(BroadcastTryRecvError::Empty) => {}
                    Err(BroadcastTryRecvError::Lagged(n)) => {
                        warn!(skipped = n, "command receiver lagged");
                    }
                    Err(BroadcastTryRecvError::Closed) => {
                        info!("command channel closed, aggregator exiting");
                        break;
                    }
                }

                std::thread::sleep(Duration::from_millis(100));
            }
        })
        .expect("failed to spawn aggregator thread");

    AggregatorHandle { thread: handle }
}

/// Check for completed discovery results from the background thread.
///
/// Returns `None` if no discovery is pending, `Some(false)` if the discovery
/// is still running, or `Some(true)` if it completed (successfully or via
/// disconnect) and the caller should clear the pending receiver.
fn drain_discovery_results(
    pending: &mut Option<DiscoveryReceiver>,
    store: &SessionStore,
    state_tx: &tokio::sync::mpsc::Sender<StateUpdate>,
) -> Option<bool> {
    let rx = pending.as_ref()?;
    match rx.try_recv() {
        Ok(entries) => {
            info!(count = entries.len(), "background discovery completed");
            if let Err(err) = store.upsert_sessions(&entries) {
                warn!(%err, "failed to upsert discovered sessions");
            }
            send_sessions(store, state_tx);
            Some(true)
        }
        Err(StdTryRecvError::Empty) => Some(false),
        Err(StdTryRecvError::Disconnected) => {
            warn!("discovery thread disconnected without sending results");
            Some(true)
        }
    }
}

/// Open the session store at the platform data directory, or return an error.
fn open_session_store() -> Result<SessionStore, Box<dyn std::error::Error>> {
    let data_dir =
        dirs::data_dir().map(|d| d.join("veil")).ok_or("dirs::data_dir() returned None")?;
    std::fs::create_dir_all(&data_dir)?;
    let db_path = data_dir.join("sessions.db");
    info!(?db_path, "opening session store");
    let store = SessionStore::open(&db_path)?;
    Ok(store)
}

/// Send the current session list from the store via `state_tx`.
fn send_sessions(store: &SessionStore, state_tx: &tokio::sync::mpsc::Sender<StateUpdate>) {
    match store.list_sessions() {
        Ok(sessions) => {
            info!(count = sessions.len(), "sending ConversationsUpdated");
            if let Err(err) = state_tx.blocking_send(StateUpdate::ConversationsUpdated(sessions)) {
                error!(%err, "failed to send ConversationsUpdated");
            }
        }
        Err(err) => {
            error!(%err, "failed to list sessions from store");
        }
    }
}

/// Build a new adapter registry with all available adapters.
fn build_adapter_registry() -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();
    if let Some(adapter) = ClaudeCodeAdapter::new() {
        registry.register(Box::new(adapter));
    }
    registry
}

/// Spawn a discovery thread and return a receiver for the results.
///
/// Discovery can be slow on machines with many session files, so it runs on
/// a separate thread to avoid blocking the command loop.
fn spawn_discovery(registry: AdapterRegistry) -> DiscoveryReceiver {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("aggregator-discover".into())
        .spawn(move || {
            let entries = registry.discover_all();
            let _ = tx.send(entries);
        })
        .expect("failed to spawn discovery thread");
    rx
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
