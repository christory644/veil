//! Application lifecycle — coordinates graceful startup and shutdown.
//!
//! The main event loop creates a `ShutdownSignal` and passes clones of
//! `ShutdownHandle` to each actor. When shutdown is triggered, all actors
//! observe it and clean up.

/// Coordinates graceful shutdown across the application.
///
/// Uses `tokio::sync::watch` (single-producer, multi-consumer) because shutdown
/// is a one-shot broadcast signal.
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    sender: tokio::sync::watch::Sender<bool>,
    receiver: tokio::sync::watch::Receiver<bool>,
}

/// A handle that actors hold to observe shutdown.
#[derive(Debug, Clone)]
pub struct ShutdownHandle {
    receiver: tokio::sync::watch::Receiver<bool>,
}

impl ShutdownSignal {
    /// Create a new signal (initially not triggered).
    pub fn new() -> Self {
        todo!()
    }

    /// Signal shutdown.
    pub fn trigger(&self) {
        todo!()
    }

    /// Create a handle for an actor to observe shutdown.
    pub fn handle(&self) -> ShutdownHandle {
        todo!()
    }

    /// Check if shutdown has been triggered.
    pub fn is_triggered(&self) -> bool {
        todo!()
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownHandle {
    /// Check if shutdown has been signaled.
    pub fn is_triggered(&self) -> bool {
        todo!()
    }

    /// Async wait until shutdown is triggered.
    pub async fn wait(&mut self) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // --- ShutdownSignal::new ---

    #[test]
    fn new_signal_is_not_triggered() {
        let signal = ShutdownSignal::new();
        assert!(!signal.is_triggered());
    }

    // --- ShutdownSignal::trigger ---

    #[test]
    fn after_trigger_is_triggered_returns_true() {
        let signal = ShutdownSignal::new();
        signal.trigger();
        assert!(signal.is_triggered());
    }

    // --- ShutdownHandle observes trigger ---

    #[test]
    fn handle_observes_shutdown_after_trigger() {
        let signal = ShutdownSignal::new();
        let handle = signal.handle();
        assert!(!handle.is_triggered());
        signal.trigger();
        assert!(handle.is_triggered());
    }

    // --- Multiple handles ---

    #[test]
    fn multiple_handles_all_observe_trigger() {
        let signal = ShutdownSignal::new();
        let h1 = signal.handle();
        let h2 = signal.handle();
        let h3 = signal.handle();
        signal.trigger();
        assert!(h1.is_triggered());
        assert!(h2.is_triggered());
        assert!(h3.is_triggered());
    }

    // --- Async wait resolves after trigger ---

    #[tokio::test]
    async fn wait_resolves_after_trigger() {
        let signal = ShutdownSignal::new();
        let mut handle = signal.handle();
        // Trigger in a spawned task
        let signal_clone = signal.clone();
        tokio::spawn(async move {
            signal_clone.trigger();
        });
        // wait should resolve
        tokio::time::timeout(Duration::from_secs(1), handle.wait())
            .await
            .expect("wait should resolve within timeout");
    }

    // --- Async wait does NOT resolve before trigger ---

    #[tokio::test]
    async fn wait_does_not_resolve_before_trigger() {
        let signal = ShutdownSignal::new();
        let mut handle = signal.handle();
        let result = tokio::time::timeout(Duration::from_millis(50), handle.wait()).await;
        // Should time out because trigger was never called
        assert!(result.is_err(), "wait should not resolve before trigger");
    }

    // --- Clone handle works independently ---

    #[test]
    fn cloned_handle_works_independently() {
        let signal = ShutdownSignal::new();
        let h1 = signal.handle();
        let h2 = h1.clone();
        signal.trigger();
        assert!(h1.is_triggered());
        assert!(h2.is_triggered());
    }
}
