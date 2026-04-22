# VEI-6: libghosty FFI Integration

## Context

Veil uses libghosty (from Ghostty) as its terminal emulation engine. libghosty-vt is a zero-dependency library that handles VT sequence parsing, terminal state management (cursor position, styles, text reflow, scrollback), and renderer state management. It exposes a C API that Veil consumes via Rust FFI.

This task integrates the pre-built `libghostty-vt.a` static library into the `veil-ghostty` crate, producing safe Rust wrappers over the raw C API. The crate is the sole boundary where `unsafe` FFI code lives -- all other Veil crates consume a 100% safe Rust API.

The scope is deliberately constrained to the core terminal lifecycle and query APIs:
- Terminal creation, writing VT data, resizing, resetting, and querying state (cursor, dimensions, title, pwd, visibility, active screen)
- Render state creation, updating from a terminal, querying dirty state, cursor position/style, viewport dimensions, and colors
- Error mapping from `GhosttyResult` to idiomatic Rust `Result`

**NOT in scope** (follow-up issues):
- Callback registration (write_pty, bell, title_changed) -- VEI-36
- Row/cell iteration for rendering pipeline -- VEI-37
- Key and mouse encoding -- VEI-38
- Scrollback navigation
- Windows/Linux verification

### What already exists

- `crates/veil-ghostty/src/lib.rs` -- 9-line skeleton with `#[cfg(not(no_libghosty))]` gated empty `ffi` module
- `crates/veil-ghostty/build.rs` -- checks for `libghosty/` directory, sets `no_libghosty` cfg when absent
- `crates/veil-ghostty/Cargo.toml` -- depends on `veil-core` and `tracing`

### Build prerequisites

- **Zig 0.15.2** (exact version required by Ghostty)
- Ghostty source vendored as git submodule at `vendor/ghostty`
- Pre-built library at `vendor/ghostty-lib/lib/libghostty-vt.a` (gitignored, must be built locally)
- Build command: `cd vendor/ghostty && zig build -Demit-lib-vt=true -Doptimize=ReleaseFast`
- Header files at `vendor/ghostty/include/ghostty/vt.h` (umbrella header) and `vendor/ghostty/include/ghostty/vt/*.h`

### Key design decisions

**Manual FFI bindings, not bindgen.** The Ghostty VT C API uses opaque pointer handles, tagged enums with `INT_MAX` sentinels, `void*` out-params for a generic getter pattern, and Zig-specific allocator structs. These patterns produce bindings that are awkward to use with bindgen (lots of `c_int` enums, untyped void pointers). Writing manual `extern "C"` declarations for the ~25 functions we actually call gives us precise control over types, avoids a build-time bindgen dependency, and keeps the FFI surface small and auditable. We can always switch to bindgen later if the API surface grows significantly.

**`catch_unwind` at the FFI boundary.** Per AGENTS.md, all FFI calls are wrapped in `catch_unwind` to prevent panics from unwinding across the FFI boundary. In practice, the Ghostty C API is Zig code compiled to C ABI and won't panic in the Rust sense, but the wrapper protects against hypothetical future changes and satisfies the project's safety invariant.

**Conditional compilation preserved.** The existing `no_libghosty` cfg pattern is retained. When libghosty is not available, the crate compiles with stub types that cannot be instantiated. This lets the rest of the workspace build and test without the native library. Tests that exercise real FFI are gated behind `#[cfg(not(no_libghosty))]`.

**Default allocator (NULL).** All `GhosttyAllocator*` parameters are passed as `std::ptr::null()`, using Ghostty's default allocator. Custom allocators are unnecessary for Veil's use case.

## Implementation Units

### Unit 1: Build system and raw FFI declarations

Set up the build pipeline to find the pre-built static library and declare the raw C function signatures.

**Files:**

| File | Purpose |
|------|---------|
| `crates/veil-ghostty/build.rs` | Rewrite: locate `libghostty-vt.a`, emit linker directives, set `no_libghosty` cfg when absent |
| `crates/veil-ghostty/src/ffi.rs` | Raw `extern "C"` declarations and C type definitions |
| `crates/veil-ghostty/src/lib.rs` | Module structure, re-exports |

**build.rs changes:**

