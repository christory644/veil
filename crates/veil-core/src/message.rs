//! Message channel infrastructure for actor-based communication.
//!
//! Defines the message types that flow between background actors and `AppState`,
//! plus the channel creation and wiring.

use std::path::PathBuf;

use crate::config::{AppConfig, ConfigDelta, ConfigWarning};
use crate::session::SessionEntry;
use crate::workspace::{SurfaceId, WorkspaceId};

/// Messages sent from background actors to update `AppState`.
/// The main event loop receives these and applies them to `AppState`.
#[derive(Debug)]
pub enum StateUpdate {
    /// Session aggregator discovered/updated conversations.
    ConversationsUpdated(Vec<SessionEntry>),
    /// A notification arrived (from PTY OSC, socket API, etc.).
    NotificationReceived {
        /// Which workspace.
        workspace_id: WorkspaceId,
        /// Notification message.
        message: String,
    },
    /// A terminal surface's process exited.
    SurfaceExited {
        /// Which surface exited.
        surface_id: SurfaceId,
        /// Process exit code, if available.
        exit_code: Option<i32>,
    },
    /// Config was reloaded from disk. Contains the new full config and what changed.
    ConfigReloaded {
        /// The new full config.
        config: Box<AppConfig>,
        /// What changed from the previous config.
        delta: ConfigDelta,
        /// Non-fatal warnings from validation.
        warnings: Vec<ConfigWarning>,
    },
    /// An actor encountered a non-fatal error worth surfacing.
    ActorError {
        /// Name of the actor.
        actor_name: String,
        /// Error message.
        message: String,
    },
}

/// Commands sent from the UI thread to background actors.
/// `Clone` is required by `tokio::sync::broadcast`.
#[derive(Debug, Clone)]
pub enum AppCommand {
    /// Request the aggregator to re-scan sessions.
    RefreshConversations,
    /// Send input bytes to a terminal surface's PTY.
    SendInput {
        /// Target surface.
        surface_id: SurfaceId,
        /// Input data.
        data: Vec<u8>,
    },
    /// Resize a terminal surface.
    ResizeSurface {
        /// Target surface.
        surface_id: SurfaceId,
        /// New column count.
        cols: u16,
        /// New row count.
        rows: u16,
    },
    /// Create a new PTY surface (shell spawn).
    SpawnSurface {
        /// Surface identifier.
        surface_id: SurfaceId,
        /// Working directory for the shell.
        working_directory: PathBuf,
    },
    /// Close a PTY surface.
    CloseSurface {
        /// Target surface.
        surface_id: SurfaceId,
    },
    /// Initiate graceful shutdown.
    Shutdown,
}

/// Bundle of channel endpoints for wiring actors to the main loop.
pub struct Channels {
    /// Sender for actors to push state updates.
    pub state_tx: tokio::sync::mpsc::Sender<StateUpdate>,
    /// Receiver for the main loop to consume state updates.
    pub state_rx: tokio::sync::mpsc::Receiver<StateUpdate>,
    /// Sender for the main loop to push commands to actors.
    pub command_tx: tokio::sync::broadcast::Sender<AppCommand>,
}

impl Channels {
    /// Create the channel pairs with the given buffer size.
    pub fn new(buffer_size: usize) -> Self {
        let (state_tx, state_rx) = tokio::sync::mpsc::channel(buffer_size);
        let (command_tx, _) = tokio::sync::broadcast::channel(buffer_size);
        Self { state_tx, state_rx, command_tx }
    }

