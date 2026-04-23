# VEI-78: Wire egui Sidebar into wgpu Render Pass

## Context

The egui sidebar exists as pure UI logic in `veil-ui` (sidebar container, workspace list, conversation list) and the `sidebar_wiring` module connects `SidebarResponse` to `AppState` mutations. However, none of this is visible to the user because egui is not integrated into the wgpu render pass.

The dependencies are already in place (`egui = "0.34"`, `egui-wgpu = "0.34"`, `egui-winit = "0.34"` in the workspace `Cargo.toml` and `crates/veil/Cargo.toml`), and the sidebar wiring module (`crates/veil/src/sidebar_wiring.rs`) has `apply_sidebar_response()` with comprehensive tests. The `sidebar_wiring` module is `#[allow(dead_code)]` in `main.rs`, meaning it compiles but is never called.

This task wires everything together: creating the egui context and renderer at startup, feeding winit events to egui for input handling, running the sidebar UI each frame, rendering egui output as an overlay in the wgpu render pass, and connecting sidebar interactions to state mutations via the existing `apply_sidebar_response()`.

### What exists

**Sidebar UI logic (`veil-ui` crate):**
- `veil_ui::sidebar::render_sidebar(ui: &mut egui::Ui, state: &AppState) -> SidebarResponse` -- renders the sidebar container with tab header and delegates to active tab content. Uses `egui::Panel::left()` with exact width from `state.sidebar.width_px`.
- `veil_ui::sidebar::SidebarResponse` -- `switch_to_workspace: Option<WorkspaceId>`, `switch_tab: Option<SidebarTab>`, `selected_conversation: Option<SessionId>`.
- `veil_ui::workspace_list::render_workspaces_tab(ui, entries) -> Option<WorkspaceId>` -- renders clickable workspace entries.
- `veil_ui::workspace_list::extract_workspace_entries(state) -> Vec<WorkspaceEntryData>` -- extracts view-model from AppState.
- `veil_ui::conversation_list::render_conversations_tab(ui, groups) -> Option<SessionId>` -- renders conversation groups.
- `veil_ui::conversation_list::extract_conversation_groups(state, now, live_state) -> Vec<ConversationGroup>` -- extracts grouped conversation data.

**Sidebar wiring (`crates/veil/src/sidebar_wiring.rs`):**
- `apply_sidebar_response(response, app_state, focus) -> Result<(), String>` -- handles tab switching, workspace switching, focus updates. Already tested with 11 unit tests covering happy paths and edge cases.

**Renderer (`crates/veil/src/renderer.rs`):**
- `Renderer` struct owns wgpu device, queue, surface, config, render pipeline, uniform buffer, bind group. Has `new()`, `resize()`, `render(&mut self, frame_geometry: &FrameGeometry)`.
- Single render pipeline for solid-color quads via `shader.wgsl`. The pipeline uses `Vertex` (position + color), alpha blending enabled.
- `render()` gets surface texture, creates encoder, begins render pass with clear color, draws indexed geometry, submits, presents.

**Main event loop (`crates/veil/src/main.rs`):**
- `VeilApp` struct owns `window`, `renderer`, `app_state`, `channels`, `shutdown`, `keybindings`, `focus`, `current_modifiers`, `window_size`, `pty_manager`.
- `resumed()`: creates window, creates renderer, bootstraps default workspace, spawns PTY.
- `window_event()`: handles close, resize, redraw, modifiers, keyboard input.
- `RedrawRequested`: calls `build_frame_geometry()` then `renderer.render()`, then requests redraw.
- `KeyboardInput`: translates keys, routes via `route_key_event()`, dispatches actions or forwards to PTY. `KeyRoute::ForwardToSidebar` is currently a no-op comment: `// Sidebar key handling and unhandled keys are no-ops until egui sidebar integration.`

**Frame builder (`crates/veil/src/frame.rs`):**
- `build_frame_geometry()` already respects `sidebar.visible` and `sidebar.width_px` -- when sidebar is visible, terminal area starts at `x = sidebar.width_px`.
- The sidebar region is currently empty (just the clear color background).