```rust
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(no_libghosty)");

    // Look for the pre-built static library
    let manifest_dir = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").unwrap(),
    );
    let workspace_root = manifest_dir
        .parent().unwrap()  // crates/
        .parent().unwrap(); // workspace root

    let lib_dir = workspace_root.join("vendor/ghostty-lib/lib");
    let lib_path = lib_dir.join("libghostty-vt.a");

    if !lib_path.exists() {
        println!(
            "cargo:warning=vendor/ghostty-lib/lib/libghostty-vt.a not found \
             -- building without libghosty support"
        );
        println!("cargo:rustc-cfg=no_libghosty");
        return;
    }

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=ghostty-vt");

    // libghostty-vt may depend on system libraries on macOS
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=System");
    }
}
```

**ffi.rs -- raw C type definitions and function declarations:**

The module declares only the subset of the Ghostty VT API needed by VEI-6. Types mirror the C header exactly, with Rust equivalents:

```rust
// Opaque handles
pub type GhosttyTerminal = *mut std::ffi::c_void;
pub type GhosttyRenderState = *mut std::ffi::c_void;

// Result code enum
#[repr(C)]
pub enum GhosttyResult {
    Success = 0,
    OutOfMemory = -1,
    InvalidValue = -2,
    OutOfSpace = -3,
    NoValue = -4,
}

// Terminal init options
#[repr(C)]
pub struct GhosttyTerminalOptions {
    pub cols: u16,
    pub rows: u16,
    pub max_scrollback: usize,
}

// Borrowed byte string
#[repr(C)]
pub struct GhosttyString {
    pub ptr: *const u8,
    pub len: usize,
}

// Terminal data query enum (subset)
#[repr(C)]
pub enum GhosttyTerminalData {
    Invalid = 0,
    Cols = 1,
    Rows = 2,
    CursorX = 3,
    CursorY = 4,
    CursorPendingWrap = 5,
    ActiveScreen = 6,
    CursorVisible = 7,
    // ... (8 = KittyKeyboardFlags, 9 = Scrollbar, 10 = CursorStyle)
    Title = 12,
    Pwd = 13,
    TotalRows = 14,
    ScrollbackRows = 15,
    WidthPx = 16,
    HeightPx = 17,
}

// Terminal screen enum
#[repr(C)]
pub enum GhosttyTerminalScreen {
    Primary = 0,
    Alternate = 1,
}

// Color types
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GhosttyColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

// Render state dirty enum
#[repr(C)]
pub enum GhosttyRenderStateDirty {
    False = 0,
    Partial = 1,
    Full = 2,
}

// Render state cursor visual style
#[repr(C)]
pub enum GhosttyRenderStateCursorVisualStyle {
    Bar = 0,
    Block = 1,
    Underline = 2,
    BlockHollow = 3,
}

// Render state data query enum (subset)
#[repr(C)]
pub enum GhosttyRenderStateData {
    Invalid = 0,
    Cols = 1,
    Rows = 2,
    Dirty = 3,
    // 4 = RowIterator (VEI-37)
    ColorBackground = 5,
    ColorForeground = 6,
    ColorCursor = 7,
    ColorCursorHasValue = 8,
    // 9 = ColorPalette
    CursorVisualStyle = 10,
    CursorVisible = 11,
    CursorBlinking = 12,
    CursorPasswordInput = 13,
    CursorViewportHasValue = 14,
    CursorViewportX = 15,
    CursorViewportY = 16,
    CursorViewportWideTail = 17,
}

// Render state option enum
#[repr(C)]
pub enum GhosttyRenderStateOption {
    Dirty = 0,
}

// Render state colors (sized struct)
#[repr(C)]
pub struct GhosttyRenderStateColors {
    pub size: usize,
    pub background: GhosttyColorRgb,
    pub foreground: GhosttyColorRgb,
    pub cursor: GhosttyColorRgb,
    pub cursor_has_value: bool,
    pub palette: [GhosttyColorRgb; 256],
}

// extern "C" function declarations
extern "C" {
    // Terminal lifecycle
    pub fn ghostty_terminal_new(
        allocator: *const std::ffi::c_void,
        terminal: *mut GhosttyTerminal,
        options: GhosttyTerminalOptions,
    ) -> GhosttyResult;

    pub fn ghostty_terminal_free(terminal: GhosttyTerminal);

    pub fn ghostty_terminal_reset(terminal: GhosttyTerminal);

    pub fn ghostty_terminal_resize(
        terminal: GhosttyTerminal,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> GhosttyResult;

    pub fn ghostty_terminal_vt_write(
        terminal: GhosttyTerminal,
        data: *const u8,
        len: usize,
    );

    pub fn ghostty_terminal_get(
        terminal: GhosttyTerminal,
        data: GhosttyTerminalData,
        out: *mut std::ffi::c_void,
    ) -> GhosttyResult;

    // Render state lifecycle
    pub fn ghostty_render_state_new(
        allocator: *const std::ffi::c_void,
        state: *mut GhosttyRenderState,
    ) -> GhosttyResult;

    pub fn ghostty_render_state_free(state: GhosttyRenderState);

    pub fn ghostty_render_state_update(
        state: GhosttyRenderState,
        terminal: GhosttyTerminal,
    ) -> GhosttyResult;

    pub fn ghostty_render_state_get(
        state: GhosttyRenderState,
        data: GhosttyRenderStateData,
        out: *mut std::ffi::c_void,
    ) -> GhosttyResult;

    pub fn ghostty_render_state_set(
        state: GhosttyRenderState,
        option: GhosttyRenderStateOption,
        value: *const std::ffi::c_void,
    ) -> GhosttyResult;

    pub fn ghostty_render_state_colors_get(
        state: GhosttyRenderState,
        out_colors: *mut GhosttyRenderStateColors,
    ) -> GhosttyResult;
}
```

