//! GPU renderer for Veil's terminal UI.
//!
//! Owns all wgpu state: device, queue, surface, pipeline, buffers.
//! Created once at startup, resized on window resize, renders each frame.
//!
//! GPU-dependent tests are `#[ignore]`. `EguiIntegration` headless tests
//! run without a GPU.

use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::window::Window;

use veil_core::state::AppState;
use veil_ui::sidebar::SidebarResponse;

use crate::frame::FrameGeometry;
use crate::vertex::Vertex;

/// Uniform buffer data matching the WGSL `WindowUniform` struct.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WindowUniform {
    width: f32,
    height: f32,
}

/// Bundles egui context, winit event translator, and wgpu renderer.
///
/// Created alongside `Renderer` at startup. Owns the `egui::Context` used
/// for headless frame execution (via [`run_frame`]) and the platform/GPU
/// integration needed for event translation and rendering.
#[allow(dead_code)]
pub struct EguiIntegration {
    /// The egui context. Public so the event loop can call `ctx.run()`.
    pub ctx: egui::Context,
}

#[allow(dead_code)]
impl EguiIntegration {
    /// Create a headless `EguiIntegration` (no GPU, no window).
    ///
    /// Useful for tests. The real constructor ([`EguiIntegration::new`]) will
    /// also initialize `egui_winit::State` and `egui_wgpu::Renderer`.
    pub fn new_headless() -> Self {
        Self { ctx: egui::Context::default() }
    }

    /// Run a single egui frame with the sidebar UI.
    ///
    /// Executes `render_sidebar` inside the egui context and returns the
    /// sidebar interaction response together with the full egui output
    /// (shapes, textures, platform output) needed for GPU rendering.
    pub fn run_frame(&self, app_state: &AppState) -> (SidebarResponse, egui::FullOutput) {
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(1280.0, 800.0),
            )),
            ..Default::default()
        };

        let mut sidebar_response = SidebarResponse::default();
        let full_output = self.ctx.run_ui(raw_input, |_ui| {
            // TODO(VEI-78): call render_sidebar(ui, app_state) here.
            // For now this is a stub -- returns default SidebarResponse and
            // an empty egui frame (no shapes).
            let _ = &app_state;
            let _ = &mut sidebar_response;
        });

        (sidebar_response, full_output)
    }
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

/// Create the render pipeline with position+color vertex layout.
fn create_render_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("veil render pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Vertex::buffer_layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}

/// Create the window-uniform bind group layout and bind group.
fn create_window_bind_group(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("window bind group layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("window bind group"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
    });

    (layout, bind_group)
}