**Keyboard wiring:**
- `KeyAction::ToggleSidebar` already dispatched via `action_dispatch.rs` -- calls `app_state.toggle_sidebar()` and returns `ActionEffect::Redraw`.
- `KeyAction::SwitchToWorkspacesTab` and `SwitchToConversationsTab` already wired.
- `KeyAction::FocusSidebar` already calls `focus.focus_sidebar()`.

### What's missing

1. **egui context and renderer** -- No `egui::Context`, `egui_winit::State`, or `egui_wgpu::Renderer` exists anywhere. These need to be created at startup.

2. **egui event feeding** -- Winit events are not forwarded to egui. Mouse clicks, scrolling, and keyboard input in the sidebar region are ignored.

3. **egui frame execution** -- `render_sidebar()` is never called during the render loop. The sidebar UI logic exists but never runs.

4. **egui render pass** -- egui paint output is never rendered to the wgpu surface. Even if the sidebar ran, nothing would appear.

5. **SidebarResponse handling** -- `apply_sidebar_response()` exists and is tested but is never called from the event loop. Sidebar clicks have no effect.

6. **Remove `#[allow(dead_code)]`** -- `sidebar_wiring` is dead code. After wiring, the annotation should be removed.

### Design decisions

**egui integration struct lives on `Renderer`, not on `VeilApp`.**

The egui wgpu renderer needs access to `device`, `queue`, and `surface_format` from `Renderer`. Rather than duplicating these or passing them around, add the egui state directly to `Renderer`. This keeps GPU concerns colocated. `VeilApp` passes events to the renderer's egui integration methods.

**egui renders in a second render pass within the same command encoder.**

The existing render pass clears the background and draws terminal quads. The egui pass renders on top of the same surface texture view, compositing the sidebar over the cleared background. This means:
1. Terminal pass: clear + terminal quads (existing behavior, unchanged)
2. egui pass: sidebar UI composited on top

Both passes use the same `CommandEncoder` and are submitted together, so there is only one `queue.submit()` call per frame.

**`render_sidebar()` signature adaptation.**

The existing `render_sidebar()` takes `&mut egui::Ui`, which means it expects to be called inside an existing UI container. The egui-wgpu integration provides a top-level `egui::Context`, so we need to call `render_sidebar` inside an `egui::CentralPanel` or equivalent. Looking at the existing code, `render_sidebar` creates its own `Panel::left()` internally, so it needs a top-level `Ui` from the context. We will call it via `ctx.run()` which provides the root `Ui`. Actually, `egui::Context::run()` returns a `FullOutput` and takes a closure with `&egui::Context`. The existing `render_sidebar` takes `&mut egui::Ui`. The tests use `ctx.run_ui(input, |ui| render_sidebar(ui, state))` which gives a `Ui`. For the real integration, we need to match this pattern. We will use `egui::CentralPanel::default().show(ctx, |ui| render_sidebar(ui, state))` inside the `ctx.run()` closure.

**Event gating: always feed events to egui, but only consume when egui wants them.**

Rather than conditionally feeding events based on `sidebar.visible`, we always feed events to `egui_winit::State::on_window_event()`. When the sidebar is hidden, egui simply has no widgets to interact with, so it won't consume events. We check `egui_ctx.wants_pointer_input()` and `egui_ctx.wants_keyboard_input()` after the frame to determine whether to suppress forwarding the event to the terminal. This is simpler and more correct than manual gating.

**Expose `device`, `queue`, and `config` from `Renderer` for egui initialization.**

The `Renderer::new()` creates and owns the wgpu state. The egui renderer needs references to `device` and `queue` during initialization and rendering. We add accessor methods or pass them during construction.

## Implementation Units

### Unit 1: Add `EguiIntegration` struct to `Renderer`

**Location:** `crates/veil/src/renderer.rs`

Add a new struct that bundles egui's wgpu and winit integration:

```rust
/// Bundles egui context, winit event translator, and wgpu renderer.
pub struct EguiIntegration {
    pub ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}
```

Add a constructor:

