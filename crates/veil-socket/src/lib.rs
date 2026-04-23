#![deny(unsafe_code)]
#![warn(missing_docs)]

//! JSON-RPC socket API server for Veil.

mod connection;
mod dispatcher;
mod error_conversion;
mod handlers;
pub mod rpc;
pub mod server;
pub mod transport;

pub use rpc::{ErrorResponse, Request, Response, RpcError, RpcOutcome};
pub use server::{ServerConfig, SocketServer};
pub use transport::{SocketError, SocketListener, SocketPath};