**Notes on enum repr:** All Ghostty C enums use `int` as the underlying type (with an `INT_MAX` sentinel). Using `#[repr(C)]` on the Rust side matches the C `int` ABI. We only declare the discriminants we use -- unknown values from the C side would be caught by the `GhosttyResult` error handling, not by exhaustive match.

**Test strategy:**

This unit has no independent tests -- it provides raw declarations consumed by Units 2 and 3. Correctness is verified transitively through the safe wrapper tests. However, a build-system smoke test ensures the crate compiles in both modes:

- Without libghosty: `cargo build -p veil-ghostty` succeeds, `no_libghosty` cfg is set
- With libghosty: `cargo build -p veil-ghostty` succeeds, library links

### Unit 2: Error type and `GhosttyResult` mapping

Map the C result codes to an idiomatic Rust error type.

**File:** `crates/veil-ghostty/src/error.rs`

**Types:**

```rust
/// Errors from libghosty FFI operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GhosttyError {
    /// Memory allocation failed inside libghosty.
    #[error("libghosty allocation failed")]
    OutOfMemory,

    /// An invalid value was passed to or returned from libghosty.
    #[error("invalid value in libghosty call")]
    InvalidValue,

    /// A provided buffer was too small.
    #[error("buffer too small for libghosty output")]
    OutOfSpace,

    /// The requested value has no value (e.g., unset optional color).
    #[error("requested value is not set")]
    NoValue,

    /// A panic was caught at the FFI boundary.
    #[error("panic caught at FFI boundary")]
    Panic,

    /// An unexpected/unknown result code was returned.
    #[error("unknown libghosty error code: {0}")]
    Unknown(i32),
}
```

**Conversion function** (internal, not public):

```rust
/// Convert a GhosttyResult to Result<(), GhosttyError>.
/// GHOSTTY_SUCCESS (0) maps to Ok(()), all others map to the
/// corresponding error variant.
pub(crate) fn check_result(result: ffi::GhosttyResult) -> Result<(), GhosttyError>
```

**Test strategy:**

Happy path:
- `Success` maps to `Ok(())`

Error cases:
- `OutOfMemory` maps to `Err(GhosttyError::OutOfMemory)`
- `InvalidValue` maps to `Err(GhosttyError::InvalidValue)`
- `OutOfSpace` maps to `Err(GhosttyError::OutOfSpace)`
- `NoValue` maps to `Err(GhosttyError::NoValue)`

Edge cases:
- Unknown result code (e.g., raw value 99) maps to `Err(GhosttyError::Unknown(99))`
- Error types implement `Display` with meaningful messages
- Error type is `Send + Sync` (for use across threads)

These tests do NOT require libghosty -- they test pure Rust enum conversion logic and are always compiled.

### Unit 3: Safe `Terminal` wrapper

The core safe abstraction over the raw terminal handle.

**File:** `crates/veil-ghostty/src/terminal.rs`

**Types:**

```rust
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
        Self {
            cols: 80,
            rows: 24,
            max_scrollback: 10_000,
        }
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
```

**Methods on `Terminal`:**

