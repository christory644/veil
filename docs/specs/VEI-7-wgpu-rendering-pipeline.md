# VEI-7: wgpu Rendering Pipeline

## Context

Veil needs a GPU rendering pipeline that converts application state into pixels on screen. This is the visual backbone -- without it, the app shows a blank window. The renderer consumes `AppState` (workspaces, pane layouts, focus state, sidebar visibility) and draws colored rectangles for cell backgrounds, cursors, pane dividers, and focus borders. Text rendering (VEI-8) and egui sidebar integration (VEI-12) are follow-up issues; this task delivers the geometric foundation they build on.

The renderer lives in the `crates/veil/` binary crate per AGENTS.md ("Rendering layer lives in veil binary"). It depends on `wgpu` for GPU abstraction (Metal/Vulkan/DX12/OpenGL auto-selection) and `bytemuck` for safe vertex buffer casting. It integrates with the existing winit event loop in `main.rs`, consuming `compute_layout()` output from `veil-core` to position pane geometry.

### What already exists

- **`veil-core::layout`** -- `Rect`, `PaneLayout`, `compute_layout(root, available, zoomed_pane)` that produces pixel rectangles from the `PaneNode` tree
- **`veil-core::state::AppState`** -- central state with `workspaces`, `active_workspace_id`, `sidebar` (visible/width_px)
- **`veil-core::workspace::Workspace`** -- `layout` (PaneNode tree), `zoomed_pane` (Option<PaneId>)
- **`veil-core::focus::FocusManager`** -- tracks which surface has keyboard focus via `focused_surface()`
- **`crates/veil/src/main.rs`** -- winit `ApplicationHandler` with `VeilApp`, handling `Resumed`, `Resized`, `RedrawRequested`, `CloseRequested`
- **`veil-ghostty::RenderState`** -- exposes `CursorState` (position, style, visibility), `RenderColors` (background, foreground), `cols()/rows()` for grid dimensions

### Key design decisions

**Pixel-coordinate vertices, clip-space conversion in shader.** Vertex positions are specified in pixel coordinates (matching `Rect` output from `compute_layout`). The WGSL shader converts to clip space using a uniform buffer containing window dimensions. This avoids the quad generation code needing to know about NDC, keeps the CPU-side math in intuitive units, and makes debugging straightforward.

**Single render pipeline for all geometry.** Cell backgrounds, cursors, dividers, and focus borders are all solid-color quads. A single pipeline with position+color vertices and alpha blending handles everything. Text (VEI-8) will add a second pipeline with texture sampling.

**Frame geometry built fresh each frame.** No incremental diffing yet (that is explicitly out of scope). Each frame, `build_frame_geometry` walks the `AppState`, generates all quads, uploads vertices and indices, and draws. This is simple, correct, and fast enough for the quad counts involved (a few hundred quads at most).

**Sidebar offset, not sidebar rendering.** The sidebar is rendered by egui (VEI-12). The renderer accounts for it by offsetting the terminal area: when the sidebar is visible, the terminal area starts at `x = sidebar.width_px` instead of `x = 0`. The sidebar region itself is left for egui to fill.

## Implementation Units

### Unit 1: Vertex type and quad generation helpers

Pure data types and geometry math. No GPU dependencies. Fully unit-testable.

**File:** `crates/veil/src/vertex.rs`

**Types:**

```rust
/// A vertex with position (2D) and RGBA color.
/// 24 bytes: 2 * f32 (position) + 4 * f32 (color) = 24 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}
```

**Functions:**

```rust
/// Generate the 4 vertices for an axis-aligned quad.
///
/// Vertices are in pixel coordinates, ordered:
/// 0: top-left, 1: top-right, 2: bottom-left, 3: bottom-right
pub fn quad_vertices(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> [Vertex; 4]

/// Generate the 6 index values for a quad (two triangles),
/// offset by `base` to allow batching multiple quads into one
/// index buffer.
///
/// Triangles: (base+0, base+2, base+1), (base+1, base+2, base+3)
pub fn quad_indices(base: u16) -> [u16; 6]

/// Compute the vertex base index for the Nth quad in a batch.
/// Each quad uses 4 vertices, so quad N starts at vertex index N * 4.
pub fn vertex_base(quad_index: usize) -> u16
```

**Test strategy:**

