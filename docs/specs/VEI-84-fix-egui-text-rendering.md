# VEI-84: Fix Broken egui Sidebar Text Rendering

## Context

The egui sidebar renders its background, panels, and structural elements correctly, but text
is broken: labels show repeating "de" fragments instead of readable workspace/tab text. The
sidebar background color, panel borders, and scroll areas all render fine -- only text is
garbled.

### Symptom analysis

The repeating "de" pattern means egui's font atlas texture is being sampled, but every glyph
quad maps to the same small region of the atlas. This is consistent with a coordinate-space
mismatch: if the shader's `screen_size_in_points` uniform is wrong, all vertex positions
compress into a tiny region of clip space, causing glyph quads to overlap and show fragments
of whatever characters happen to land at atlas coordinates near (0, 0).

### Root cause: `pixels_per_point` on the first frame

egui-wgpu's shader (`egui.wgsl`) converts vertex positions to clip space using
`screen_size_in_points`, which is computed as:

```text
screen_size_in_points = size_in_pixels / pixels_per_point
```

The `ScreenDescriptor` in `EguiIntegration::render()` (line 112-113 of `egui_integration.rs`)
reads `pixels_per_point` from `self.ctx.pixels_per_point()`:

```rust
let pixels_per_point = self.ctx.pixels_per_point();
let screen_descriptor = egui_wgpu::ScreenDescriptor {
    size_in_pixels: surface_size,
    pixels_per_point,
};
```

The `EguiIntegration::new()` constructor passes `None` for `native_pixels_per_point` to
`egui_winit::State::new()` (line 32 of `egui_integration.rs`). This means the egui context
starts with the default `pixels_per_point = 1.0`.

On macOS Retina displays, the actual scale factor is 2.0. The `take_egui_input()` method
(in egui-winit, line 268) sets `native_pixels_per_point = Some(window.scale_factor() as f32)`
on each call, which eventually propagates to `ctx.pixels_per_point()`. However, there is a
timing issue:

1. `EguiIntegration::new()` creates the egui context with `pixels_per_point = 1.0`
2. `take_egui_input()` sets `native_pixels_per_point = 2.0` in the `RawInput`
3. `ctx.run_ui(raw_input, ...)` processes the input and updates `ctx.pixels_per_point()`
4. `EguiIntegration::render()` reads `ctx.pixels_per_point()` for the `ScreenDescriptor`

The critical question is whether `ctx.pixels_per_point()` returns the correct value (2.0 on
Retina) at step 4. In egui 0.34, `ctx.run()` / `ctx.run_ui()` processes the raw input
(including `native_pixels_per_point`) and updates the context's internal state. After `run_ui`
returns, `ctx.pixels_per_point()` should reflect the value from the latest `RawInput`.

However, there is a subtle issue: `ctx.run_ui()` creates a top-level `Ui` with
`max_rect = ctx.available_rect()`. The `available_rect()` is computed from `screen_rect` in
the `RawInput`. In `main.rs` line 470, the sidebar frame is run like this:

```rust
let raw_input = renderer.egui.take_raw_input(window);
let full_output = renderer.egui.ctx.run_ui(raw_input, |ui| {
    sidebar_response = veil_ui::sidebar::render_sidebar(ui, &self.app_state);
});
```

The `take_egui_input()` method computes `screen_rect` in logical points:

```rust
let screen_size_in_points = screen_size_in_pixels / pixels_per_point(&self.egui_ctx, window);
```

Where `pixels_per_point()` is `egui_zoom_factor * native_pixels_per_point`. On the first call,
if the context's zoom factor is 1.0 and `native_pixels_per_point` comes from
`window.scale_factor()` (2.0 on Retina), the `screen_rect` would be in correct logical points.

**The actual bug is more likely in the `ScreenDescriptor` construction.** The `surface_size`
passed to `EguiIntegration::render()` is `[self.config.width, self.config.height]` from the
wgpu surface configuration (line 556 of `renderer.rs`). These are physical pixel dimensions.
The `pixels_per_point` comes from `ctx.pixels_per_point()`.

If `pixels_per_point` is correct (2.0), then:
- `screen_size_in_points = [2560/2.0, 1600/2.0] = [1280, 800]` -- correct
- Vertex positions (in logical points) map correctly to clip space

If `pixels_per_point` is wrong (1.0), then:
- `screen_size_in_points = [2560/1.0, 1600/1.0] = [2560, 1600]` -- doubled
- Vertex positions (in logical points, e.g. 0-1280 range) map to only the top-left quadrant
- All text quads compress into a quarter of the screen, overlapping

