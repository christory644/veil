//! GPU renderer for Veil's terminal UI.
//!
//! Owns all wgpu state: device, queue, surface, pipeline, buffers.
//! Created once at startup, resized on window resize, renders each frame.

use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::egui_integration::EguiIntegration;
use crate::frame::FrameGeometry;
use crate::vertex::Vertex;

/// Uniform buffer data matching the WGSL `WindowUniform` struct.
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
    /// egui integration for sidebar rendering.
    pub egui: EguiIntegration,
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
        multiview_mask: None,
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
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await?;

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
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let render_pipeline =
            create_render_pipeline(&device, &pipeline_layout, &shader, surface_format);

        let egui = EguiIntegration::new(&window, &device, surface_format);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            window_uniform_buffer,
            window_bind_group,
            size: (width, height),
            egui,
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

    /// Render a complete frame: terminal geometry + optional egui overlay.
    ///
    /// 1. Get next surface texture
    /// 2. Create texture view and command encoder
    /// 3. Terminal render pass (clear + indexed quads)
    /// 4. If `egui_full_output` is `Some`, render egui in a second pass
    ///    with `LoadOp::Load` (composite on top)
    /// 5. Submit and present
    ///
    /// Handles surface errors:
    /// - `Lost` / `Outdated`: calls `resize()` to reconfigure
    /// - `Validation`: returns `Err` (caller should exit)
    pub fn render(
        &mut self,
        frame_geometry: &FrameGeometry,
        egui_full_output: Option<egui::FullOutput>,
    ) -> anyhow::Result<()> {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Occluded => {
                self.resize(self.size.0, self.size.1);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(anyhow::anyhow!("surface validation error"));
            }
        };

        let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("veil render encoder"),
        });

        // Pass 1: terminal quads
        self.render_terminal_pass(&mut encoder, &view, frame_geometry);

        // Pass 2: egui overlay (composited on top of terminal)
        if let Some(full_output) = egui_full_output {
            let surface_size = [self.config.width, self.config.height];
            self.egui.render(
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                surface_size,
                full_output,
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        Ok(())
    }

    /// Execute the terminal render pass: clear background and draw indexed quads.
    fn render_terminal_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        frame_geometry: &FrameGeometry,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("veil render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(frame_geometry.clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.window_bind_group, &[]);

        if !frame_geometry.vertices.is_empty() {
            let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("veil vertex buffer"),
                contents: bytemuck::cast_slice(&frame_geometry.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
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
}