```rust
impl Terminal {
    /// Create a new terminal with the given configuration.
    ///
    /// Returns an error if allocation fails or dimensions are zero.
    pub fn new(config: TerminalConfig) -> Result<Self, GhosttyError>

    /// Write VT-encoded data to the terminal for processing.
    ///
    /// Feeds raw bytes through the VT parser, updating terminal state.
    /// This never fails -- malformed input is logged internally by
    /// libghosty but does not produce an error.
    pub fn write_vt(&mut self, data: &[u8])

    /// Resize the terminal to the given cell dimensions and pixel sizes.
    ///
    /// `cell_width_px` and `cell_height_px` are the pixel dimensions of
    /// a single cell, used for image protocols and size reports.
    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<(), GhosttyError>

    /// Perform a full terminal reset (RIS).
    ///
    /// Resets all state (modes, scrollback, screen contents) but
    /// preserves dimensions.
    pub fn reset(&mut self)

    /// Query the terminal width in cells.
    pub fn cols(&self) -> Result<u16, GhosttyError>

    /// Query the terminal height in cells.
    pub fn rows(&self) -> Result<u16, GhosttyError>

    /// Query the cursor column position (0-indexed).
    pub fn cursor_x(&self) -> Result<u16, GhosttyError>

    /// Query the cursor row position within the active area (0-indexed).
    pub fn cursor_y(&self) -> Result<u16, GhosttyError>

    /// Query whether the cursor is visible (DEC mode 25).
    pub fn cursor_visible(&self) -> Result<bool, GhosttyError>

    /// Query which screen is currently active.
    pub fn active_screen(&self) -> Result<Screen, GhosttyError>

    /// Query the terminal title (set via OSC 0/2).
    ///
    /// Returns an empty string if no title has been set. The returned
    /// string is a copy -- it remains valid after further terminal
    /// mutations.
    pub fn title(&self) -> Result<String, GhosttyError>

    /// Query the terminal's working directory (set via OSC 7).
    ///
    /// Returns an empty string if no pwd has been set. The returned
    /// string is a copy.
    pub fn pwd(&self) -> Result<String, GhosttyError>

    /// Returns the raw FFI handle. Used internally by RenderState.
    pub(crate) fn handle(&self) -> ffi::GhosttyTerminal
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // Safety: handle is valid, only freed once via Drop
        unsafe { ffi::ghostty_terminal_free(self.handle); }
    }
}

// Terminal is not Send/Sync because the underlying C handle may have
// thread-affinity requirements.
```

**Implementation notes:**

Each query method follows the same pattern:
1. Declare an output variable of the appropriate type.
2. Call `ghostty_terminal_get` via `catch_unwind`, passing the handle, the `GhosttyTerminalData` variant, and a pointer to the output as `*mut c_void`.
3. Map the result via `check_result`.
4. Return the output value.

For `title()` and `pwd()`, the C API returns a `GhosttyString` (borrowed pointer valid until next `vt_write` or `reset`). The Rust wrapper copies the bytes into an owned `String` immediately, so the caller doesn't need to worry about lifetime constraints.

**Validation:** `Terminal::new` validates that `cols > 0` and `rows > 0` before calling into C. Zero dimensions would make the C API return `GHOSTTY_INVALID_VALUE`, but we catch it early with a clear error message.

**Test strategy:**

All tests in this unit require libghosty and are gated with `#[cfg(not(no_libghosty))]`.

Happy path:
- Create terminal with default config: succeeds, cols=80, rows=24
- Create terminal with custom config: succeeds, dimensions match
- `write_vt` with ASCII text: no panic, cursor advances
- `write_vt` with escape sequences (e.g., `\x1b[2J` clear screen): cursor resets
- `resize` to larger dimensions: cols/rows update accordingly
- `resize` to smaller dimensions: cols/rows update accordingly
- `reset` clears terminal state: cursor returns to (0,0)
- `cursor_x`/`cursor_y` after writing text: position matches expected
- `cursor_visible` default: returns true
- `cursor_visible` after `\x1b[?25l` (hide cursor): returns false
- `cursor_visible` after `\x1b[?25h` (show cursor): returns true
- `active_screen` default: returns `Screen::Primary`
- `active_screen` after `\x1b[?1049h` (alt screen): returns `Screen::Alternate`
- `active_screen` after `\x1b[?1049l` (back to primary): returns `Screen::Primary`
- `title` default: returns empty string
- `title` after `\x1b]2;My Title\x07` (OSC 2): returns "My Title"
- `pwd` default: returns empty string
- `pwd` after `\x1b]7;file:///tmp\x07` (OSC 7): returns "file:///tmp"