```rust
impl EguiIntegration {
    pub fn new(
        window: &winit::window::Window,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(
            ctx.clone(),
            ctx.viewport_id(),
            window,
            None, // native_pixels_per_point: auto-detect
            None, // max_texture_side: auto-detect
        );
        let renderer = egui_wgpu::Renderer::new(device, surface_format, None, 1, false);
        Self { ctx, state, renderer }
    }
}
```

Add methods for the event loop to call:

```rust
impl EguiIntegration {
    /// Feed a winit event to egui. Returns whether egui consumed it.
    pub fn on_window_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    /// Begin an egui frame. Returns the raw input to pass to ctx.run().
    pub fn take_raw_input(&mut self, window: &winit::window::Window) -> egui::RawInput {
        self.state.take_egui_input(window)
    }

    /// Process egui output after a frame (cursor changes, clipboard, etc.).
    pub fn handle_platform_output(
        &mut self,
        window: &winit::window::Window,
        platform_output: egui::PlatformOutput,
    ) {
        self.state.handle_platform_output(window, platform_output);
    }
}
```

Add the `EguiIntegration` as a field on `Renderer`:

```rust
pub struct Renderer {
    // ... existing fields ...
    egui: EguiIntegration,
}
```

Initialize in `Renderer::new()` after creating device, queue, and surface format:

```rust
let egui = EguiIntegration::new(&window, &device, surface_format);
```

Add a method to render egui output:

```rust
impl Renderer {
    /// Render egui output into the given texture view.
    ///
    /// This creates a second render pass (after the terminal pass)
    /// that composites egui's UI on top.
    pub fn render_egui(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        full_output: egui::FullOutput,
    ) {
        let pixels_per_point = self.egui.ctx.pixels_per_point();
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point,
        };

        let clipped_primitives = self.egui.ctx.tessellate(
            full_output.shapes,
            pixels_per_point,
        );

        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui.renderer.update_texture(
                &self.device,
                &self.queue,
                *id,
                image_delta,
            );
        }

        self.egui.renderer.update_buffers(
            &self.device,
            &self.queue,
            encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,  // Don't clear -- composite on top
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.egui.renderer.render(
                &mut render_pass,
                &clipped_primitives,
                &screen_descriptor,
            );
        }

        for id in &full_output.textures_delta.free {
            self.egui.renderer.free_texture(id);
        }
    }
}
```

**Refactor `render()` to expose the encoder and view for multi-pass rendering.** Currently `render()` owns the entire frame lifecycle (get texture, create encoder, begin pass, draw, submit, present). To add egui as a second pass, split the method:

Change the `render()` signature to also accept a callback or simply restructure to:

```rust
/// Render a complete frame: terminal geometry + egui overlay.
pub fn render(
    &mut self,
    frame_geometry: &FrameGeometry,
    egui_full_output: Option<egui::FullOutput>,
) -> anyhow::Result<()>
```

Inside `render()`:
1. Get surface texture (existing)
2. Create encoder (existing)
3. Terminal render pass (existing, unchanged)
4. If `egui_full_output` is `Some`, call the egui render logic (new)
5. Submit and present (existing)

**Tests:**

- `window_uniform_size_is_8_bytes` -- existing test, must still pass
- `vertex_buffer_stride_matches_vertex_size` -- existing test, must still pass
- New test: `egui_integration_struct_size` -- verify `EguiIntegration` fields are accessible (compile test)
- GPU-dependent integration tests are `#[ignore]`

### Unit 2: Wire egui event handling in `VeilApp`

**Location:** `crates/veil/src/main.rs`

**Changes to `window_event()`:**

Before the existing `match event` block, forward the event to egui:

```rust
// Forward event to egui for input handling
if let Some(renderer) = &mut self.renderer {
    let _egui_response = renderer.egui.on_window_event(
        self.window.as_ref().unwrap(),
        &event,
    );
}
```

The event response has `consumed: bool`. For now, we do NOT short-circuit our own event handling based on this. The reason: egui's consumption flag is for pointer events over egui widgets. Our keyboard routing (`route_key_event`) already handles sidebar focus correctly via `FocusManager`. Skipping our handler when egui consumed an event would break keyboard shortcuts like Cmd+B that should work even when the pointer is over the sidebar.

