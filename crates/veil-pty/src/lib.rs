#![deny(unsafe_code)]
#![warn(missing_docs)]

//! Cross-platform PTY abstraction for Veil.
//!
//! Provides a platform-agnostic [`Pty`] trait for creating and managing
//! pseudo-terminals, plus a [`PtyManager`] actor that dispatches
//! [`AppCommand`](veil_core::message::AppCommand) messages to PTY instances.

pub mod error;
pub mod manager;
pub mod types;

#[cfg(unix)]
#[allow(unsafe_code)]
mod posix;

#[cfg(windows)]
mod windows;

pub use error::PtyError;
pub use manager::PtyManager;
pub use types::{PtyConfig, PtyEvent, PtySize};

/// Platform abstraction for a pseudo-terminal.
///
/// A `Pty` owns the master side of a PTY pair and the associated child process.
/// It provides channels for I/O and methods for resize and shutdown.
///
/// The read side is a background thread that sends [`PtyEvent`]s through a channel.
/// The write side accepts bytes through a channel.
pub trait Pty: Send {
    /// Get the receiver for PTY events (output bytes, child exit).
    ///
    /// This is a `std` mpsc `Receiver`. The background read thread sends events
    /// through it. Returns `None` if the receiver has already been taken.
    fn take_event_rx(&mut self) -> Option<std::sync::mpsc::Receiver<PtyEvent>>;

    /// Get the sender for writing bytes to the PTY.
    ///
    /// Clone this sender to write from multiple places if needed.
    fn writer(&self) -> std::sync::mpsc::Sender<Vec<u8>>;

    /// Resize the PTY to new dimensions.
    fn resize(&self, size: PtySize) -> Result<(), PtyError>;

    /// Get the child process ID, if available.
    fn child_pid(&self) -> Option<u32>;

    /// Request graceful shutdown.
    ///
    /// On POSIX: sends SIGHUP to the child process group, then closes the
    /// master fd. The read thread will observe the close and emit `ChildExited`.
    ///
    /// This is idempotent -- calling it multiple times is safe.
    fn shutdown(&mut self) -> Result<(), PtyError>;

    /// Check if the PTY has been shut down.
    fn is_closed(&self) -> bool;
}

/// Create a new PTY with the given configuration.
///
/// This allocates the PTY pair, spawns the child process, and starts
/// the background read and write threads. Returns a boxed trait object.
///
/// Dispatches to the platform implementation at compile time.
#[cfg(unix)]
pub fn create_pty(config: PtyConfig) -> Result<Box<dyn Pty>, PtyError> {
    posix::PosixPty::new(config).map(|p| Box::new(p) as Box<dyn Pty>)
}

/// Create a new PTY with the given configuration.
///
/// On Windows, this currently returns [`PtyError::Unsupported`].
#[cfg(windows)]
pub fn create_pty(_config: PtyConfig) -> Result<Box<dyn Pty>, PtyError> {
    Err(PtyError::Unsupported)
}
