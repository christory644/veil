//! Safe wrapper around a libghosty terminal instance.

use crate::error::GhosttyError;

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
    _private: (), // placeholder -- will hold ffi::GhosttyTerminal
}

impl Terminal {
    /// Create a new terminal with the given configuration.
    ///
    /// Returns an error if allocation fails or dimensions are zero.
    pub fn new(_config: TerminalConfig) -> Result<Self, GhosttyError> {
        todo!("Terminal::new: create terminal via FFI")
    }

    /// Write VT-encoded data to the terminal for processing.
    ///
    /// Feeds raw bytes through the VT parser, updating terminal state.
    pub fn write_vt(&mut self, _data: &[u8]) {
        todo!("Terminal::write_vt: write data via FFI")
    }

    /// Resize the terminal to the given cell dimensions and pixel sizes.
    pub fn resize(
        &mut self,
        _cols: u16,
        _rows: u16,
        _cell_width_px: u32,
        _cell_height_px: u32,
    ) -> Result<(), GhosttyError> {
        todo!("Terminal::resize: resize via FFI")
    }

    /// Perform a full terminal reset (RIS).
    pub fn reset(&mut self) {
        todo!("Terminal::reset: reset via FFI")
    }

    /// Query the terminal width in cells.
    pub fn cols(&self) -> Result<u16, GhosttyError> {
        todo!("Terminal::cols: query via FFI")
    }

    /// Query the terminal height in cells.
    pub fn rows(&self) -> Result<u16, GhosttyError> {
        todo!("Terminal::rows: query via FFI")
    }

    /// Query the cursor column position (0-indexed).
    pub fn cursor_x(&self) -> Result<u16, GhosttyError> {
        todo!("Terminal::cursor_x: query via FFI")
    }

    /// Query the cursor row position within the active area (0-indexed).
    pub fn cursor_y(&self) -> Result<u16, GhosttyError> {
        todo!("Terminal::cursor_y: query via FFI")
    }

    /// Query whether the cursor is visible (DEC mode 25).
    pub fn cursor_visible(&self) -> Result<bool, GhosttyError> {
        todo!("Terminal::cursor_visible: query via FFI")
    }

    /// Query which screen is currently active.
    pub fn active_screen(&self) -> Result<Screen, GhosttyError> {
        todo!("Terminal::active_screen: query via FFI")
    }

    /// Query the terminal title (set via OSC 0/2).
    ///
    /// Returns an empty string if no title has been set.
    pub fn title(&self) -> Result<String, GhosttyError> {
        todo!("Terminal::title: query via FFI")
    }

    /// Query the terminal's working directory (set via OSC 7).
    ///
    /// Returns an empty string if no pwd has been set.
    pub fn pwd(&self) -> Result<String, GhosttyError> {
        todo!("Terminal::pwd: query via FFI")
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