For pointer events specifically, we may want to skip terminal-specific handling (future: click-to-focus pane) when the pointer is over the sidebar. But since no terminal pointer handling exists yet, this is a no-op.

**Changes to `KeyboardInput` handling:**

The existing `KeyRoute::ForwardToSidebar` arm is a no-op:
```rust
KeyRoute::ForwardToSidebar | KeyRoute::Unhandled => {}
```

For now, leave this as-is. Keyboard navigation within the sidebar (j/k, arrow keys) is a follow-up task. The mouse click interaction works through egui's event system without needing `ForwardToSidebar`.

**Tests:**

- Existing tests in `action_dispatch.rs` must still pass (no changes to action dispatch)
- No new tests needed for this unit -- the wiring is validated by Unit 4 integration tests and by running the app

### Unit 3: Run egui sidebar frame during `RedrawRequested`

**Location:** `crates/veil/src/main.rs`

**Changes to the `RedrawRequested` handler:**

Replace the current:
```rust
WindowEvent::RedrawRequested => {
    let frame_geometry = build_frame_geometry(...);
    if let Some(renderer) = &mut self.renderer {
        match renderer.render(&frame_geometry) { ... }
    }
    if let Some(window) = &self.window { window.request_redraw(); }
}
```

With:
```rust
WindowEvent::RedrawRequested => {
    let frame_geometry = build_frame_geometry(
        &self.app_state,
        &self.focus,
        self.window_size.0,
        self.window_size.1,
    );

    // Run egui frame if sidebar is visible
    let egui_output = if self.app_state.sidebar.visible {
        if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
            let raw_input = renderer.egui.take_raw_input(window);
            let full_output = renderer.egui.ctx.run(raw_input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = veil_ui::sidebar::render_sidebar(ui, &self.app_state);
                    sidebar_wiring::apply_sidebar_response(
                        &response,
                        &mut self.app_state,
                        &mut self.focus,
                    ).unwrap_or_else(|e| {
                        tracing::warn!("sidebar response error: {e}");
                    });
                });
            });
            renderer.egui.handle_platform_output(window, full_output.platform_output.clone());
            Some(full_output)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(renderer) = &mut self.renderer {
        match renderer.render(&frame_geometry, egui_output) {
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

**Borrow checker consideration:** The closure inside `ctx.run()` needs `&self.app_state` (immutable) while also needing `&mut self.app_state` for `apply_sidebar_response`. This is a conflict. Solution: collect the `SidebarResponse` from the closure and apply it after `ctx.run()` returns.

Corrected pattern:
```rust
let full_output = renderer.egui.ctx.run(raw_input, |ctx| {
    egui::CentralPanel::default().show(ctx, |ui| {
        veil_ui::sidebar::render_sidebar(ui, &self.app_state)
    });
});
```

Wait -- this won't work either because `ctx.run` takes `&egui::Context` which is behind `renderer.egui`, and we also need `&self.app_state`. Since `renderer` and `app_state` are separate fields on `VeilApp`, Rust can handle partial borrows in a closure only when they're different struct fields. Let's restructure:

```rust
// Take raw input before running egui
let raw_input = renderer.egui.take_raw_input(window);

// Run egui frame -- capture SidebarResponse
let mut sidebar_response = veil_ui::sidebar::SidebarResponse::default();
let full_output = renderer.egui.ctx.run(raw_input, |ctx| {
    egui::CentralPanel::default().show(ctx, |ui| {
        sidebar_response = veil_ui::sidebar::render_sidebar(ui, &self.app_state);
    });
});

// Apply sidebar response to state (after egui frame completes)
if let Err(e) = sidebar_wiring::apply_sidebar_response(
    &sidebar_response,
    &mut self.app_state,
    &mut self.focus,
) {
    tracing::warn!("sidebar response error: {e}");
}

