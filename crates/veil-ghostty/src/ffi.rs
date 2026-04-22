//! Raw FFI declarations for the libghosty-vt C API.
//!
//! This module declares only the subset of the Ghostty VT API needed
//! by VEI-6. Types mirror the C header exactly, with Rust equivalents.

// Some constants and types are declared for completeness with the C API
// but not yet consumed by safe wrappers. Allow dead code at the module level.
#![allow(dead_code)]

use std::ffi::c_void;

// ---- Opaque handles ----
// The C API uses typed opaque pointers (e.g. `struct GhosttyTerminalImpl*`).
// We model these as `*mut c_void` on the Rust side since we never
// dereference them -- they're only passed back to C functions.

/// Opaque handle to a libghosty terminal instance.
pub type GhosttyTerminal = *mut c_void;

/// Opaque handle to a libghosty render state instance.
pub type GhosttyRenderState = *mut c_void;

// ---- Result code ----

/// Raw result codes from the C API, represented as `c_int`.
/// We use i32 directly since `c_int` is i32 on all supported platforms.
pub const GHOSTTY_SUCCESS: i32 = 0;
pub const GHOSTTY_OUT_OF_MEMORY: i32 = -1;
pub const GHOSTTY_INVALID_VALUE: i32 = -2;
pub const GHOSTTY_OUT_OF_SPACE: i32 = -3;
pub const GHOSTTY_NO_VALUE: i32 = -4;

// ---- Terminal init options ----

/// Terminal initialization options, matching `GhosttyTerminalOptions` in C.
#[repr(C)]
pub struct GhosttyTerminalOptions {
    /// Terminal width in cells. Must be greater than zero.
    pub cols: u16,
    /// Terminal height in cells. Must be greater than zero.
    pub rows: u16,
    /// Maximum number of lines to keep in scrollback history.
    pub max_scrollback: usize,
}

// ---- Borrowed byte string ----

/// A borrowed byte string (pointer + length), matching `GhosttyString` in C.
#[repr(C)]
pub struct GhosttyString {
    /// Pointer to the string bytes.
    pub ptr: *const u8,
    /// Length of the string in bytes.
    pub len: usize,
}

// ---- Terminal data query enum ----

/// Terminal data types for `ghostty_terminal_get`.
/// We represent these as i32 constants to match the C `int` enum ABI.
pub const GHOSTTY_TERMINAL_DATA_COLS: i32 = 1;
pub const GHOSTTY_TERMINAL_DATA_ROWS: i32 = 2;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_X: i32 = 3;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_Y: i32 = 4;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP: i32 = 5;
pub const GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN: i32 = 6;
pub const GHOSTTY_TERMINAL_DATA_CURSOR_VISIBLE: i32 = 7;
pub const GHOSTTY_TERMINAL_DATA_TITLE: i32 = 12;
pub const GHOSTTY_TERMINAL_DATA_PWD: i32 = 13;

// ---- Terminal screen enum ----

/// Terminal screen identifiers matching `GhosttyTerminalScreen` in C.
pub const GHOSTTY_TERMINAL_SCREEN_PRIMARY: i32 = 0;
pub const GHOSTTY_TERMINAL_SCREEN_ALTERNATE: i32 = 1;

// ---- Color types ----

/// RGB color value, matching `GhosttyColorRgb` in C.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhosttyColorRgb {
    /// Red component (0-255).
    pub r: u8,
    /// Green component (0-255).
    pub g: u8,
    /// Blue component (0-255).
    pub b: u8,
}

// ---- Render state dirty enum ----

/// Dirty state values matching `GhosttyRenderStateDirty` in C.
pub const GHOSTTY_RENDER_STATE_DIRTY_FALSE: i32 = 0;
pub const GHOSTTY_RENDER_STATE_DIRTY_PARTIAL: i32 = 1;
pub const GHOSTTY_RENDER_STATE_DIRTY_FULL: i32 = 2;

// ---- Render state cursor visual style ----

