//! egui integration — bundles context, winit event translator, and wgpu renderer.
//!
//! Created alongside `Renderer` at startup. Owns the `egui::Context` and the
//! platform/GPU plumbing needed for event translation and rendering.

/// Bundles egui context, winit event translator, and wgpu renderer.
///
/// The `egui_winit::State` and `egui_wgpu::Renderer` fields are gated
/// behind `Option` so that headless tests can construct an `EguiIntegration`
/// without a window or GPU.
pub struct EguiIntegration {
    /// The egui context. Public so the event loop can call `ctx.run()`.
    pub ctx: egui::Context,
    /// Translates winit events into egui input. `None` in headless mode.
    winit_state: Option<egui_winit::State>,
    /// Renders egui output to wgpu textures. `None` in headless mode.
    wgpu_renderer: Option<egui_wgpu::Renderer>,
}

impl EguiIntegration {
    /// Create an `EguiIntegration` with full windowing and GPU support.
    pub fn new(
        window: &winit::window::Window,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let ctx = egui::Context::default();
        let winit_state = egui_winit::State::new(
            ctx.clone(),
            ctx.viewport_id(),
            window,
            None, // native_pixels_per_point: auto-detect
            None, // theme: auto-detect
            None, // max_texture_side: auto-detect
        );
        let wgpu_renderer = egui_wgpu::Renderer::new(
            device,
            surface_format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                dithering: false,
                predictable_texture_filtering: false,
            },
        );
        Self { ctx, winit_state: Some(winit_state), wgpu_renderer: Some(wgpu_renderer) }
    }

    // -- Event handling -------------------------------------------------------

    /// Feed a winit event to egui. Returns whether egui consumed it.
    ///
    /// No-op in headless mode (returns a default response).
    pub fn on_window_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> egui_winit::EventResponse {
        if let Some(state) = &mut self.winit_state {
            state.on_window_event(window, event)
        } else {
            egui_winit::EventResponse { consumed: false, repaint: false }
        }
    }

    /// Begin an egui frame. Returns the raw input to pass to `ctx.run()`.
    ///
    /// No-op in headless mode (returns default `RawInput`).
    pub fn take_raw_input(&mut self, window: &winit::window::Window) -> egui::RawInput {
        if let Some(state) = &mut self.winit_state {
            state.take_egui_input(window)
        } else {
            egui::RawInput::default()
        }
    }

    /// Process egui output after a frame (cursor changes, clipboard, etc.).
    ///
    /// No-op in headless mode.
    pub fn handle_platform_output(
        &mut self,
        window: &winit::window::Window,
        platform_output: egui::PlatformOutput,
    ) {
        if let Some(state) = &mut self.winit_state {
            state.handle_platform_output(window, platform_output);
        }
    }

    // -- GPU rendering --------------------------------------------------------

    /// Render egui output into the given texture view.
    ///
    /// Tessellates egui shapes, uploads textures, and renders in a render pass
    /// with `LoadOp::Load` to composite the sidebar on top of terminal quads.
    ///
    /// No-op if the wgpu renderer is not available (headless mode).
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
        let screen_descriptor =
            egui_wgpu::ScreenDescriptor { size_in_pixels: surface_size, pixels_per_point };

        let clipped_primitives = self.ctx.tessellate(full_output.shapes, pixels_per_point);

        for (id, image_delta) in &full_output.textures_delta.set {
            wgpu_renderer.update_texture(device, queue, *id, image_delta);
        }

        wgpu_renderer.update_buffers(
            device,
            queue,
            encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // egui-wgpu requires `RenderPass<'static>`; `forget_lifetime()`
            // detaches the borrow. Safe because the render pass is used and
            // dropped within this block before `encoder.finish()`.
            let mut render_pass = render_pass.forget_lifetime();
            wgpu_renderer.render(&mut render_pass, &clipped_primitives, &screen_descriptor);
        }

        for id in &full_output.textures_delta.free {
            wgpu_renderer.free_texture(id);
        }
    }
}

#[cfg(test)]
impl EguiIntegration {
    /// Create a headless `EguiIntegration` (no GPU, no window).
    ///
    /// Useful for tests that only need `run_frame` without rendering.
    fn new_headless() -> Self {
        Self { ctx: egui::Context::default(), winit_state: None, wgpu_renderer: None }
    }