Happy path:
- `quad_vertices` produces 4 vertices with correct positions and uniform color
- `quad_indices(0)` produces `[0, 2, 1, 1, 2, 3]`
- `quad_indices(4)` produces `[4, 6, 5, 5, 6, 7]` (offset by one quad)
- `vertex_base(0)` returns 0, `vertex_base(1)` returns 4, `vertex_base(5)` returns 20
- `Vertex` is 24 bytes (`std::mem::size_of::<Vertex>() == 24`)
- `Vertex` satisfies `bytemuck::Pod` and `Zeroable` (compile-time -- the derives enforce this)

Edge cases:
- `quad_vertices` with zero width/height: produces degenerate quad (top-left == top-right, etc.), no panic
- `quad_vertices` with negative coordinates: valid (produces correct positions)
- `vertex_base` with large index: produces correct value (no overflow for reasonable quad counts; document u16 limit of 16383 quads)

### Unit 2: Quad builders -- cell backgrounds, cursor, dividers, focus border

Functions that consume layout geometry (from `veil-core`) and produce `Vertex`/index arrays. Pure geometry, no GPU state. Fully unit-testable.

**File:** `crates/veil/src/quad_builder.rs`

**Types:**

```rust
/// Parameters for building cell background quads for a pane.
pub struct CellGridParams {
    /// The pane's pixel rectangle (from compute_layout).
    pub rect: Rect,
    /// Number of columns in the terminal grid.
    pub cols: u16,
    /// Number of rows in the terminal grid.
    pub rows: u16,
    /// Background color as RGBA.
    pub bg_color: [f32; 4],
}
```

**Functions:**

```rust
/// Build cell background quads for a single pane.
///
/// Generates one quad per cell (cols * rows quads total). Each cell
/// is sized to evenly fill the pane rect. All cells use the same
/// background color (real per-cell colors are a follow-up).
///
/// Returns (vertices, indices) ready for GPU upload.
pub fn build_cell_background_quads(params: &CellGridParams) -> (Vec<Vertex>, Vec<u16>)

/// Build a cursor quad at the given grid position within a pane rect.
///
/// `col` and `row` are zero-indexed grid positions. The cursor quad
/// occupies the cell at (col, row) with the given color.
///
/// Returns (vertices, indices) for a single quad.
pub fn build_cursor_quad(
    rect: &Rect,
    cols: u16,
    rows: u16,
    col: u16,
    row: u16,
    color: [f32; 4],
) -> (Vec<Vertex>, Vec<u16>)

/// Build divider quads between adjacent pane edges.
///
/// Examines all pairs of pane layouts and, where two panes share
/// an edge (within a tolerance), generates a thin line quad (1px
/// wide for vertical dividers, 1px tall for horizontal dividers).
///
/// Returns (vertices, indices) for all divider quads.
pub fn build_divider_quads(
    pane_layouts: &[PaneLayout],
    divider_color: [f32; 4],
) -> (Vec<Vertex>, Vec<u16>)

/// Build focus border quads around a pane rect.
///
/// Generates 4 quads (top, bottom, left, right edges) forming a
/// border of the given thickness around the pane rect.
///
/// Returns (vertices, indices) for the 4 border quads.
pub fn build_focus_border(
    rect: &Rect,
    border_thickness: f32,
    color: [f32; 4],
) -> (Vec<Vertex>, Vec<u16>)
```

**Test strategy:**

`build_cell_background_quads`:
- Happy path: 80x24 grid in an 800x600 rect produces 1920 quads (80*24), each cell 10x25 pixels
- Vertex count: 4 * cols * rows; index count: 6 * cols * rows
- All vertices fall within the pane rect bounds
- Adjacent cells share edges (no gaps, no overlaps)
- 1x1 grid produces a single quad filling the rect

Edge cases:
- Zero cols or zero rows: returns empty vectors
- Non-integer cell size (e.g., 800px / 3 cols): cells tile correctly, last cell extends to edge

`build_cursor_quad`:
- Happy path: cursor at (0, 0) in 80x24 grid, 800x600 rect -- quad at top-left cell position
- Cursor at (79, 23) -- quad at bottom-right cell position
- Returns exactly 4 vertices and 6 indices

Edge cases:
- Cursor at col >= cols or row >= rows: clamp to last valid cell (defensive)
- Zero cols or rows: returns empty vectors

`build_divider_quads`:
- Happy path: two panes side by side (left edge of right pane == right edge of left pane) -- one vertical divider
- Two panes stacked (bottom edge of top == top edge of bottom) -- one horizontal divider
- Four-pane grid -- 4 divider segments (2 vertical, 2 horizontal)
- Single pane: no dividers (empty result)

Edge cases:
- Panes with gap between them (not adjacent): no divider generated
- Zoomed pane (only 1 layout): no dividers
- Empty pane list: returns empty vectors

