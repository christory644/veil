#![deny(unsafe_code)]
#![warn(missing_docs)]

//! End-to-end test harness for Veil.
//!
//! Provides `VeilTestInstance` and `JsonRpcClient` for driving E2E tests
//! against Veil's JSON-RPC socket API. Both types are currently stubs
//! that define the API shape -- real implementations will arrive with
//! VEI-20 (socket API).

use std::path::PathBuf;

/// Manages the lifecycle of a test Veil instance.
///
/// In production use (once VEI-20 is implemented), this will:
/// - Launch a Veil process with a temp config and socket path
/// - Wait for the socket to become available
/// - Provide the socket path for `JsonRpcClient` to connect
/// - Shut down the process cleanly on drop
pub struct VeilTestInstance {
    socket_path: PathBuf,
}

impl VeilTestInstance {
    /// Start a new Veil test instance.
    ///
    /// Will eventually launch a real Veil process and wait for the
    /// JSON-RPC socket to become available. Currently returns an error
    /// because the socket API (VEI-20) is not yet implemented.
    pub fn start() -> Result<Self, VeilTestError> {
        Err(VeilTestError::NotImplemented("VeilTestInstance::start requires VEI-20 (socket API)"))
    }

    /// Stop the test instance and wait for clean shutdown.
    ///
    /// Will eventually send a shutdown signal and wait for the process
    /// to exit. Currently a no-op.
    pub fn stop(&mut self) -> Result<(), VeilTestError> {
        Err(VeilTestError::NotImplemented("VeilTestInstance::stop requires VEI-20 (socket API)"))
    }

    /// Returns the path to the JSON-RPC Unix socket.
    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }
}

/// Client for sending JSON-RPC 2.0 requests over a Unix socket.
///
/// Will eventually connect to a Veil instance's socket and provide
/// request/response and notification methods. Currently all methods
/// return errors because the socket API (VEI-20) is not yet implemented.
pub struct JsonRpcClient {
    _socket_path: PathBuf,
}

impl JsonRpcClient {
    /// Create a new client connected to the given socket path.
    ///
    /// Will eventually establish a Unix socket connection.
    /// Currently returns an error because VEI-20 is not implemented.
    pub fn connect(_socket_path: &std::path::Path) -> Result<Self, VeilTestError> {
        Err(VeilTestError::NotImplemented("JsonRpcClient::connect requires VEI-20 (socket API)"))
    }

    /// Send a JSON-RPC request and wait for a response.
    ///
    /// Will eventually serialize a JSON-RPC 2.0 request, send it over
    /// the socket, read the response, and return the `result` field.
    pub fn call(
        &self,
        _method: &str,
        _params: serde_json::Value,
    ) -> Result<serde_json::Value, VeilTestError> {
        Err(VeilTestError::NotImplemented("JsonRpcClient::call requires VEI-20 (socket API)"))
    }

    /// Send a JSON-RPC notification (no response expected).
    ///
    /// Will eventually serialize and send a notification without an `id`
    /// field, and return immediately without waiting for a response.
    pub fn notify(&self, _method: &str, _params: serde_json::Value) -> Result<(), VeilTestError> {
        Err(VeilTestError::NotImplemented("JsonRpcClient::notify requires VEI-20 (socket API)"))
    }
}

/// Errors from the E2E test harness.
#[derive(Debug, thiserror::Error)]
pub enum VeilTestError {
    /// Functionality not yet implemented (waiting on VEI-20).
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    /// I/O error communicating with the Veil process.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
