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
}
