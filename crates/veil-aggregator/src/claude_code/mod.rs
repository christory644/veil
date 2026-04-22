//! Claude Code agent adapter — session discovery and JSONL parsing.

mod adapter;
mod discovery;
/// JSONL record types for Claude Code session files.
pub mod jsonl;
/// JSONL file parser for Claude Code sessions.
pub mod parser;

pub use adapter::ClaudeCodeAdapter;