impl Renderer {
    /// Initialize the renderer with a window.
    ///
    /// Creates the wgpu instance with backend auto-selection, requests
    /// adapter and device, configures the surface, creates the shader
    /// module, pipeline layout, render pipeline, and uniform buffer.
    ///
    /// This is async because wgpu adapter/device requests are async.
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("no suitable GPU adapter"))?;

        let (device, queue) =
            adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await?;

        let inner_size = window.inner_size();
        let width = inner_size.width.max(1);
        let height = inner_size.height.max(1);

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .first()
            .copied()
            .ok_or_else(|| anyhow::anyhow!("no supported surface format"))?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("veil shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        // Uniform buffer for window dimensions (8 bytes).
        #[allow(clippy::cast_precision_loss)]
        let uniform = WindowUniform { width: width as f32, height: height as f32 };
        let window_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("window uniform buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let (bind_group_layout, window_bind_group) =
            create_window_bind_group(&device, &window_uniform_buffer);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("veil pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline =
            create_render_pipeline(&device, &pipeline_layout, &shader, surface_format);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            window_uniform_buffer,
            window_bind_group,
            size: (width, height),
        })
    }

    /// Handle window resize.
    ///
    /// Updates the surface configuration and uniform buffer with new
    /// dimensions. Called from winit's `Resized` event handler.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.size = (width, height);

        #[allow(clippy::cast_precision_loss)]
        let uniform = WindowUniform { width: width as f32, height: height as f32 };
        self.queue.write_buffer(&self.window_uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    /// Render a frame.
    ///
    /// 1. Get next surface texture
    /// 2. Create texture view
    /// 3. Build command encoder
    /// 4. Begin render pass with clear color
    /// 5. Set pipeline, bind group
    /// 6. Upload vertex/index buffers from `FrameGeometry`
    /// 7. Draw indexed
    /// 8. Submit and present
    ///
    /// Handles surface errors:
    /// - `Lost` / `Outdated`: calls `resize()` to reconfigure
    /// - `OutOfMemory`: returns `Err` (caller should exit)
    pub fn render(&mut self, frame_geometry: &FrameGeometry) -> anyhow::Result<()> {
        let output = match self.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.resize(self.size.0, self.size.1);
                return Ok(());
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                return Err(anyhow::anyhow!("GPU out of memory"));
            }
            Err(e) => {
                return Err(anyhow::anyhow!("surface error: {e}"));
            }
        };

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("veil render encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("veil render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(frame_geometry.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.window_bind_group, &[]);

            if !frame_geometry.vertices.is_empty() {
                let vertex_buffer =
                    self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("veil vertex buffer"),
                        contents: bytemuck::cast_slice(&frame_geometry.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

                let index_buffer =
                    self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("veil index buffer"),
                        contents: bytemuck::cast_slice(&frame_geometry.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });

                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);

                #[allow(clippy::cast_possible_truncation)]
                let index_count = frame_geometry.indices.len() as u32;
                render_pass.draw_indexed(0..index_count, 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Get the current surface size.
    #[allow(dead_code)]
    pub fn size(&self) -> (u32, u32) {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_uniform_size_is_8_bytes() {
        assert_eq!(std::mem::size_of::<WindowUniform>(), 8);
    }

    #[test]
    fn vertex_buffer_stride_matches_vertex_size() {
        assert_eq!(std::mem::size_of::<Vertex>(), 24);
    }

    // ================================================================
    // EguiIntegration — headless construction
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
    // EguiIntegration::run_frame — sidebar produces egui shapes
    // ================================================================

    #[test]
    fn run_frame_produces_shapes_when_sidebar_visible() {
        // When the sidebar is visible and contains UI widgets (tab buttons,
        // workspace list), the egui frame should produce shapes to paint.
        // The stub returns an empty frame, so this is RED.
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
        // The stub never calls egui UI functions, so no textures are generated.
        // This is RED until run_frame actually calls render_sidebar.
        let integration = EguiIntegration::new_headless();
        let state = AppState::new();
        let (_response, full_output) = integration.run_frame(&state);

        assert!(
            !full_output.textures_delta.set.is_empty(),
            "first egui frame should request at least one texture upload (font atlas)"
        );
    }

    // ================================================================
    // EguiIntegration::run_frame — sidebar response reflects state
    // ================================================================

    #[test]
    fn run_frame_without_interaction_returns_default_response() {
        // Without simulated clicks, the sidebar should return a default
        // (no-op) response. This verifies the basic contract.
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
    // EguiIntegration::run_frame — workspace entries rendered
    // ================================================================

    #[test]
    fn run_frame_with_workspaces_produces_shapes() {
        use std::path::PathBuf;

        // Create state with actual workspaces so the sidebar has content.
        let integration = EguiIntegration::new_headless();
        let mut state = AppState::new();
        state.create_workspace("project-alpha".to_string(), PathBuf::from("/tmp/alpha"));
        state.create_workspace("project-beta".to_string(), PathBuf::from("/tmp/beta"));

        let (_response, full_output) = integration.run_frame(&state);

        // The workspace list should render entries, producing shapes.
        assert!(
            !full_output.shapes.is_empty(),
            "run_frame with workspaces should produce shapes for workspace list entries"
        );
    }

    // ================================================================
    // EguiIntegration::run_frame — conversations tab
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
    // EguiIntegration::run_frame — hidden sidebar
    // ================================================================

    #[test]
    fn run_frame_hidden_sidebar_still_returns_output() {
        // Even when sidebar is hidden, run_frame should still produce a
        // valid FullOutput (the caller decides whether to call run_frame
        // based on sidebar visibility, but the method itself should work).
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
    // EguiIntegration::run_frame — consecutive frames share context
    // ================================================================

    #[test]
    fn run_frame_consecutive_frames_share_context() {
        // Running multiple frames on the same EguiIntegration should work.
        // The egui context accumulates state across frames. After the first
        // frame, subsequent frames should also produce shapes (the sidebar
        // is persistent, not a one-shot).
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