// Handle platform output (cursor changes, etc.)
renderer.egui.handle_platform_output(window, full_output.platform_output.clone());
```

This works because:
1. `renderer.egui.ctx.run()` borrows `renderer.egui.ctx` mutably and `self.app_state` immutably
2. After `run()` returns, `renderer.egui` is no longer borrowed
3. `apply_sidebar_response` borrows `self.app_state` and `self.focus` mutably -- no conflict since `renderer.egui` is done

**However**, there is a subtlety: `renderer.egui.ctx.run()` takes a `FnOnce(&egui::Context)`. The `ctx` parameter is the egui context. We need to call `render_sidebar` with a `&mut egui::Ui`, which we get from `CentralPanel::default().show()`. The `self.app_state` borrow inside the closure is fine because it's a read-only borrow on a different field.

The real challenge is that `self.renderer` and `self.app_state` are both fields on `VeilApp`. When we do `self.renderer.as_mut().unwrap().egui.ctx.run(...)`, Rust borrows `self.renderer` mutably. Inside the closure, accessing `self.app_state` would borrow `self` again. **Solution: destructure self before the block.**

```rust
let VeilApp {
    renderer: ref mut renderer_opt,
    app_state: ref app_state,
    focus: ref mut focus,
    window: ref window_opt,
    ..
} = self;

// Then use renderer_opt, app_state, focus, window_opt directly
```

This is the idiomatic Rust pattern for partial borrows of struct fields.

**Remove `#[allow(dead_code)]` from `sidebar_wiring` module declaration in `main.rs`.**

**Tests:**

- No new unit tests for the render loop itself (it requires a GPU + window)
- The correctness of sidebar response handling is already tested by `sidebar_wiring::tests`
- Verify via `cargo build` that the code compiles

### Unit 4: Remove dead code annotations and validate integration

**Location:** `crates/veil/src/main.rs`

**Changes:**

1. Remove `#[allow(dead_code)]` from `mod sidebar_wiring;` declaration
2. Verify that all existing tests pass: `cargo test -p veil`, `cargo test -p veil-ui`
3. Run `cargo clippy --all-targets --all-features -- -D warnings`
4. Run `cargo fmt --check`

**New tests in `crates/veil/src/sidebar_wiring.rs`:**

No new tests needed -- the existing 11 tests already cover the full `apply_sidebar_response` surface.

**New test in `crates/veil/src/renderer.rs`:**

```rust
#[test]
fn render_with_none_egui_output_does_not_panic() {
    // Verify that render() accepts None for egui_output
    // This is a compile/signature test -- actual rendering needs GPU
}
```

Since the existing renderer tests are `#[ignore]` (GPU-dependent), we add a compile-level test that verifies the new signature compiles.

## Test Strategy Summary

| Unit | What | Test type | GPU required? |
|------|------|-----------|---------------|
| 1: EguiIntegration struct | Struct construction, method signatures | Compile test | No (struct is tested indirectly) |
| 1: render() signature change | Accepts `Option<egui::FullOutput>` | Compile test + existing ignored tests | Yes (ignored) |
| 2: Event forwarding | Events reach egui | Manual/E2E (run the app) | Yes |
| 3: Sidebar frame execution | `render_sidebar` called, response applied | Compile test; correctness via sidebar_wiring tests | No |
| 4: Dead code cleanup | `#[allow(dead_code)]` removed | `cargo clippy` | No |
| Existing | `sidebar_wiring::tests` (11 tests) | Unit tests | No |
| Existing | `veil_ui::sidebar::tests` (12 tests) | Unit tests (headless egui) | No |
| Existing | `veil_ui::workspace_list::tests` (17 tests) | Unit tests (headless egui) | No |
| Existing | `veil_ui::conversation_list::tests` (30+ tests) | Unit tests (headless egui) | No |
| Existing | `renderer::tests` (2 tests) | Unit tests | No |
| Existing | `frame::tests` (11 tests) | Unit tests | No |

## Acceptance Criteria

