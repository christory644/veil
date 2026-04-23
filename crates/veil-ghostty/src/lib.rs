#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

//! Safe FFI wrapper around libghosty for terminal emulation.
//!
//! This crate provides safe Rust abstractions over the libghosty-vt
//! C API for terminal emulation. All unsafe FFI code is encapsulated
//! within this crate -- consumers get a 100% safe API.
//!
//! When the `no_libghosty` cfg is set (because the native library is
//! not available), this crate compiles as an empty module with only
//! the error type available.

// Error type is always available (no FFI dependency)
mod error;
pub use error::GhosttyError;

#[cfg(not(no_libghosty))]
mod ffi;

#[cfg(not(no_libghosty))]
mod terminal;

#[cfg(not(no_libghosty))]
mod render_state;

#[cfg(not(no_libghosty))]
pub use terminal::{Screen, Terminal, TerminalConfig};

#[cfg(not(no_libghosty))]
pub use render_state::{
    CellData, CellGrid, Color, CursorState, CursorStyle, DirtyState, RenderColors, RenderState,
};
