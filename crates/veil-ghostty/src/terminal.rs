//! Safe wrapper around a libghosty terminal instance.

use std::panic::catch_unwind;

use crate::error::{check_result, GhosttyError};
use crate::ffi;

/// Configuration for creating a new terminal.
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    /// Terminal width in cells. Must be > 0.
    pub cols: u16,
    /// Terminal height in cells. Must be > 0.
    pub rows: u16,
    /// Maximum scrollback lines.
    pub max_scrollback: usize,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self { cols: 80, rows: 24, max_scrollback: 10_000 }
    }
}

/// The active terminal screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Primary (normal) screen.
    Primary,
    /// Alternate screen (used by full-screen apps like vim).
    Alternate,
}

/// Safe wrapper around a libghosty terminal instance.
///
/// Owns the underlying C handle and frees it on drop.
/// All methods are safe; unsafe FFI calls are encapsulated.
pub struct Terminal {
    handle: ffi::GhosttyTerminal,
}

impl Terminal {
    /// Create a new terminal with the given configuration.
    ///
    /// Returns an error if allocation fails or dimensions are zero.
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(config: TerminalConfig) -> Result<Self, GhosttyError> {
        if config.cols == 0 || config.rows == 0 {
            return Err(GhosttyError::InvalidValue);
        }

        let options = ffi::GhosttyTerminalOptions {
            cols: config.cols,
            rows: config.rows,
            max_scrollback: config.max_scrollback,
        };

        let result = catch_unwind(|| {
            let mut handle: ffi::GhosttyTerminal = std::ptr::null_mut();
            // SAFETY: passing null allocator (use default), valid out-pointer,
            // and a fully-initialized options struct.
            let code =
                unsafe { ffi::ghostty_terminal_new(std::ptr::null(), &raw mut handle, options) };
            check_result(code)?;
            Ok(Self { handle })
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    /// Write VT-encoded data to the terminal for processing.
    ///
    /// Feeds raw bytes through the VT parser, updating terminal state.
    pub fn write_vt(&mut self, data: &[u8]) {
        let handle = self.handle;
        let ptr = data.as_ptr();
        let len = data.len();
        let _ = catch_unwind(move || {
            // SAFETY: handle is valid (owned by self), ptr/len come from a
            // valid slice that outlives this call.
            unsafe { ffi::ghostty_terminal_vt_write(handle, ptr, len) };
        });
    }

    /// Resize the terminal to the given cell dimensions and pixel sizes.
    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<(), GhosttyError> {
        if cols == 0 || rows == 0 {
            return Err(GhosttyError::InvalidValue);
        }

        let handle = self.handle;
        let result = catch_unwind(move || {
            // SAFETY: handle is valid (owned by self), dimensions validated above.
            let code = unsafe {
                ffi::ghostty_terminal_resize(handle, cols, rows, cell_width_px, cell_height_px)
            };
            check_result(code)
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    /// Perform a full terminal reset (RIS).
    pub fn reset(&mut self) {
        let handle = self.handle;
        let _ = catch_unwind(move || {
            // SAFETY: handle is valid (owned by self).
            unsafe { ffi::ghostty_terminal_reset(handle) };
        });
    }

    /// Query the terminal width in cells.
    pub fn cols(&self) -> Result<u16, GhosttyError> {
        self.get_u16(ffi::GHOSTTY_TERMINAL_DATA_COLS)
    }

    /// Query the terminal height in cells.
    pub fn rows(&self) -> Result<u16, GhosttyError> {
        self.get_u16(ffi::GHOSTTY_TERMINAL_DATA_ROWS)
    }

    /// Query the cursor column position (0-indexed).
    pub fn cursor_x(&self) -> Result<u16, GhosttyError> {
        self.get_u16(ffi::GHOSTTY_TERMINAL_DATA_CURSOR_X)
    }

    /// Query the cursor row position within the active area (0-indexed).
    pub fn cursor_y(&self) -> Result<u16, GhosttyError> {
        self.get_u16(ffi::GHOSTTY_TERMINAL_DATA_CURSOR_Y)
    }

    /// Query whether the cursor is visible (DEC mode 25).
    pub fn cursor_visible(&self) -> Result<bool, GhosttyError> {
        self.get_bool(ffi::GHOSTTY_TERMINAL_DATA_CURSOR_VISIBLE)
    }

    /// Query which screen is currently active.
    pub fn active_screen(&self) -> Result<Screen, GhosttyError> {
        let raw = self.get_i32(ffi::GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN)?;
        match raw {
            ffi::GHOSTTY_TERMINAL_SCREEN_PRIMARY => Ok(Screen::Primary),
            ffi::GHOSTTY_TERMINAL_SCREEN_ALTERNATE => Ok(Screen::Alternate),
            _ => Err(GhosttyError::InvalidValue),
        }
    }

    /// Query the terminal title (set via OSC 0/2).
    ///
    /// Returns an empty string if no title has been set.
    pub fn title(&self) -> Result<String, GhosttyError> {
        self.get_string(ffi::GHOSTTY_TERMINAL_DATA_TITLE)
    }

    /// Query the terminal's working directory (set via OSC 7).
    ///
    /// Returns an empty string if no pwd has been set.
    pub fn pwd(&self) -> Result<String, GhosttyError> {
        self.get_string(ffi::GHOSTTY_TERMINAL_DATA_PWD)
    }

    /// Returns the raw FFI handle. Used internally by `RenderState`.
    pub(crate) fn handle(&self) -> ffi::GhosttyTerminal {
        self.handle
    }

    // ---- Private helpers ----

    /// Query a `u16` value from the terminal.
    fn get_u16(&self, data: i32) -> Result<u16, GhosttyError> {
        let handle = self.handle;
        let result = catch_unwind(move || {
            let mut value: u16 = 0;
            // SAFETY: handle is valid (owned by self), out-pointer is valid
            // and correctly sized for the requested data type.
            let code = unsafe {
                ffi::ghostty_terminal_get(handle, data, (&raw mut value).cast::<std::ffi::c_void>())
            };
            check_result(code)?;
            Ok(value)
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    /// Query a `bool` value from the terminal.
    fn get_bool(&self, data: i32) -> Result<bool, GhosttyError> {
        let handle = self.handle;
        let result = catch_unwind(move || {
            let mut value: bool = false;
            // SAFETY: handle is valid (owned by self), out-pointer is valid
            // and correctly sized for the requested data type.
            let code = unsafe {
                ffi::ghostty_terminal_get(handle, data, (&raw mut value).cast::<std::ffi::c_void>())
            };
            check_result(code)?;
            Ok(value)
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    /// Query an `i32` value from the terminal.
    fn get_i32(&self, data: i32) -> Result<i32, GhosttyError> {
        let handle = self.handle;
        let result = catch_unwind(move || {
            let mut value: i32 = 0;
            // SAFETY: handle is valid (owned by self), out-pointer is valid
            // and correctly sized for the requested data type.
            let code = unsafe {
                ffi::ghostty_terminal_get(handle, data, (&raw mut value).cast::<std::ffi::c_void>())
            };
            check_result(code)?;
            Ok(value)
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    /// Query a string value (`GhosttyString`) from the terminal.
    ///
    /// The C API returns a borrowed `GhosttyString` pointer valid until the
    /// next `vt_write` or `reset`. We copy the bytes into an owned `String`
    /// immediately so the caller doesn't need to worry about lifetime
    /// constraints.
    ///
    /// `NoValue` (returned when no title/pwd has been set) is mapped to an
    /// empty string rather than an error, since callers typically treat
    /// "not set" and "empty" identically.
    fn get_string(&self, data: i32) -> Result<String, GhosttyError> {
        let handle = self.handle;
        let result = catch_unwind(move || {
            let mut gs = ffi::GhosttyString { ptr: std::ptr::null(), len: 0 };
            // SAFETY: handle is valid (owned by self), out-pointer is valid
            // and correctly sized for `GhosttyString`.
            let code = unsafe {
                ffi::ghostty_terminal_get(handle, data, (&raw mut gs).cast::<std::ffi::c_void>())
            };

            // NoValue means no title/pwd has been set -- return empty string.
            if code == ffi::GHOSTTY_NO_VALUE {
                return Ok(String::new());
            }
            check_result(code)?;

            if gs.ptr.is_null() || gs.len == 0 {
                return Ok(String::new());
            }

            // SAFETY: the C API guarantees `gs.ptr` points to `gs.len` valid
            // bytes that remain valid until the next `vt_write` or `reset`.
            // We copy immediately so the borrow doesn't escape.
            let bytes = unsafe { std::slice::from_raw_parts(gs.ptr, gs.len) };
            String::from_utf8(bytes.to_vec()).map_err(|_| GhosttyError::InvalidValue)
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let handle = self.handle;
        let _ = catch_unwind(move || {
            // SAFETY: handle is valid and only freed once via Drop.
            unsafe { ffi::ghostty_terminal_free(handle) };
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // All terminal FFI tests require libghosty to be linked.

    #[cfg(not(no_libghosty))]
    #[test]
    fn create_terminal_with_default_config() {
        let term = Terminal::new(TerminalConfig::default()).unwrap();
        assert_eq!(term.cols().unwrap(), 80);
        assert_eq!(term.rows().unwrap(), 24);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn create_terminal_with_custom_config() {
        let config = TerminalConfig { cols: 120, rows: 40, max_scrollback: 5_000 };
        let term = Terminal::new(config).unwrap();
        assert_eq!(term.cols().unwrap(), 120);
        assert_eq!(term.rows().unwrap(), 40);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn create_terminal_with_zero_cols_returns_error() {
        let config = TerminalConfig { cols: 0, rows: 24, max_scrollback: 10_000 };
        let result = Terminal::new(config);
        assert!(result.is_err());
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn create_terminal_with_zero_rows_returns_error() {
        let config = TerminalConfig { cols: 80, rows: 0, max_scrollback: 10_000 };
        let result = Terminal::new(config);
        assert!(result.is_err());
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn write_vt_ascii_text_advances_cursor() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        term.write_vt(b"Hello");
        assert_eq!(term.cursor_x().unwrap(), 5);
        assert_eq!(term.cursor_y().unwrap(), 0);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn write_vt_empty_data_is_noop() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        term.write_vt(b"");
        assert_eq!(term.cursor_x().unwrap(), 0);
        assert_eq!(term.cursor_y().unwrap(), 0);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn write_vt_escape_sequence_moves_cursor() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        // CUP: move cursor to row 10, col 5 (1-indexed in VT, 0-indexed in query)
        term.write_vt(b"\x1b[10;5H");
        assert_eq!(term.cursor_x().unwrap(), 4); // 0-indexed
        assert_eq!(term.cursor_y().unwrap(), 9); // 0-indexed
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn resize_to_larger_dimensions() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        term.resize(160, 48, 8, 16).unwrap();
        assert_eq!(term.cols().unwrap(), 160);
        assert_eq!(term.rows().unwrap(), 48);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn resize_to_smaller_dimensions() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        term.resize(40, 12, 8, 16).unwrap();
        assert_eq!(term.cols().unwrap(), 40);
        assert_eq!(term.rows().unwrap(), 12);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn resize_to_zero_cols_returns_error() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        let result = term.resize(0, 24, 8, 16);
        assert!(result.is_err());
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn reset_clears_state() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        term.write_vt(b"Hello, World!");
        term.reset();
        assert_eq!(term.cursor_x().unwrap(), 0);
        assert_eq!(term.cursor_y().unwrap(), 0);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn cursor_visible_default_is_true() {
        let term = Terminal::new(TerminalConfig::default()).unwrap();
        assert!(term.cursor_visible().unwrap());
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn cursor_visible_after_hide_and_show() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        // Hide cursor: DECTCEM reset
        term.write_vt(b"\x1b[?25l");
        assert!(!term.cursor_visible().unwrap());
        // Show cursor: DECTCEM set
        term.write_vt(b"\x1b[?25h");
        assert!(term.cursor_visible().unwrap());
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn active_screen_default_is_primary() {
        let term = Terminal::new(TerminalConfig::default()).unwrap();
        assert_eq!(term.active_screen().unwrap(), Screen::Primary);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn active_screen_switches_to_alternate_and_back() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        // Switch to alternate screen
        term.write_vt(b"\x1b[?1049h");
        assert_eq!(term.active_screen().unwrap(), Screen::Alternate);
        // Switch back to primary
        term.write_vt(b"\x1b[?1049l");
        assert_eq!(term.active_screen().unwrap(), Screen::Primary);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn title_default_is_empty() {
        let term = Terminal::new(TerminalConfig::default()).unwrap();
        assert_eq!(term.title().unwrap(), "");
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn pwd_default_is_empty() {
        let term = Terminal::new(TerminalConfig::default()).unwrap();
        assert_eq!(term.pwd().unwrap(), "");
    }
}