- [ ] Sidebar is visible on the left side of the window (250px default width) when `sidebar.visible` is true
- [ ] Workspace list shows workspace entries with active indicator, name, git branch, abbreviated working directory
- [ ] Cmd+B (ToggleSidebar) toggles sidebar visibility -- terminal area resizes to fill freed space
- [ ] Tab switching between Workspaces and Conversations works (click tab headers)
- [ ] Clicking a workspace in the list switches the active workspace (verified by workspace becoming active with bullet indicator)
- [ ] Conversation list renders when Conversations tab is active (with session entries if any exist)
- [ ] Mouse clicks and scrolling in the sidebar work (egui receives events)
- [ ] Keyboard shortcuts (Cmd+B, Ctrl+Shift+W, Ctrl+Shift+C) continue to work
- [ ] No rendering regressions: terminal quads, cursor, dividers, focus border all render correctly
- [ ] `#[allow(dead_code)]` removed from `sidebar_wiring` module in `main.rs`
- [ ] All existing tests pass (`cargo test`)
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt --check` passes

## Dependencies

**Crate dependencies (already present -- no changes needed):**

| Crate | Version | Location | Purpose |
|-------|---------|----------|---------|
| `egui` | `0.34` | `Cargo.toml` workspace + `crates/veil/Cargo.toml` + `crates/veil-ui/Cargo.toml` | egui types and layout |
| `egui-wgpu` | `0.34` | `Cargo.toml` workspace + `crates/veil/Cargo.toml` | Renders egui via wgpu |
| `egui-winit` | `0.34` | `Cargo.toml` workspace + `crates/veil/Cargo.toml` | Translates winit events to egui |

**Files to modify:**

| File | Changes |
|------|---------|
| `crates/veil/src/renderer.rs` | Add `EguiIntegration` struct, add to `Renderer`, add `render_egui` method, modify `render()` signature to accept `Option<egui::FullOutput>` |
| `crates/veil/src/main.rs` | Remove `#[allow(dead_code)]` from `sidebar_wiring`, add egui event forwarding in `window_event`, add egui frame execution in `RedrawRequested`, call `apply_sidebar_response` |

**No new files needed.**

**No new crate dependencies needed.**

## Implementation Order

Units 1 and 2 are closely coupled (the event loop needs the renderer's egui integration). Implement in order: 1 -> 2 -> 3 -> 4.

Unit 1 (EguiIntegration + Renderer changes) must come first because Units 2 and 3 depend on it.

Unit 2 (event forwarding) and Unit 3 (frame execution + response handling) can be done together since they both modify `window_event()` in `main.rs`.

Unit 4 (cleanup) is the final step after everything compiles and tests pass.

## Key API Notes for Implementation

### `egui_winit::State` constructor

```rust
egui_winit::State::new(
    ctx: egui::Context,      // clone of the context
    viewport_id: ViewportId, // ctx.viewport_id()
    display_target: &dyn ..., // &Window implements this
    native_pixels_per_point: Option<f32>, // None for auto
    max_texture_side: Option<usize>,      // None for auto
)
```

### `egui_wgpu::Renderer` constructor

```rust
egui_wgpu::Renderer::new(
    device: &wgpu::Device,
    output_color_format: wgpu::TextureFormat,
    output_depth_format: Option<wgpu::TextureFormat>, // None
    msaa_samples: u32,  // 1 (no MSAA)
    dithering: bool,    // false
)
```

### `egui::Context::run()`

```rust
ctx.run(raw_input: egui::RawInput, run_ui: impl FnOnce(&egui::Context)) -> egui::FullOutput
```

The closure receives `&egui::Context`. Inside, call `egui::CentralPanel::default().show(ctx, |ui| ...)` to get a `&mut Ui` for `render_sidebar`.

### `render_sidebar` call site

The existing `render_sidebar` takes `&mut egui::Ui`. In the egui frame:

```rust
renderer.egui.ctx.run(raw_input, |ctx| {
    egui::CentralPanel::default().show(ctx, |ui| {
        sidebar_response = veil_ui::sidebar::render_sidebar(ui, &app_state);
    });
});
```

### Partial borrow pattern for `VeilApp`

To avoid borrow checker conflicts when accessing multiple `VeilApp` fields:

```rust
let Self {
    renderer: ref mut renderer_opt,
    app_state: ref mut app_state,
    focus: ref mut focus,
    window: ref window_opt,
    ..
} = self;
```

This destructures `self` into individual field references, allowing independent borrows.
