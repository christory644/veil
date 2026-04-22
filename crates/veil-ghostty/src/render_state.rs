//! Safe wrapper around a libghosty render state instance.

use crate::error::GhosttyError;
use crate::terminal::Terminal;

/// Dirty state of a render state after update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyState {
    /// No changes; rendering can be skipped entirely.
    Clean,
    /// Some rows changed; renderer can redraw incrementally.
    Partial,
    /// Global state changed; renderer should redraw everything.
    Full,
}

/// Visual style of the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    /// Vertical bar cursor.
    Bar,
    /// Filled block cursor.
    Block,
    /// Underline cursor.
    Underline,
    /// Hollow block cursor.
    BlockHollow,
}

/// RGB color value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red component (0-255).
    pub r: u8,
    /// Green component (0-255).
    pub g: u8,
    /// Blue component (0-255).
    pub b: u8,
}

/// Cursor state within the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorState {
    /// Whether the cursor is visible in the viewport.
    pub in_viewport: bool,
    /// X position in cells (only valid when `in_viewport` is true).
    pub x: u16,
    /// Y position in cells (only valid when `in_viewport` is true).
    pub y: u16,
    /// Whether the cursor is visible (DEC mode 25).
    pub visible: bool,
    /// Whether the cursor should blink.
    pub blinking: bool,
    /// The visual style of the cursor.
    pub style: CursorStyle,
    /// Whether the cursor is at a password input field.
    pub password_input: bool,
}

/// Color theme state from the render state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderColors {
    /// Background color.
    pub background: Color,
    /// Foreground color.
    pub foreground: Color,
    /// Cursor color, if explicitly set by the terminal.
    pub cursor: Option<Color>,
}

/// Safe wrapper around a libghosty render state instance.
///
/// Owns the underlying C handle and frees it on drop.
pub struct RenderState {
    _private: (), // placeholder -- will hold ffi::GhosttyRenderState
}

impl RenderState {
    /// Create a new empty render state.
    pub fn new() -> Result<Self, GhosttyError> {
        todo!("RenderState::new: create render state via FFI")
    }

    /// Update the render state from a terminal.
    ///
    /// This reads the terminal's current state and computes what has
    /// changed since the last update.
    pub fn update(&mut self, _terminal: &mut Terminal) -> Result<(), GhosttyError> {
        todo!("RenderState::update: update from terminal via FFI")
    }

    /// Query the current dirty state.
    pub fn dirty(&self) -> Result<DirtyState, GhosttyError> {
        todo!("RenderState::dirty: query via FFI")
    }

    /// Set the dirty state (typically to `Clean` after rendering a frame).
    pub fn set_dirty(&mut self, _state: DirtyState) -> Result<(), GhosttyError> {
        todo!("RenderState::set_dirty: set via FFI")
    }

    /// Query the viewport width in cells.
    pub fn cols(&self) -> Result<u16, GhosttyError> {
        todo!("RenderState::cols: query via FFI")
    }

    /// Query the viewport height in cells.
    pub fn rows(&self) -> Result<u16, GhosttyError> {
        todo!("RenderState::rows: query via FFI")
    }

    /// Query the full cursor state.
    pub fn cursor(&self) -> Result<CursorState, GhosttyError> {
        todo!("RenderState::cursor: query via FFI")
    }

    /// Query the render colors (background, foreground, optional cursor).
    pub fn colors(&self) -> Result<RenderColors, GhosttyError> {
        todo!("RenderState::colors: query via FFI")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TerminalConfig;

    // All render state FFI tests require libghosty to be linked.

    #[cfg(not(no_libghosty))]
    #[test]
    fn create_render_state_succeeds() {
        let _rs = RenderState::new().unwrap();
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn update_from_fresh_terminal_is_full_dirty() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        let mut rs = RenderState::new().unwrap();
        rs.update(&mut term).unwrap();
        assert_eq!(rs.dirty().unwrap(), DirtyState::Full);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn after_update_cols_rows_match_terminal() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        let mut rs = RenderState::new().unwrap();
        rs.update(&mut term).unwrap();
        assert_eq!(rs.cols().unwrap(), 80);
        assert_eq!(rs.rows().unwrap(), 24);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn after_update_cursor_reflects_terminal() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        let mut rs = RenderState::new().unwrap();
        rs.update(&mut term).unwrap();
        let cursor = rs.cursor().unwrap();
        assert!(cursor.visible);
        assert!(cursor.in_viewport);
        assert_eq!(cursor.x, 0);
        assert_eq!(cursor.y, 0);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn set_dirty_clean_then_query_returns_clean() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        let mut rs = RenderState::new().unwrap();
        rs.update(&mut term).unwrap();
        rs.set_dirty(DirtyState::Clean).unwrap();
        assert_eq!(rs.dirty().unwrap(), DirtyState::Clean);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn update_after_no_changes_is_clean() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        let mut rs = RenderState::new().unwrap();
        rs.update(&mut term).unwrap();
        rs.set_dirty(DirtyState::Clean).unwrap();
        // Update again without any terminal changes
        rs.update(&mut term).unwrap();
        assert_eq!(rs.dirty().unwrap(), DirtyState::Clean);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn cursor_hidden_in_render_state_after_hide_escape() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        term.write_vt(b"\x1b[?25l"); // hide cursor
        let mut rs = RenderState::new().unwrap();
        rs.update(&mut term).unwrap();
        let cursor = rs.cursor().unwrap();
        assert!(!cursor.visible);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn resize_terminal_updates_render_state_dimensions() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        term.resize(100, 50, 8, 16).unwrap();
        let mut rs = RenderState::new().unwrap();
        rs.update(&mut term).unwrap();
        assert_eq!(rs.cols().unwrap(), 100);
        assert_eq!(rs.rows().unwrap(), 50);
    }

    #[cfg(not(no_libghosty))]
    #[test]
    fn colors_returns_foreground_and_background() {
        let mut term = Terminal::new(TerminalConfig::default()).unwrap();
        let mut rs = RenderState::new().unwrap();
        rs.update(&mut term).unwrap();
        let colors = rs.colors().unwrap();
        // Default colors should exist (non-zero is not guaranteed,
        // but the struct should be populated without error)
        let _ = colors.background;
        let _ = colors.foreground;
    }
}