Cursor movement tests:
- `\x1b[10;5H` (CUP): cursor at (4, 9) -- 0-indexed
- `\x1b[B` (cursor down): cursor_y increments
- `\x1b[C` (cursor forward): cursor_x increments
- `\x1b[A` (cursor up): cursor_y decrements
- `\x1b[D` (cursor back): cursor_x decrements

Error cases:
- Create terminal with cols=0: returns error
- Create terminal with rows=0: returns error
- Resize to cols=0: returns error
- Resize to rows=0: returns error

Edge cases:
- `write_vt` with empty data: no-op, no panic
- `write_vt` with very large data (1MB): no panic
- Multiple `reset` calls: idempotent
- Query after `reset`: cursor at (0,0), title empty, pwd empty
- Drop terminal: no double-free, no leak (verified by absence of crash)

### Unit 4: Safe `RenderState` wrapper

Wrapper for the render state, which tracks incremental rendering updates.

**File:** `crates/veil-ghostty/src/render_state.rs`

**Types:**

```rust
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
    pub r: u8,
    pub g: u8,
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
    handle: ffi::GhosttyRenderState,
}
```

**Methods on `RenderState`:**

```rust
impl RenderState {
    /// Create a new empty render state.
    pub fn new() -> Result<Self, GhosttyError>

    /// Update the render state from a terminal.
    ///
    /// This reads the terminal's current state and computes what has
    /// changed since the last update. The terminal must not be mutated
    /// concurrently with this call.
    pub fn update(&mut self, terminal: &mut Terminal) -> Result<(), GhosttyError>

    /// Query the current dirty state.
    pub fn dirty(&self) -> Result<DirtyState, GhosttyError>

    /// Set the dirty state (typically to `Clean` after rendering a frame).
    pub fn set_dirty(&mut self, state: DirtyState) -> Result<(), GhosttyError>

    /// Query the viewport width in cells.
    pub fn cols(&self) -> Result<u16, GhosttyError>

    /// Query the viewport height in cells.
    pub fn rows(&self) -> Result<u16, GhosttyError>

    /// Query the full cursor state.
    pub fn cursor(&self) -> Result<CursorState, GhosttyError>

    /// Query the render colors (background, foreground, optional cursor).
    pub fn colors(&self) -> Result<RenderColors, GhosttyError>
}

impl Drop for RenderState {
    fn drop(&mut self) {
        unsafe { ffi::ghostty_render_state_free(self.handle); }
    }
}
```

**Implementation notes:**

`cursor()` batches multiple `ghostty_render_state_get` calls to assemble the `CursorState` struct. It first checks `CursorViewportHasValue` to determine if x/y are meaningful.

`colors()` uses `ghostty_render_state_colors_get` with the sized-struct pattern: initialize `GhosttyRenderStateColors` with `size = std::mem::size_of::<GhosttyRenderStateColors>()`, call the function, then convert to the safe `RenderColors` type.

**Test strategy:**

All tests require libghosty and are gated with `#[cfg(not(no_libghosty))]`.

Happy path:
- Create render state: succeeds
- Update from a freshly created terminal: succeeds, dirty state is `Full`
- After update, `cols()`/`rows()` match the terminal's dimensions
- After update, cursor state reflects terminal cursor (position, visible=true)
- After writing text to terminal and updating: dirty state changes
- After `set_dirty(Clean)` with no further changes: dirty state is `Clean`
- Colors include default foreground and background values

Render state sync tests:
- Write text to terminal, update render state: cursor position matches terminal
- Hide cursor (`\x1b[?25l`), update: `cursor().visible` is false
- Show cursor, update: `cursor().visible` is true
- Resize terminal, update: render state cols/rows match new dimensions

Edge cases:
- Update with no changes since last update: dirty state is `Clean`
- Multiple sequential updates without changes: all return `Clean`
- Drop render state: no crash
- Create render state, drop without ever updating: no crash

### Unit 5: Module structure and public API

Wire the modules together and establish the public API surface.

**File:** `crates/veil-ghostty/src/lib.rs`

**Structure:**

```rust
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
pub use terminal::{Terminal, TerminalConfig, Screen};

#[cfg(not(no_libghosty))]
pub use render_state::{
    RenderState, DirtyState, CursorStyle, CursorState, Color, RenderColors,
};
```

**Cargo.toml changes:**

```toml
[dependencies]
veil-core = { path = "../veil-core" }
tracing.workspace = true
thiserror.workspace = true

[dev-dependencies]
# No new dev dependencies needed
```

**Test strategy:**