    /// Create a new subscriber for the command channel.
    pub fn command_subscriber(&self) -> tokio::sync::broadcast::Receiver<AppCommand> {
        self.command_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{SurfaceId, WorkspaceId};

    // --- Channels::new ---

    #[tokio::test]
    async fn channels_new_creates_valid_pair() {
        let channels = Channels::new(16);
        // Verify we can access both ends
        let _tx = &channels.state_tx;
        let _rx = &channels.state_rx;
        let _cmd_tx = &channels.command_tx;
    }

    // --- StateUpdate round-trip ---

    #[tokio::test]
    async fn state_update_send_and_receive() {
        let channels = Channels::new(16);
        let Channels { state_tx, mut state_rx, .. } = channels;
        state_tx
            .send(StateUpdate::NotificationReceived {
                workspace_id: WorkspaceId::new(1),
                message: "hello".to_string(),
            })
            .await
            .expect("send should succeed");
        let msg = state_rx.recv().await.expect("should receive message");
        match msg {
            StateUpdate::NotificationReceived { workspace_id, message } => {
                assert_eq!(workspace_id, WorkspaceId::new(1));
                assert_eq!(message, "hello");
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    // --- AppCommand broadcast ---

    #[tokio::test]
    async fn app_command_broadcast_to_subscriber() {
        let channels = Channels::new(16);
        let mut sub = channels.command_subscriber();
        channels.command_tx.send(AppCommand::RefreshConversations).expect("send should succeed");
        let cmd = sub.recv().await.expect("should receive command");
        assert!(matches!(cmd, AppCommand::RefreshConversations));
    }

    #[tokio::test]
    async fn multiple_subscribers_receive_same_message() {
        let channels = Channels::new(16);
        let mut sub1 = channels.command_subscriber();
        let mut sub2 = channels.command_subscriber();
        channels.command_tx.send(AppCommand::Shutdown).expect("send should succeed");
        let cmd1 = sub1.recv().await.expect("sub1 should receive");
        let cmd2 = sub2.recv().await.expect("sub2 should receive");
        assert!(matches!(cmd1, AppCommand::Shutdown));
        assert!(matches!(cmd2, AppCommand::Shutdown));
    }

    // --- Pattern matching ---

    #[tokio::test]
    async fn state_update_variants_destructure() {
        let channels = Channels::new(16);
        let Channels { state_tx, mut state_rx, .. } = channels;
        state_tx
            .send(StateUpdate::SurfaceExited { surface_id: SurfaceId::new(42), exit_code: Some(0) })
            .await
            .expect("send should succeed");
        let msg = state_rx.recv().await.expect("recv");
        match msg {
            StateUpdate::SurfaceExited { surface_id, exit_code } => {
                assert_eq!(surface_id, SurfaceId::new(42));
                assert_eq!(exit_code, Some(0));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn app_command_variants_destructure() {
        let channels = Channels::new(16);
        let mut sub = channels.command_subscriber();
        channels
            .command_tx
            .send(AppCommand::SendInput {
                surface_id: SurfaceId::new(1),
                data: vec![0x1b, 0x5b, 0x41],
            })
            .expect("send should succeed");
        let cmd = sub.recv().await.expect("recv");
        match cmd {
            AppCommand::SendInput { surface_id, data } => {
                assert_eq!(surface_id, SurfaceId::new(1));
                assert_eq!(data, vec![0x1b, 0x5b, 0x41]);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    // --- Backpressure ---

    #[tokio::test]
    async fn mpsc_respects_buffer_capacity() {
        let channels = Channels::new(2);
        let Channels { state_tx, state_rx, .. } = channels;
        // Fill the buffer
        state_tx
            .try_send(StateUpdate::ActorError {
                actor_name: "test".to_string(),
                message: "err1".to_string(),
            })
            .expect("first send should succeed");
        state_tx
            .try_send(StateUpdate::ActorError {
                actor_name: "test".to_string(),
                message: "err2".to_string(),
            })
            .expect("second send should succeed");
        // Third send should fail (buffer full)
        let result = state_tx.try_send(StateUpdate::ActorError {
            actor_name: "test".to_string(),
            message: "err3".to_string(),
        });
        assert!(result.is_err());
        drop(state_rx);
    }

    // --- Dropping senders/receivers ---

    #[tokio::test]
    async fn dropping_sender_closes_receiver() {
        let channels = Channels::new(16);
        let Channels { state_tx, mut state_rx, .. } = channels;
        drop(state_tx);
        let result = state_rx.recv().await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn dropping_receiver_causes_send_error() {
        let channels = Channels::new(16);
        let Channels { state_tx, state_rx, .. } = channels;
        drop(state_rx);
        let result = state_tx
            .send(StateUpdate::ActorError {
                actor_name: "test".to_string(),
                message: "err".to_string(),
            })
            .await;
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod notification_channel_tests {
    use super::*;
    use crate::notification::{NotificationSource, OscSequenceType};
    use crate::workspace::WorkspaceId;

    #[tokio::test]
    async fn state_update_notification_with_osc_source_applied_to_state() {
        let channels = Channels::new(16);
        let Channels { state_tx, mut state_rx, .. } = channels;

        // Send existing NotificationReceived through channel
        state_tx
            .send(StateUpdate::NotificationReceived {
                workspace_id: WorkspaceId::new(1),
                message: "osc alert".to_string(),
            })
            .await
            .expect("send should succeed");

        let msg = state_rx.recv().await.expect("should receive message");

        // Apply to AppState using the new method (tests store integration)
        let mut state = crate::state::AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), std::path::PathBuf::from("/tmp"));

        match msg {
            StateUpdate::NotificationReceived { message, .. } => {
                let source = NotificationSource::Osc { sequence_type: OscSequenceType::Osc9 };
                state.add_notification_with_source(ws_id, None, message, None, source);
            }
            _ => panic!("unexpected message"),
        }

        let latest = state.latest_notification(ws_id);
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().message, "osc alert");
    }

    #[tokio::test]
    async fn state_update_notification_with_socket_source_applied_to_state() {
        let channels = Channels::new(16);
        let Channels { state_tx, mut state_rx, .. } = channels;

        state_tx
            .send(StateUpdate::NotificationReceived {
                workspace_id: WorkspaceId::new(1),
                message: "socket notification".to_string(),
            })
            .await
            .expect("send should succeed");

        let msg = state_rx.recv().await.expect("should receive message");

        let mut state = crate::state::AppState::new();
        let ws_id = state.create_workspace("ws".to_string(), std::path::PathBuf::from("/tmp"));

        match msg {
            StateUpdate::NotificationReceived { message, .. } => {
                state.add_notification_with_source(
                    ws_id,
                    None,
                    message,
                    None,
                    NotificationSource::SocketApi,
                );
            }
            _ => panic!("unexpected message"),
        }

        let latest = state.latest_notification(ws_id);
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().message, "socket notification");
        assert!(matches!(latest.unwrap().source, NotificationSource::SocketApi));
    }

    #[test]
    fn notification_source_types_are_constructible() {
        // Verify all source variants can be constructed and compared
        let osc = NotificationSource::Osc { sequence_type: OscSequenceType::Osc9 };
        let socket = NotificationSource::SocketApi;
        let internal = NotificationSource::Internal;

        assert_ne!(osc, socket);
        assert_ne!(socket, internal);
        assert_ne!(osc, internal);
    }
}