### Secondary issues

1. **`predictable_texture_filtering: false`** (line 42 of `egui_integration.rs`): Uses
   hardware bilinear filtering which could cause subtle sampling artifacts on some GPUs, but
   is unlikely to cause the "de" pattern specifically.

2. **No explicit `set_pixels_per_point()` call**: The constructor never explicitly sets the
   initial `pixels_per_point` on the egui context to match the window's scale factor. It
   relies entirely on `take_egui_input()` propagating the value through `RawInput`, which
   means the first frame's `ctx.pixels_per_point()` may lag behind.

3. **Font pipeline DPI hardcoded to 96.0** (line 148 of `main.rs`): This affects Veil's
   terminal text rasterization, not egui's sidebar text. But it's a related DPI issue that
   should be noted for future work.

4. **`egui_winit::State::new()` passes `None` for `max_texture_side`**: This means egui's
   font atlas may be sized without regard for GPU limits. On most modern GPUs this is fine,
   but it's technically unbounded.

### What exists

**`EguiIntegration`** (`crates/veil/src/egui_integration.rs`):
- `new()` — creates `egui::Context::default()`, `egui_winit::State` with `None` for
  `native_pixels_per_point`, `egui_wgpu::Renderer` with default options (lines 22-47).
- `take_raw_input()` — delegates to `egui_winit::State::take_egui_input()` (lines 69-75).
- `render()` — reads `ctx.pixels_per_point()`, builds `ScreenDescriptor`, tessellates,
  uploads textures, renders (lines 98-154).

**`Renderer`** (`crates/veil/src/renderer.rs`):
- `render()` — passes `[self.config.width, self.config.height]` (physical pixels) to
  `egui.render()` (line 556).
- `resize()` — updates `self.config.width/height` and `self.size` (lines 441-453).

**`VeilApp`** (`crates/veil/src/main.rs`):
- `run_sidebar_frame()` — calls `take_raw_input(window)`, then `ctx.run_ui(raw_input, ...)`
  (lines 459-485).
- `window_event()` — forwards events to `renderer.egui.on_window_event()` (line 217).
- `Resized` handler — calls `renderer.resize(width, height)` with physical pixel sizes
  from winit (lines 228-233).

**egui-winit 0.34.1** (`~/.cargo/registry/`):
- `State::new()` — stores `native_pixels_per_point` in `egui_input.viewports` (line 164-168).
  When `None`, the viewport entry has no `native_pixels_per_point` set.
- `take_egui_input()` — sets `native_pixels_per_point = Some(window.scale_factor() as f32)`
  on every call (line 268). Computes `screen_rect` using `pixels_per_point()` which is
  `egui_zoom_factor * native_pixels_per_point` (lines 253-259).
- `pixels_per_point()` — returns `egui_zoom_factor * native_pixels_per_point` (lines 51-54).

**egui-wgpu 0.34.1** (`~/.cargo/registry/`):
- `ScreenDescriptor::screen_size_in_points()` — divides `size_in_pixels` by
  `pixels_per_point` (lines 133-137).
- `update_buffers()` — writes `screen_size_in_points` to the uniform buffer (lines 912-918).
- `Renderer::new()` — selects fragment shader based on `output_color_format.is_srgb()`.
  `Bgra8Unorm` (typical macOS Metal) is NOT sRGB, so `fs_main_gamma_framebuffer` is selected.

### What's missing

1. **No explicit scale factor initialization** — The egui context starts with
   `pixels_per_point = 1.0` (the `egui::Context::default()`). The constructor does not call
   `ctx.set_pixels_per_point(window.scale_factor() as f32)` to set the correct initial value.

2. **No diagnostic logging** — There is no tracing output for `pixels_per_point`,
   `screen_size_in_points`, or `surface_size`, making it impossible to diagnose rendering
   issues from logs.

3. **No validation** — The `ScreenDescriptor` is constructed without any sanity checks
   (e.g., `pixels_per_point > 0`, `size_in_pixels` matches actual surface dimensions).

## Implementation units

### Unit 1: Initialize `pixels_per_point` at construction

**File:** `crates/veil/src/egui_integration.rs`

**Change:** In `EguiIntegration::new()`, pass the window's scale factor as
`native_pixels_per_point` to `egui_winit::State::new()` instead of `None`. Also call
`ctx.set_pixels_per_point(window.scale_factor() as f32)` immediately after creating the
context, so that `ctx.pixels_per_point()` returns the correct value from the very first frame.