`build_focus_border`:
- Happy path: border around 400x300 rect with 2px thickness -- 4 quads, each 2px on one dimension
- All border quads are within or touching the rect boundary (inset border)
- Returns exactly 16 vertices (4 quads * 4 vertices) and 24 indices (4 * 6)

Edge cases:
- Border thickness larger than half the rect dimension: clamp to avoid overlap
- Zero-size rect: returns degenerate quads, no panic
- Zero thickness: returns zero-area quads

### Unit 3: Frame geometry composition

Combines all quad builders with `AppState` to produce the full frame's geometry. Pure logic (no GPU), but requires `AppState` and `FocusManager` as inputs.

**File:** `crates/veil/src/frame.rs`

**Types:**

```rust
/// Complete geometry for a single frame, ready for GPU upload.
pub struct FrameGeometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub clear_color: wgpu::Color,
}
```

**Functions:**

```rust
/// Build all geometry for the current frame.
///
/// This is the main composition function called once per frame:
/// 1. Compute available terminal area (window size minus sidebar if visible)
/// 2. Get active workspace layout via compute_layout (respecting zoom)
/// 3. Build cell background quads for each pane
/// 4. Build cursor quad for the focused pane (if cursor visible)
/// 5. Build divider quads between adjacent panes
/// 6. Build focus border around the focused pane
/// 7. Concatenate all vertices/indices with correct base offsets
///
/// Returns FrameGeometry ready for a single draw call.
pub fn build_frame_geometry(
    app_state: &AppState,
    focus: &FocusManager,
    window_width: u32,
    window_height: u32,
) -> FrameGeometry
```

**Sidebar offset logic:**

```
if app_state.sidebar.visible {
    terminal_x = app_state.sidebar.width_px as f32;
    terminal_width = window_width as f32 - terminal_x;
} else {
    terminal_x = 0.0;
    terminal_width = window_width as f32;
}
```

The `available` rect passed to `compute_layout` is `Rect { x: terminal_x, y: 0.0, width: terminal_width, height: window_height as f32 }`.

**Default colors (used until real terminal cell colors are implemented):**

- Cell background: `[0.1, 0.1, 0.1, 1.0]` (dark gray)
- Cursor: `[0.9, 0.9, 0.9, 1.0]` (light gray)
- Divider: `[0.3, 0.3, 0.3, 1.0]` (medium gray)
- Focus border: `[0.2, 0.5, 1.0, 0.8]` (blue with slight transparency)
- Clear color (window background): `[0.05, 0.05, 0.05, 1.0]` (near black)

**Test strategy:**

Happy path:
- Single pane, no zoom, sidebar visible: geometry contains cell bg quads offset by sidebar width, no dividers, focus border around the single pane
- Single pane, sidebar hidden: cell bg quads start at x=0, full width
- Two panes horizontal split, one focused: geometry contains cell bg quads for both panes, divider between them, focus border around the focused pane
- Zoomed pane: single pane fills terminal area, no dividers
- No active workspace: returns geometry with only clear color (empty vertices/indices)

Edge cases:
- Window width smaller than sidebar width: terminal_width clamps to 0, no quads generated
- No focused surface: no cursor quad, no focus border
- Active workspace with no panes (impossible in current data model, but defensive): empty geometry

Integration with existing types:
- Verify sidebar width of 250px (from `AppState::new()` default) offsets correctly
- Verify `compute_layout` zoom path produces single-pane geometry

### Unit 4: WGSL shader

A single shader that converts pixel-coordinate vertices to clip space and passes color through.

**File:** `crates/veil/src/shader.wgsl`

```wgsl
struct WindowUniform {
    width: f32,
    height: f32,
};

@group(0) @binding(0)
var<uniform> window: WindowUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Convert pixel coords to clip space: [0, width] -> [-1, 1], [0, height] -> [1, -1]
    // Y is flipped because wgpu clip space has Y up, but pixel coords have Y down.
    let clip_x = (in.position.x / window.width) * 2.0 - 1.0;
    let clip_y = 1.0 - (in.position.y / window.height) * 2.0;
    out.clip_position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
```

**Test strategy:**

The shader itself is not unit-testable in Rust (it is compiled by wgpu at runtime). Testing is structural:
- Verify the file is valid WGSL by including it via `include_str!` and attempting `device.create_shader_module` in a GPU integration test (if GPU available)
- Verify the uniform struct size matches the Rust-side `WindowUniform` (8 bytes)
- The coordinate conversion math is verified indirectly via visual correctness in integration/manual testing