- Verify `cargo build -p veil-ghostty` succeeds without libghosty (cfg `no_libghosty`)
- Verify `GhosttyError` is available in the public API in both modes
- Verify `Terminal` and `RenderState` are available only when `no_libghosty` is NOT set
- Verify `cargo test -p veil-ghostty` passes in no-libghosty mode (runs error mapping tests only)
- Verify `cargo test -p veil-ghostty` passes with libghosty (runs all tests)
- Verify `cargo clippy -p veil-ghostty --all-targets -- -D warnings` passes in both modes

## Acceptance Criteria

1. `cargo build -p veil-ghostty` succeeds both with and without `libghostty-vt.a` present
2. `cargo test -p veil-ghostty` passes all tests
3. `cargo clippy --all-targets --all-features -- -D warnings` passes
4. `cargo fmt --check` passes
5. `Terminal::new` creates a terminal, `cols()`/`rows()` return correct dimensions
6. `Terminal::write_vt` processes VT escape sequences correctly (cursor moves, text appears)
7. `Terminal::resize` changes terminal dimensions
8. `Terminal::reset` restores terminal to initial state
9. `Terminal::cursor_x/cursor_y` track cursor position through escape sequences
10. `Terminal::cursor_visible` reflects DEC mode 25 changes
11. `Terminal::active_screen` detects primary/alternate screen switching
12. `Terminal::title` reads OSC 2 title changes
13. `Terminal::pwd` reads OSC 7 working directory changes
14. `RenderState::new` creates a render state, `update` syncs from terminal
15. `RenderState::dirty` accurately reports clean/partial/full dirty state
16. `RenderState::cursor` returns correct cursor position, visibility, and style
17. `RenderState::colors` returns foreground and background colors
18. `RenderState::cols/rows` match terminal dimensions after update
19. All `GhosttyResult` error codes map to descriptive `GhosttyError` variants
20. No `unsafe` code exists outside `crates/veil-ghostty/src/ffi.rs` and the private methods in `terminal.rs`/`render_state.rs`
21. Tests requiring libghosty are gated with `#[cfg(not(no_libghosty))]`
22. 26+ tests covering terminal creation, VT processing, escape sequences, cursor movement, OSC title/pwd, render state sync, error mapping

## Dependencies

**Build-time requirements (for full FFI tests):**

| Requirement | Purpose |
|-------------|---------|
| Zig 0.15.2 | Build libghostty-vt from Ghostty source |
| Ghostty source at `vendor/ghostty` | Git submodule providing C headers and Zig source |
| Pre-built `vendor/ghostty-lib/lib/libghostty-vt.a` | Static library linked into the crate |

**Crate dependencies (changes to Cargo.toml):**

| Dependency | Purpose |
|------------|---------|
| `thiserror` (workspace) | Derive `Error` for `GhosttyError` |

No other new dependencies. The `veil-core` and `tracing` dependencies already exist.

**New files:**

| File | Purpose |
|------|---------|
| `crates/veil-ghostty/src/ffi.rs` | Raw `extern "C"` declarations and C type definitions |
| `crates/veil-ghostty/src/error.rs` | `GhosttyError` enum and `check_result` converter |
| `crates/veil-ghostty/src/terminal.rs` | Safe `Terminal` wrapper |
| `crates/veil-ghostty/src/render_state.rs` | Safe `RenderState` wrapper |

**Modified files:**

| File | Changes |
|------|---------|
| `crates/veil-ghostty/src/lib.rs` | Replace skeleton with module structure and re-exports |
| `crates/veil-ghostty/build.rs` | Rewrite to locate static library and emit linker directives |
| `crates/veil-ghostty/Cargo.toml` | Add `thiserror` dependency |
| `.gitmodules` | Add `vendor/ghostty` submodule (if not already present) |
| `.gitignore` | Add `vendor/ghostty-lib/` (built artifacts) |

**Setup steps (one-time, manual):**

```bash
# 1. Add Ghostty as a git submodule
git submodule add https://github.com/ghostty-org/ghostty.git vendor/ghostty

# 2. Install Zig 0.15.2 (exact version required)
# On macOS with Nix: nix-shell -p zig_0_15

# 3. Build the VT library
cd vendor/ghostty && zig build -Demit-lib-vt=true -Doptimize=ReleaseFast

# 4. Copy the built library to the expected location
mkdir -p vendor/ghostty-lib/lib
cp vendor/ghostty/zig-out/lib/libghostty-vt.a vendor/ghostty-lib/lib/
```
