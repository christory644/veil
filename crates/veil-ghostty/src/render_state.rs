//! Safe wrapper around a libghosty render state instance.

use std::panic::catch_unwind;

use crate::error::{check_result, GhosttyError};
use crate::ffi;
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
#[allow(clippy::struct_excessive_bools)]
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
    handle: ffi::GhosttyRenderState,
}

impl RenderState {
    /// Create a new empty render state.
    pub fn new() -> Result<Self, GhosttyError> {
        let result = catch_unwind(|| {
            let mut handle: ffi::GhosttyRenderState = std::ptr::null_mut();
            let code = unsafe { ffi::ghostty_render_state_new(std::ptr::null(), &raw mut handle) };
            check_result(code)?;
            Ok(Self { handle })
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    /// Update the render state from a terminal.
    ///
    /// This reads the terminal's current state and computes what has
    /// changed since the last update.
    pub fn update(&mut self, terminal: &mut Terminal) -> Result<(), GhosttyError> {
        let state_handle = self.handle;
        let term_handle = terminal.handle();
        let result = catch_unwind(move || {
            let code = unsafe { ffi::ghostty_render_state_update(state_handle, term_handle) };
            check_result(code)
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    /// Query the current dirty state.
    pub fn dirty(&self) -> Result<DirtyState, GhosttyError> {
        let state_handle = self.handle;
        let result = catch_unwind(move || {
            let mut value: i32 = 0;
            let code = unsafe {
                ffi::ghostty_render_state_get(
                    state_handle,
                    ffi::GHOSTTY_RENDER_STATE_DATA_DIRTY,
                    (&raw mut value).cast::<std::ffi::c_void>(),
                )
            };
            check_result(code)?;
            match value {
                ffi::GHOSTTY_RENDER_STATE_DIRTY_FALSE => Ok(DirtyState::Clean),
                ffi::GHOSTTY_RENDER_STATE_DIRTY_PARTIAL => Ok(DirtyState::Partial),
                ffi::GHOSTTY_RENDER_STATE_DIRTY_FULL => Ok(DirtyState::Full),
                other => Err(GhosttyError::Unknown(other)),
            }
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    /// Set the dirty state (typically to `Clean` after rendering a frame).
    pub fn set_dirty(&mut self, state: DirtyState) -> Result<(), GhosttyError> {
        let raw_value: i32 = match state {
            DirtyState::Clean => ffi::GHOSTTY_RENDER_STATE_DIRTY_FALSE,
            DirtyState::Partial => ffi::GHOSTTY_RENDER_STATE_DIRTY_PARTIAL,
            DirtyState::Full => ffi::GHOSTTY_RENDER_STATE_DIRTY_FULL,
        };
        let state_handle = self.handle;
        let result = catch_unwind(move || {
            let code = unsafe {
                ffi::ghostty_render_state_set(
                    state_handle,
                    ffi::GHOSTTY_RENDER_STATE_OPTION_DIRTY,
                    (&raw const raw_value).cast::<std::ffi::c_void>(),
                )
            };
            check_result(code)
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    /// Query the viewport width in cells.
    pub fn cols(&self) -> Result<u16, GhosttyError> {
        self.get_u16(ffi::GHOSTTY_RENDER_STATE_DATA_COLS)
    }

    /// Query the viewport height in cells.
    pub fn rows(&self) -> Result<u16, GhosttyError> {
        self.get_u16(ffi::GHOSTTY_RENDER_STATE_DATA_ROWS)
    }

    /// Query the full cursor state.
    pub fn cursor(&self) -> Result<CursorState, GhosttyError> {
        let in_viewport =
            self.get_bool(ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE)?;
        let x = self.get_u16(ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X)?;
        let y = self.get_u16(ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y)?;
        let visible = self.get_bool(ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR_VISIBLE)?;
        let blinking = self.get_bool(ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR_BLINKING)?;
        let password_input = self.get_bool(ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR_PASSWORD_INPUT)?;

        let style_raw = self.get_i32(ffi::GHOSTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE)?;
        let style = match style_raw {
            ffi::GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR => CursorStyle::Bar,
            ffi::GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK => CursorStyle::Block,
            ffi::GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE => CursorStyle::Underline,
            ffi::GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW => CursorStyle::BlockHollow,
            other => return Err(GhosttyError::Unknown(other)),
        };

        Ok(CursorState { in_viewport, x, y, visible, blinking, style, password_input })
    }

    /// Query the render colors (background, foreground, optional cursor).
    pub fn colors(&self) -> Result<RenderColors, GhosttyError> {
        let state_handle = self.handle;
        let result = catch_unwind(move || {
            let mut colors = ffi::GhosttyRenderStateColors {
                size: std::mem::size_of::<ffi::GhosttyRenderStateColors>(),
                ..unsafe { std::mem::zeroed() }
            };
            let code =
                unsafe { ffi::ghostty_render_state_colors_get(state_handle, &raw mut colors) };
            check_result(code)?;

            let cursor = if colors.cursor_has_value {
                Some(Color { r: colors.cursor.r, g: colors.cursor.g, b: colors.cursor.b })
            } else {
                None
            };

            Ok(RenderColors {
                background: Color {
                    r: colors.background.r,
                    g: colors.background.g,
                    b: colors.background.b,
                },
                foreground: Color {
                    r: colors.foreground.r,
                    g: colors.foreground.g,
                    b: colors.foreground.b,
                },
                cursor,
            })
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    // ---- Private helpers ----

    /// Query a u16 value from the render state.
    fn get_u16(&self, data: i32) -> Result<u16, GhosttyError> {
        let state_handle = self.handle;
        let result = catch_unwind(move || {
            let mut value: u16 = 0;
            let code = unsafe {
                ffi::ghostty_render_state_get(
                    state_handle,
                    data,
                    (&raw mut value).cast::<std::ffi::c_void>(),
                )
            };
            check_result(code)?;
            Ok(value)
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    /// Query a bool value from the render state.
    fn get_bool(&self, data: i32) -> Result<bool, GhosttyError> {
        let state_handle = self.handle;
        let result = catch_unwind(move || {
            let mut value: bool = false;
            let code = unsafe {
                ffi::ghostty_render_state_get(
                    state_handle,
                    data,
                    (&raw mut value).cast::<std::ffi::c_void>(),
                )
            };
            check_result(code)?;
            Ok(value)
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }

    /// Query an i32 value from the render state.
    fn get_i32(&self, data: i32) -> Result<i32, GhosttyError> {
        let state_handle = self.handle;
        let result = catch_unwind(move || {
            let mut value: i32 = 0;
            let code = unsafe {
                ffi::ghostty_render_state_get(
                    state_handle,
                    data,
                    (&raw mut value).cast::<std::ffi::c_void>(),
                )
            };
            check_result(code)?;
            Ok(value)
        });
        match result {
            Ok(inner) => inner,
            Err(_) => Err(GhosttyError::Panic),
        }
    }
}

impl Drop for RenderState {
    fn drop(&mut self) {
        // Safety: handle is valid and only freed once via Drop.
        let handle = self.handle;
        let _ = catch_unwind(move || {
            unsafe { ffi::ghostty_render_state_free(handle) };
        });
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