### Unit 5: Renderer struct -- GPU initialization, resize, render

The core `Renderer` struct that owns wgpu state and orchestrates rendering. This is the GPU-facing code.

**File:** `crates/veil/src/renderer.rs`

**Types:**

```rust
/// Uniform buffer data matching the WGSL WindowUniform struct.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WindowUniform {
    width: f32,
    height: f32,
}

/// GPU renderer for Veil's terminal UI.
///
/// Owns all wgpu state: device, queue, surface, pipeline, buffers.
/// Created once at startup, resized on window resize, renders each frame.
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    window_uniform_buffer: wgpu::Buffer,
    window_bind_group: wgpu::BindGroup,
    size: (u32, u32),
}
```

**Methods:**

```rust
impl Renderer {
    /// Initialize the renderer with a window.
    ///
    /// Creates the wgpu instance with backend auto-selection, requests
    /// adapter and device, configures the surface, creates the shader
    /// module, pipeline layout, render pipeline, and uniform buffer.
    ///
    /// This is async because wgpu adapter/device requests are async.
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self>

    /// Handle window resize.
    ///
    /// Updates the surface configuration and uniform buffer with new
    /// dimensions. Called from winit's Resized event handler.
    pub fn resize(&mut self, width: u32, height: u32)

    /// Render a frame.
    ///
    /// 1. Get next surface texture
    /// 2. Create texture view
    /// 3. Build command encoder
    /// 4. Begin render pass with clear color
    /// 5. Set pipeline, bind group
    /// 6. Upload vertex/index buffers from FrameGeometry
    /// 7. Draw indexed
    /// 8. Submit and present
    ///
    /// Handles surface errors:
    /// - Lost: calls resize() to reconfigure
    /// - OutOfMemory: logs error and returns Err (caller should exit)
    pub fn render(&mut self, frame_geometry: &FrameGeometry) -> anyhow::Result<()>

    /// Get the current surface size.
    pub fn size(&self) -> (u32, u32)
}
```

**wgpu initialization details:**

```rust
// Instance with all backends
let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
    backends: wgpu::Backends::all(),
    ..Default::default()
});

// Surface from window
let surface = instance.create_surface(window)?;

// Adapter with default power preference
let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
    power_preference: wgpu::PowerPreference::default(),
    compatible_surface: Some(&surface),
    force_fallback_adapter: false,
}).await.context("no suitable GPU adapter")?;

// Device with defaults
let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await?;
```

**Pipeline configuration:**

- Vertex buffer layout: stride 24 bytes, 2 attributes (position: Float32x2 at offset 0, color: Float32x4 at offset 8)
- Fragment target: surface format, alpha blending enabled (`BlendState::ALPHA_BLENDING`)
- Primitive: triangle list, front face CCW, no culling (quads rendered as two triangles)
- No depth/stencil (2D rendering, draw order handles layering)

**Vertex/index buffer strategy:**

Created per-frame with `wgpu::util::DeviceExt::create_buffer_init` from `FrameGeometry` data. This is simple and correct. For future optimization, persistent buffers with sub-allocation can replace this, but per-frame creation is fine for the quad counts involved here (< 10K quads).

**Test strategy:**

The `Renderer` struct requires a GPU and a real window, making it hard to unit test in CI. Testing strategy:

Structural tests (no GPU needed):
- `WindowUniform` is 8 bytes (`size_of`)
- `WindowUniform` satisfies `Pod` and `Zeroable` (compile-time)
- Vertex buffer layout descriptor matches `Vertex` struct layout (stride == 24, offsets correct)

Integration tests (require GPU, can be `#[ignore]` in CI):
- `Renderer::new` succeeds with a real window (validates adapter/device creation)
- `Renderer::resize` updates stored size
- `Renderer::render` with empty geometry (0 vertices/indices) completes without error
- `Renderer::render` with a simple quad completes without error

### Unit 6: winit integration -- wiring the renderer into the event loop

Modify `main.rs` to create and use the `Renderer`. This is the integration point.

**File:** `crates/veil/src/main.rs` (modify existing)

**Changes to `VeilApp`:**

```rust
struct VeilApp {
    window: Option<Arc<Window>>,  // Changed to Arc<Window> for wgpu surface
    renderer: Option<Renderer>,   // New: created in resumed()
    app_state: AppState,          // Rename from _app_state, now used
    _channels: Channels,
    shutdown: ShutdownSignal,
    _keybindings: KeybindingRegistry,
    focus: FocusManager,          // Rename from _focus, now used
    window_size: (u32, u32),
}
```

**Event handler changes:**