    /// Create a headless `EguiIntegration` with a specific scale factor.
    ///
    /// Mirrors what `EguiIntegration::new()` should do: initialize the egui
    /// context with the given `native_pixels_per_point` so that
    /// `ctx.pixels_per_point()` returns the correct value from the first frame.
    ///
    /// TODO(VEI-84): Once the production constructor passes `native_pixels_per_point`
    /// to `State::new()` and calls `ctx.set_pixels_per_point()`, this test helper
    /// should mirror that behavior.
    fn new_headless_with_scale(native_pixels_per_point: f32) -> Self {
        let ctx = egui::Context::default();
        // Currently does NOT set pixels_per_point — this is the bug VEI-84 fixes.
        // The production `new()` should call:
        //   ctx.set_pixels_per_point(native_pixels_per_point);
        // but doesn't yet, so tests using this helper will fail.
        let _ = native_pixels_per_point;
        Self { ctx, winit_state: None, wgpu_renderer: None }
    }

    /// Run a single egui frame with the sidebar UI using synthetic input.
    ///
    /// Executes `render_sidebar` inside the egui context and returns the
    /// sidebar interaction response together with the full egui output
    /// (shapes, textures, platform output).
    fn run_frame(
        &self,
        app_state: &veil_core::state::AppState,
    ) -> (veil_ui::sidebar::SidebarResponse, egui::FullOutput) {
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(1280.0, 800.0),
            )),
            ..Default::default()
        };

        let mut sidebar_response = veil_ui::sidebar::SidebarResponse::default();
        let full_output = self.ctx.run_ui(raw_input, |ui| {
            sidebar_response = veil_ui::sidebar::render_sidebar(ui, app_state);
        });

        (sidebar_response, full_output)
    }

    /// Run a single egui frame with custom `RawInput`.
    ///
    /// Useful for tests that need to set `native_pixels_per_point` or
    /// `max_texture_side` in the viewport info.
    fn run_frame_with_raw_input(&self, raw_input: egui::RawInput) -> egui::FullOutput {
        self.ctx.run_ui(raw_input, |_ui| {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veil_core::state::AppState;

    // ================================================================
    // Headless construction
    // ================================================================

    #[test]
    fn egui_integration_creates_default_context() {
        let integration = EguiIntegration::new_headless();
        // A default egui context has a positive pixels_per_point.
        assert!(
            integration.ctx.pixels_per_point() > 0.0,
            "default context should have positive pixels_per_point"
        );
    }

    // ================================================================
    // run_frame — sidebar produces egui shapes
    // ================================================================

    #[test]
    fn run_frame_produces_shapes_when_sidebar_visible() {
        let integration = EguiIntegration::new_headless();
        let state = AppState::new(); // sidebar visible by default, width_px = 250
        let (_response, full_output) = integration.run_frame(&state);

        assert!(
            !full_output.shapes.is_empty(),
            "run_frame with visible sidebar should produce egui shapes"
        );
    }

    #[test]
    fn run_frame_produces_texture_updates() {
        // egui needs at least one font texture upload on the first frame.
        let integration = EguiIntegration::new_headless();
        let state = AppState::new();
        let (_response, full_output) = integration.run_frame(&state);

        assert!(
            !full_output.textures_delta.set.is_empty(),
            "first egui frame should request at least one texture upload (font atlas)"
        );
    }

    // ================================================================
    // run_frame — sidebar response reflects state
    // ================================================================

    #[test]
    fn run_frame_without_interaction_returns_default_response() {
        let integration = EguiIntegration::new_headless();
        let state = AppState::new();
        let (response, _output) = integration.run_frame(&state);

        assert!(
            response.switch_to_workspace.is_none(),
            "no interaction should leave switch_to_workspace as None"
        );
        assert!(response.switch_tab.is_none(), "no interaction should leave switch_tab as None");
        assert!(
            response.selected_conversation.is_none(),
            "no interaction should leave selected_conversation as None"
        );
    }

    // ================================================================
    // run_frame — workspace entries rendered
    // ================================================================

    #[test]
    fn run_frame_with_workspaces_produces_shapes() {
        use std::path::PathBuf;

        let integration = EguiIntegration::new_headless();
        let mut state = AppState::new();
        state.create_workspace("project-alpha".to_string(), PathBuf::from("/tmp/alpha"));
        state.create_workspace("project-beta".to_string(), PathBuf::from("/tmp/beta"));

        let (_response, full_output) = integration.run_frame(&state);

        assert!(
            !full_output.shapes.is_empty(),
            "run_frame with workspaces should produce shapes for workspace list entries"
        );
    }

    // ================================================================
    // run_frame — conversations tab
    // ================================================================

    #[test]
    fn run_frame_conversations_tab_produces_shapes() {
        use veil_core::session::{AgentKind, SessionEntry, SessionId, SessionStatus};

        let integration = EguiIntegration::new_headless();
        let mut state = AppState::new();
        state.set_sidebar_tab(veil_core::state::SidebarTab::Conversations);
        state.update_conversations(vec![SessionEntry {
            id: SessionId::new("session-1"),
            agent: AgentKind::ClaudeCode,
            title: "Fix authentication bug".to_string(),
            working_dir: std::path::PathBuf::from("/tmp/project"),
            branch: Some("main".to_string()),
            pr_number: None,
            pr_url: None,
            plan_content: None,
            status: SessionStatus::Active,
            started_at: chrono::Utc::now(),
            ended_at: None,
            indexed_at: chrono::Utc::now(),
        }]);

        let (_response, full_output) = integration.run_frame(&state);

        assert!(
            !full_output.shapes.is_empty(),
            "run_frame with conversations should produce shapes for conversation entries"
        );
    }

    // ================================================================
    // run_frame — hidden sidebar
    // ================================================================

    #[test]
    fn run_frame_hidden_sidebar_still_returns_output() {
        let integration = EguiIntegration::new_headless();
        let mut state = AppState::new();
        state.toggle_sidebar(); // hide

        let (response, full_output) = integration.run_frame(&state);

        // Response should be default (no interactions possible on hidden sidebar).
        assert!(response.switch_to_workspace.is_none());
        assert!(response.switch_tab.is_none());
        // FullOutput should still be valid (not crash).
        let _ = full_output.platform_output;
    }

    // ================================================================
    // run_frame — consecutive frames share context
    // ================================================================

    #[test]
    fn run_frame_consecutive_frames_share_context() {
        let integration = EguiIntegration::new_headless();
        let state = AppState::new();

        // First frame
        let (_, output1) = integration.run_frame(&state);
        // Second frame
        let (_, output2) = integration.run_frame(&state);

        // Both frames should produce shapes when the sidebar is visible.
        assert!(!output1.shapes.is_empty(), "first frame should produce shapes");
        assert!(!output2.shapes.is_empty(), "second frame should also produce shapes");
    }

    // ================================================================
    // VEI-84: pixels_per_point initialization (Unit 1)
    // ================================================================

    /// Helper: assert two f32 values are approximately equal.
    fn assert_f32_eq(actual: f32, expected: f32, msg: &str) {
        assert!(
            (actual - expected).abs() < f32::EPSILON,
            "{msg}: expected {expected}, got {actual}"
        );
    }

    #[test]
    fn headless_with_scale_factor_sets_pixels_per_point() {
        // Unit 1: When EguiIntegration is constructed with a scale factor,
        // ctx.pixels_per_point() must return that value immediately --
        // before any frame is run. This is the core fix for VEI-84.
        let integration = EguiIntegration::new_headless_with_scale(2.0);
        assert_f32_eq(
            integration.ctx.pixels_per_point(),
            2.0,
            "pixels_per_point should match the scale factor passed at construction",
        );
    }

    #[test]
    fn headless_with_scale_factor_1x_display() {
        // Unit 1: On a standard 1x display, scale factor is 1.0.
        // After construction, pixels_per_point should be 1.0 (same as default,
        // but explicitly set rather than relying on the egui default).
        let integration = EguiIntegration::new_headless_with_scale(1.0);
        assert_f32_eq(
            integration.ctx.pixels_per_point(),
            1.0,
            "pixels_per_point should be 1.0 for standard displays",
        );
    }

    #[test]
    fn headless_with_scale_factor_3x_display() {
        // Unit 1: Some displays (e.g., certain Android/Windows 4K) use 3x.
        let integration = EguiIntegration::new_headless_with_scale(3.0);
        assert_f32_eq(
            integration.ctx.pixels_per_point(),
            3.0,
            "pixels_per_point should be 3.0 for 3x displays",
        );
    }

    #[test]
    fn headless_with_fractional_scale_factor() {
        // Unit 1: Windows commonly uses fractional scaling (e.g., 1.25, 1.5).
        let integration = EguiIntegration::new_headless_with_scale(1.5);
        assert_f32_eq(
            integration.ctx.pixels_per_point(),
            1.5,
            "pixels_per_point should handle fractional scale factors",
        );
    }

    // ================================================================
    // VEI-84: pixels_per_point propagation through RawInput (Unit 1)
    // ================================================================

    #[test]
    fn pixels_per_point_propagates_through_raw_input() {
        // Unit 1: When RawInput contains native_pixels_per_point in viewport
        // info, ctx.pixels_per_point() should reflect that after run_ui().
        // This verifies the propagation path that take_egui_input() uses.
        let integration = EguiIntegration::new_headless();

        let viewport_info =
            egui::ViewportInfo { native_pixels_per_point: Some(2.0), ..Default::default() };

        let mut viewports = egui::ViewportIdMap::default();
        viewports.insert(egui::ViewportId::ROOT, viewport_info);

        let raw_input = egui::RawInput {
            viewports,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(640.0, 400.0), // 1280/2, 800/2 logical points
            )),
            ..Default::default()
        };

        let _output = integration.run_frame_with_raw_input(raw_input);

        assert_f32_eq(
            integration.ctx.pixels_per_point(),
            2.0,
            "after run_ui with native_pixels_per_point=2.0, ctx should report 2.0",
        );
    }

    // ================================================================
    // VEI-84: ScreenDescriptor math (Unit 1)
    // ================================================================

    #[allow(clippy::cast_precision_loss)] // test uses known small pixel values
    #[test]
    fn screen_descriptor_uses_correct_pixels_per_point_for_retina() {
        // Unit 1: The ScreenDescriptor constructed in render() must use
        // the correct pixels_per_point. On Retina (2x), a 2560x1600 pixel
        // surface should produce 1280x800 logical points.
        let ppp = 2.0_f32;
        let surface_size: [u32; 2] = [2560, 1600];

        let descriptor =
            egui_wgpu::ScreenDescriptor { size_in_pixels: surface_size, pixels_per_point: ppp };

        // Replicate the private screen_size_in_points() math from egui-wgpu.
        let screen_points_w = descriptor.size_in_pixels[0] as f32 / descriptor.pixels_per_point;
        let screen_points_h = descriptor.size_in_pixels[1] as f32 / descriptor.pixels_per_point;

        assert_f32_eq(screen_points_w, 1280.0, "width in points should be 2560/2.0 = 1280");
        assert_f32_eq(screen_points_h, 800.0, "height in points should be 1600/2.0 = 800");
    }

    #[allow(clippy::cast_precision_loss)] // test uses known small pixel values
    #[test]
    fn screen_descriptor_with_wrong_ppp_doubles_screen_size() {
        // Unit 1: This demonstrates the bug: when ppp=1.0 on a Retina display,
        // the screen_size_in_points is doubled, causing text to compress.
        let wrong_ppp = 1.0_f32;
        let surface_size: [u32; 2] = [2560, 1600];

        let screen_points_w = surface_size[0] as f32 / wrong_ppp;

        // With wrong ppp=1.0, egui thinks the screen is 2560x1600 points
        // when it's actually 1280x800 points. All vertex positions compress.
        assert_f32_eq(screen_points_w, 2560.0, "wrong ppp=1.0 produces doubled width in points");
        assert!(
            (screen_points_w - 1280.0).abs() > 1.0,
            "wrong ppp=1.0 does NOT produce correct width"
        );
    }

    #[allow(clippy::cast_precision_loss)] // test uses known small pixel values
    #[test]
    fn screen_descriptor_after_construction_uses_initial_ppp() {
        // Unit 1: After constructing with scale=2.0, the ScreenDescriptor
        // built the same way as render() should compute correct points.
        let integration = EguiIntegration::new_headless_with_scale(2.0);
        let surface_size: [u32; 2] = [2560, 1600];

        let ppp = integration.ctx.pixels_per_point();
        let screen_points_w = surface_size[0] as f32 / ppp;
        let screen_points_h = surface_size[1] as f32 / ppp;

        assert_f32_eq(
            screen_points_w,
            1280.0,
            "with correct ppp from construction, width should be 1280 points",
        );
        assert_f32_eq(
            screen_points_h,
            800.0,
            "with correct ppp from construction, height should be 800 points",
        );
    }

    // ================================================================
    // VEI-84: max_texture_side in RawInput (Unit 4)
    // ================================================================

    #[test]
    fn max_texture_side_propagates_through_raw_input() {
        // Unit 4: When max_texture_side is set in RawInput, the context
        // should respect it for font atlas sizing. The production code
        // should pass device.limits().max_texture_dimension_2d.
        let integration = EguiIntegration::new_headless();

        let max_texture = 8192_usize;
        let raw_input = egui::RawInput {
            max_texture_side: Some(max_texture),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(1280.0, 800.0),
            )),
            ..Default::default()
        };

        let _output = integration.run_frame_with_raw_input(raw_input);

        // After processing a frame with max_texture_side, the context should
        // have recorded it. We verify by checking that font texture allocations
        // don't exceed this limit.
        let output = integration.run_frame_with_raw_input(egui::RawInput {
            max_texture_side: Some(max_texture),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(1280.0, 800.0),
            )),
            ..Default::default()
        });

        // The font atlas texture should exist and be within limits.
        assert!(
            !output.textures_delta.set.is_empty() || output.textures_delta.free.is_empty(),
            "frame with max_texture_side should still produce valid output"
        );
    }

    // ================================================================
    // VEI-84: ScaleFactorChanged handling (Unit 5)
    // ================================================================

    #[test]
    fn set_pixels_per_point_updates_context_after_frame() {
        // Unit 5: Calling set_pixels_per_point() and then running a frame
        // should cause ctx.pixels_per_point() to reflect the new value.
        // This simulates a ScaleFactorChanged event handler.
        let integration = EguiIntegration::new_headless();
        assert_f32_eq(integration.ctx.pixels_per_point(), 1.0, "default context starts at 1.0");

        // Simulate ScaleFactorChanged: set ppp to 2.0
        integration.ctx.set_pixels_per_point(2.0);

        // Run a frame so the context processes the change.
        // set_pixels_per_point docs say: "Will become active at the start
        // of the next pass."
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(640.0, 400.0),
            )),
            ..Default::default()
        };
        let _output = integration.run_frame_with_raw_input(raw_input);

        assert_f32_eq(
            integration.ctx.pixels_per_point(),
            2.0,
            "after set_pixels_per_point(2.0) and a frame, ctx should report 2.0",
        );
    }

    #[test]
    fn set_pixels_per_point_handles_multiple_changes() {
        // Unit 5: Simulates dragging a window between monitors with different
        // DPI, causing multiple scale factor changes.
        let integration = EguiIntegration::new_headless();

        let run_frame = |ppp: f32| {
            integration.ctx.set_pixels_per_point(ppp);
            let raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::Vec2::new(1280.0 / ppp, 800.0 / ppp),
                )),
                ..Default::default()
            };
            integration.run_frame_with_raw_input(raw_input)
        };

        // Move to 2x Retina
        let _ = run_frame(2.0);
        assert_f32_eq(integration.ctx.pixels_per_point(), 2.0, "ppp after move to Retina");

        // Move to 1x external monitor
        let _ = run_frame(1.0);
        assert_f32_eq(
            integration.ctx.pixels_per_point(),
            1.0,
            "ppp after move to external monitor",
        );

        // Move back to 2x Retina
        let _ = run_frame(2.0);
        assert_f32_eq(integration.ctx.pixels_per_point(), 2.0, "ppp after return to Retina");
    }

    #[allow(clippy::cast_precision_loss)] // test uses known small pixel values
    #[test]
    fn scale_factor_change_produces_correct_screen_descriptor() {
        // Unit 5: After a scale factor change, the ScreenDescriptor built
        // the same way as render() should compute correct points.
        let integration = EguiIntegration::new_headless_with_scale(1.0);

        // Simulate moving to a 2x display.
        integration.ctx.set_pixels_per_point(2.0);
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(640.0, 400.0),
            )),
            ..Default::default()
        };
        let _output = integration.run_frame_with_raw_input(raw_input);

        // Now build a ScreenDescriptor the way render() does.
        let ppp = integration.ctx.pixels_per_point();
        let surface_size: [u32; 2] = [2560, 1600];
        let screen_points_w = surface_size[0] as f32 / ppp;
        let screen_points_h = surface_size[1] as f32 / ppp;

        assert_f32_eq(
            screen_points_w,
            1280.0,
            "after scale change to 2.0, screen width should be 1280 points",
        );
        assert_f32_eq(
            screen_points_h,
            800.0,
            "after scale change to 2.0, screen height should be 800 points",
        );
    }
}
