#![deny(unsafe_code)]
#![warn(missing_docs)]

//! Core types and state management for Veil.

pub mod config;
pub mod dir_checker;
pub mod error;
pub mod focus;
pub mod git_checker;
pub mod keyboard;
pub mod layout;
pub mod lifecycle;
pub mod live_state;
pub mod message;
pub mod navigation;
pub mod notification;
pub mod osc_parse;
pub mod pr_checker;
pub mod session;
pub mod state;
pub mod update;
pub mod workspace;