```rust
pub fn new(
    window: &winit::window::Window,
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
) -> Self {
    let ctx = egui::Context::default();
    let native_ppp = window.scale_factor() as f32;
    ctx.set_pixels_per_point(native_ppp);
    let winit_state = egui_winit::State::new(
        ctx.clone(),
        ctx.viewport_id(),
        window,
        Some(native_ppp),   // was None
        None,
        None,
    );
    // ...
}
```

**Rationale:** This ensures the first frame's `ScreenDescriptor` has the correct
`pixels_per_point`, preventing the coordinate-space mismatch that causes compressed/overlapping
text. Subsequent frames continue to get the correct value from `take_egui_input()`.

### Unit 2: Add `ScreenDescriptor` validation and diagnostic logging

**File:** `crates/veil/src/egui_integration.rs`

**Change:** Add `tracing::debug!` calls in `render()` to log the `pixels_per_point`,
`surface_size`, and computed `screen_size_in_points`. Add a debug assertion that
`pixels_per_point > 0.0`.

```rust
pub fn render(
    &mut self,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    surface_size: [u32; 2],
    full_output: egui::FullOutput,
) {
    let Some(wgpu_renderer) = &mut self.wgpu_renderer else {
        return;
    };

    let pixels_per_point = self.ctx.pixels_per_point();
    debug_assert!(
        pixels_per_point > 0.0,
        "pixels_per_point must be positive, got {pixels_per_point}"
    );

    tracing::debug!(
        pixels_per_point,
        surface_width = surface_size[0],
        surface_height = surface_size[1],
        screen_points_w = surface_size[0] as f32 / pixels_per_point,
        screen_points_h = surface_size[1] as f32 / pixels_per_point,
        "egui ScreenDescriptor"
    );

    // ... rest unchanged
}
```

**Rationale:** Diagnostic logging makes it possible to confirm the fix is working and to
diagnose any future rendering issues from the log output alone. The `debug_assert!` catches
invalid states during development.

### Unit 3: Set `predictable_texture_filtering` to `true`

**File:** `crates/veil/src/egui_integration.rs`

**Change:** In the `egui_wgpu::RendererOptions`, set `predictable_texture_filtering: true`.

```rust
let wgpu_renderer = egui_wgpu::Renderer::new(
    device,
    surface_format,
    egui_wgpu::RendererOptions {
        msaa_samples: 1,
        depth_stencil_format: None,
        dithering: false,
        predictable_texture_filtering: true,  // was false
    },
);
```

**Rationale:** When `false`, egui-wgpu uses hardware bilinear filtering which can vary across
GPU vendors and driver versions. Setting `true` enables manual nearest-neighbor filtering in
the shader, producing pixel-perfect glyph rendering that is consistent across all platforms.
This is especially important for a terminal application where text clarity is critical.

### Unit 4: Pass `max_texture_side` from GPU device limits

**File:** `crates/veil/src/egui_integration.rs`

**Change:** Accept `device: &wgpu::Device` and query `device.limits().max_texture_dimension_2d`
to pass as `max_texture_side` to `egui_winit::State::new()`.

```rust
pub fn new(
    window: &winit::window::Window,
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
) -> Self {
    let ctx = egui::Context::default();
    let native_ppp = window.scale_factor() as f32;
    ctx.set_pixels_per_point(native_ppp);
    let max_texture_side = device.limits().max_texture_dimension_2d as usize;
    let winit_state = egui_winit::State::new(
        ctx.clone(),
        ctx.viewport_id(),
        window,
        Some(native_ppp),
        None,
        Some(max_texture_side),
    );
    // ...
}
```

**Rationale:** Tells egui the GPU's actual maximum texture dimension so it can split its font
atlas across multiple textures if needed, rather than creating a texture that exceeds GPU
limits. This prevents potential rendering failures on GPUs with smaller texture size limits.

### Unit 5: Handle `ScaleFactorChanged` event for mid-session DPI changes

**File:** `crates/veil/src/main.rs`

**Change:** In `window_event()`, add handling for `WindowEvent::ScaleFactorChanged` to update
`ctx.set_pixels_per_point()` and trigger a surface reconfiguration. The `egui_winit::State`
already handles this event internally (setting `native_pixels_per_point` in the viewport info),
but the wgpu surface configuration and Veil's own `window_size` tracking need updating too.

