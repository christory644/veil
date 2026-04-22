#![deny(unsafe_code)]
#![warn(missing_docs)]

//! Session aggregator and agent adapters for Veil.

pub mod adapter;
pub mod registry;
pub mod store;
pub mod title;

#[cfg(test)]
pub(crate) mod testutil;
