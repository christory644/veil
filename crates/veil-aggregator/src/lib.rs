#![deny(unsafe_code)]
#![warn(missing_docs)]

//! Session aggregator and agent adapters for Veil.

pub mod adapter;
pub mod claude_code;
pub mod live_state_cache;
pub mod live_state_resolver;
pub mod registry;
pub mod store;
pub mod title;

#[cfg(test)]
pub(crate) mod testutil;