Note: winit 0.31+ reports `ScaleFactorChanged` with a `scale_factor` field, and the inner size
may change. The existing `Resized` handler already handles the surface reconfiguration, so this
unit only needs to ensure `pixels_per_point` is updated on the egui context immediately.

```rust
WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
    if let Some(renderer) = &mut self.renderer {
        renderer.egui.ctx.set_pixels_per_point(scale_factor as f32);
    }
}
```

**Rationale:** When a window is dragged between displays with different DPI (e.g., Retina to
external 1x monitor), the scale factor changes mid-session. Without this, `pixels_per_point`
would only update on the next `take_egui_input()` call, potentially causing one frame of
garbled text during the transition.

## Test strategy

### Unit 1 tests

These tests verify the initial `pixels_per_point` is correctly set from the window's scale
factor. Since `EguiIntegration::new()` requires a real window and GPU device, these tests
use the existing headless test helper pattern.

1. **`headless_context_default_pixels_per_point`**: Verify that a headless
   `EguiIntegration` (test-only constructor) has `pixels_per_point() == 1.0` as the baseline.
   This confirms the default behavior before the fix.

2. **`pixels_per_point_propagates_through_raw_input`**: Create a headless integration, run a
   frame with `RawInput` that has `native_pixels_per_point = Some(2.0)` in its viewport info,
   then verify `ctx.pixels_per_point()` returns 2.0 after `run_ui`. This confirms the
   propagation path works.

3. **`screen_descriptor_uses_correct_pixels_per_point`**: After setting `pixels_per_point` on
   the context, construct a `ScreenDescriptor` the same way the render method does and verify
   `screen_size_in_points()` returns `size_in_pixels / pixels_per_point`.

### Unit 2 tests

Diagnostic logging is verified by inspection (run `RUST_LOG=debug cargo run` and check output).
The `debug_assert!` is verified by existing tests -- if `pixels_per_point` were ever 0 or
negative, tests would panic in debug builds.

### Unit 3 tests

`predictable_texture_filtering` is a flag passed to egui-wgpu at construction time. Its effect
is internal to the egui-wgpu shader. No unit test is needed for this flag change -- the visual
result is verified by the acceptance criteria (manual inspection that text renders without
filtering artifacts).

### Unit 4 tests

1. **`max_texture_side_is_positive`**: In the existing headless constructor, verify the concept
   that `device.limits().max_texture_dimension_2d` is a positive value. Since headless tests
   don't have a device, this is verified by the integration test: run `cargo run` and confirm
   no texture-size-exceeded errors in the log.

### Unit 5 tests

1. **`set_pixels_per_point_updates_context`**: Call `ctx.set_pixels_per_point(3.0)` on a
   headless context and verify `ctx.pixels_per_point()` returns 3.0. This confirms the API
   works as expected.

### Integration verification

Run `cargo run` on a macOS Retina display and verify:
- Sidebar text labels ("Workspaces", "Conversations", workspace names) render clearly
- Text is correctly sized (not compressed or doubled)
- Moving the window between displays with different DPI updates text correctly

## Acceptance criteria

1. Sidebar text renders as readable labels on macOS Retina (2x) displays -- no "de" fragments,
   no overlapping glyphs, no compressed text.
2. Sidebar text renders correctly on standard (1x) displays.
3. `EguiIntegration::new()` initializes `pixels_per_point` from the window's actual scale
   factor, not the egui default of 1.0.
4. `egui_winit::State::new()` receives `Some(native_pixels_per_point)` and
   `Some(max_texture_side)` instead of `None`.
5. `ScreenDescriptor` uses the correct `pixels_per_point` from the first frame onward.
6. `predictable_texture_filtering` is `true` for consistent cross-platform text rendering.
7. Diagnostic logging at `DEBUG` level outputs `pixels_per_point`, surface dimensions, and
   computed `screen_size_in_points` on each egui render pass.
8. All existing tests pass (`cargo test`).
9. Quality gate passes (`cargo fmt`, `cargo clippy`, `cargo test`, `cargo build`).

## Dependencies

- **VEI-78** (wire egui sidebar) -- completed. Provides the `EguiIntegration` struct and
  sidebar rendering pipeline that this task modifies.
- **egui 0.34.1**, **egui-wgpu 0.34.1**, **egui-winit 0.34.1** -- no version changes needed.
  The fix uses existing APIs (`ctx.set_pixels_per_point()`, `State::new()` with
  `native_pixels_per_point`, `device.limits()`).
- **wgpu 29.0.1** -- no version changes needed. `device.limits().max_texture_dimension_2d` is
  a stable API.