/// Cursor visual style values matching `GhosttyRenderStateCursorVisualStyle`.
pub const GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR: i32 = 0;
pub const GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK: i32 = 1;
pub const GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE: i32 = 2;
pub const GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW: i32 = 3;

// ---- Render state data query enum ----

/// Render state data types for `ghostty_render_state_get`.
pub const GHOSTTY_RENDER_STATE_DATA_COLS: i32 = 1;
pub const GHOSTTY_RENDER_STATE_DATA_ROWS: i32 = 2;
pub const GHOSTTY_RENDER_STATE_DATA_DIRTY: i32 = 3;
pub const GHOSTTY_RENDER_STATE_DATA_COLOR_BACKGROUND: i32 = 5;
pub const GHOSTTY_RENDER_STATE_DATA_COLOR_FOREGROUND: i32 = 6;
pub const GHOSTTY_RENDER_STATE_DATA_COLOR_CURSOR: i32 = 7;
pub const GHOSTTY_RENDER_STATE_DATA_COLOR_CURSOR_HAS_VALUE: i32 = 8;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE: i32 = 10;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VISIBLE: i32 = 11;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_BLINKING: i32 = 12;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_PASSWORD_INPUT: i32 = 13;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE: i32 = 14;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X: i32 = 15;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y: i32 = 16;
pub const GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_WIDE_TAIL: i32 = 17;

// ---- Render state option enum ----

/// Render state option types for `ghostty_render_state_set`.
pub const GHOSTTY_RENDER_STATE_OPTION_DIRTY: i32 = 0;

// ---- Render state colors (sized struct) ----

/// Render-state color information, matching `GhosttyRenderStateColors` in C.
///
/// Uses the sized-struct ABI pattern: `size` must be set to
/// `std::mem::size_of::<GhosttyRenderStateColors>()` before passing to the C API.
#[repr(C)]
pub struct GhosttyRenderStateColors {
    /// Size of this struct in bytes.
    pub size: usize,
    /// Default/current background color.
    pub background: GhosttyColorRgb,
    /// Default/current foreground color.
    pub foreground: GhosttyColorRgb,
    /// Cursor color when explicitly set.
    pub cursor: GhosttyColorRgb,
    /// Whether `cursor` contains a valid explicit cursor color.
    pub cursor_has_value: bool,
    /// The active 256-color palette.
    pub palette: [GhosttyColorRgb; 256],
}

// ---- extern "C" function declarations ----

extern "C" {
    // Terminal lifecycle
    pub fn ghostty_terminal_new(
        allocator: *const c_void,
        terminal: *mut GhosttyTerminal,
        options: GhosttyTerminalOptions,
    ) -> i32;

    pub fn ghostty_terminal_free(terminal: GhosttyTerminal);

    pub fn ghostty_terminal_reset(terminal: GhosttyTerminal);

    pub fn ghostty_terminal_resize(
        terminal: GhosttyTerminal,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> i32;

    pub fn ghostty_terminal_vt_write(terminal: GhosttyTerminal, data: *const u8, len: usize);

    pub fn ghostty_terminal_get(
        terminal: GhosttyTerminal,
        data: i32, // GhosttyTerminalData enum as c_int
        out: *mut c_void,
    ) -> i32;

    // Render state lifecycle
    pub fn ghostty_render_state_new(
        allocator: *const c_void,
        state: *mut GhosttyRenderState,
    ) -> i32;

    pub fn ghostty_render_state_free(state: GhosttyRenderState);

    pub fn ghostty_render_state_update(state: GhosttyRenderState, terminal: GhosttyTerminal)
        -> i32;

    pub fn ghostty_render_state_get(
        state: GhosttyRenderState,
        data: i32, // GhosttyRenderStateData enum as c_int
        out: *mut c_void,
    ) -> i32;

    pub fn ghostty_render_state_set(
        state: GhosttyRenderState,
        option: i32, // GhosttyRenderStateOption enum as c_int
        value: *const c_void,
    ) -> i32;

    pub fn ghostty_render_state_colors_get(
        state: GhosttyRenderState,
        out_colors: *mut GhosttyRenderStateColors,
    ) -> i32;
}
