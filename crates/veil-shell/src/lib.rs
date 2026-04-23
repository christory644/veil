#![deny(unsafe_code)]
#![warn(missing_docs)]

//! Shell integration for Veil: directory tracking, agent detection,
//! and environment awareness.
//!
//! This crate provides the parsing and detection logic for shell
//! integration features. It processes OSC 7 payloads, inspects
//! process trees for known AI agents, and detects project environments.
//!
//! The crate produces [`event::ShellEvent`]s that are consumed by the PTY I/O
//! integration layer (VEI-70) to update workspace and conversation state.

pub mod agent_detector;
pub mod env_detector;
pub mod event;
pub mod osc7;
pub mod process_list;
pub mod tracker;