`resumed`:
```rust
fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    if self.window.is_some() { return; }
    let window = Arc::new(event_loop.create_window(attrs)?);
    let renderer = pollster::block_on(Renderer::new(window.clone()))?;
    self.renderer = Some(renderer);
    self.window = Some(window);
}
```

`Resized`:
```rust
WindowEvent::Resized(new_size) => {
    self.window_size = (new_size.width, new_size.height);
    if let Some(renderer) = &mut self.renderer {
        renderer.resize(new_size.width, new_size.height);
    }
}
```

`RedrawRequested`:
```rust
WindowEvent::RedrawRequested => {
    let frame_geometry = build_frame_geometry(
        &self.app_state,
        &self.focus,
        self.window_size.0,
        self.window_size.1,
    );
    if let Some(renderer) = &mut self.renderer {
        match renderer.render(&frame_geometry) {
            Ok(()) => {}
            Err(e) => {
                tracing::error!("render error: {e}");
                event_loop.exit();
            }
        }
    }
    if let Some(window) = &self.window {
        window.request_redraw();
    }
}
```

**New dependencies for the event loop:**

- `pollster` for blocking on the async `Renderer::new` in the sync `resumed` callback
- `Arc<Window>` because wgpu `Surface` requires `'static` window reference

**Test strategy:**

This unit is inherently integration-level (it wires together winit + wgpu + app state). Testing is via:
- `cargo build` succeeds (compilation is the primary gate)
- Manual smoke test: `cargo run` shows a dark window with no crashes
- Resize: window resize does not crash
- The individual components (frame geometry, renderer, quad builders) are tested in their own units

## Acceptance Criteria

1. `cargo build -p veil` succeeds with wgpu and bytemuck dependencies
2. `cargo test -p veil` passes all tests (vertex, quad builder, frame geometry tests)
3. `cargo clippy --all-targets --all-features -- -D warnings` passes
4. `cargo fmt --check` passes
5. `cargo run` opens a window and renders colored rectangles (dark background, no crash)
6. Window resize reconfigures the surface and re-renders correctly
7. `Vertex` type is 24 bytes with `Pod`/`Zeroable` derives
8. Shader converts pixel coordinates to clip space correctly (quads appear at expected positions)
9. Sidebar offset: when sidebar is visible (default), terminal area is shifted right by 250px
10. Zoomed pane: a single pane fills the entire terminal area with no dividers
11. Focus border: the focused pane has a visible colored border
12. Dividers: thin lines appear between adjacent panes
13. Surface error recovery: `SurfaceError::Lost` triggers resize, `OutOfMemory` triggers exit
14. All quad generation functions handle edge cases (zero dimensions, empty inputs) without panics

## Dependencies

**New workspace dependencies (add to root `Cargo.toml`):**

| Crate | Version | Purpose |
|-------|---------|---------|
| `wgpu` | `24` | GPU abstraction (Metal/Vulkan/DX12/OpenGL) |
| `bytemuck` | `1` | Safe casting for vertex data (`Pod`, `Zeroable` derives) |
| `pollster` | `0.4` | Block on async wgpu initialization in sync winit callback |

**Add to `Cargo.toml` `[workspace.dependencies]`:**

```toml
wgpu = "24"
bytemuck = { version = "1", features = ["derive"] }
pollster = "0.4"
```

**Add to `crates/veil/Cargo.toml` `[dependencies]`:**

```toml
wgpu = { workspace = true }
bytemuck = { workspace = true }
pollster = { workspace = true }
```

**New files:**

| File | Purpose |
|------|---------|
| `crates/veil/src/vertex.rs` | `Vertex` type, `quad_vertices`, `quad_indices`, `vertex_base` |
| `crates/veil/src/quad_builder.rs` | `CellGridParams`, `build_cell_background_quads`, `build_cursor_quad`, `build_divider_quads`, `build_focus_border` |
| `crates/veil/src/frame.rs` | `FrameGeometry`, `build_frame_geometry` |
| `crates/veil/src/shader.wgsl` | WGSL vertex/fragment shader |
| `crates/veil/src/renderer.rs` | `Renderer` struct with `new`, `resize`, `render`, `size` |

**Modified files:**

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `wgpu`, `bytemuck`, `pollster` to workspace dependencies |
| `crates/veil/Cargo.toml` | Add `wgpu`, `bytemuck`, `pollster` to dependencies |
| `crates/veil/src/main.rs` | Wire `Renderer` into `VeilApp`, use `Arc<Window>`, call `build_frame_geometry` + `render` in `RedrawRequested` |
