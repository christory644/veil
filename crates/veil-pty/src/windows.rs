//! Windows ConPTY implementation -- not yet implemented.
//!
//! TODO(VEI-XX): Implement Windows ConPTY support.
//! References: wezterm's portable-pty, alacritty's tty::windows module.

use crate::error::PtyError;
use crate::types::{PtyEvent, PtySize};
use crate::Pty;

/// Windows ConPTY stub -- not yet implemented.
pub(crate) struct WindowsPty;

impl WindowsPty {
    /// Attempt to create a Windows PTY (currently unsupported).
    pub(crate) fn new() -> Result<Self, PtyError> {
        Err(PtyError::Unsupported)
    }
}

impl Pty for WindowsPty {
    fn take_event_rx(&mut self) -> Option<std::sync::mpsc::Receiver<PtyEvent>> {
        None
    }

    fn writer(&self) -> std::sync::mpsc::Sender<Vec<u8>> {
        // Create a disconnected channel -- writes will fail
        let (tx, _rx) = std::sync::mpsc::channel();
        tx
    }

    fn resize(&self, _size: PtySize) -> Result<(), PtyError> {
        Err(PtyError::Unsupported)
    }

    fn child_pid(&self) -> Option<u32> {
        None
    }

    fn shutdown(&mut self) -> Result<(), PtyError> {
        Ok(())
    }

    fn is_closed(&self) -> bool {
        true
    }
}
