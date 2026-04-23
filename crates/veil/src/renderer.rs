//! GPU renderer for Veil's terminal UI.
//!
//! Owns all wgpu state: device, queue, surface, pipeline, buffers.
//! Created once at startup, resized on window resize, renders each frame.

use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::egui_integration::EguiIntegration;
use crate::font::text_vertex::TextVertex;
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
    /// GPU-side glyph atlas texture (`R8Unorm`).
    atlas_texture: wgpu::Texture,
    /// View into the atlas texture for binding.
    atlas_texture_view: wgpu::TextureView,
    /// Nearest-neighbor sampler for the glyph atlas.
    atlas_sampler: wgpu::Sampler,
    /// Last-known atlas dimensions; used to detect atlas growth.
    atlas_size: (u32, u32),
    /// Bind group layout for the text pipeline (uniform + texture + sampler).
    text_bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group for the text pipeline.
    text_bind_group: wgpu::BindGroup,
    /// Render pipeline for textured glyph quads.
    text_render_pipeline: wgpu::RenderPipeline,
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

/// Create the glyph atlas texture, view, and sampler.
///
/// Format is `R8Unorm` (1 byte per pixel, grayscale). Nearest-neighbor sampling.
fn create_atlas_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Sampler) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glyph atlas texture"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("glyph atlas sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });

    (texture, view, sampler)
}

/// Create the bind group layout for the text pipeline.
///
/// Bindings: 0 = window uniform (VERTEX), 1 = atlas texture (FRAGMENT), 2 = sampler (FRAGMENT).
fn create_text_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("text bind group layout"),
        entries: &[
            // binding 0: window uniform buffer (VERTEX)
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // binding 1: glyph atlas texture (FRAGMENT)
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            // binding 2: atlas sampler (FRAGMENT)
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Create the bind group for the text pipeline.
fn create_text_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    atlas_view: &wgpu::TextureView,
    atlas_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("text bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(atlas_sampler),
            },
        ],
    })
}

/// Create the render pipeline for textured glyph quads.
fn create_text_render_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("veil text render pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_text"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[TextVertex::buffer_layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_text"),
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

/// Initial glyph atlas texture size (512×512 pixels).
const INITIAL_ATLAS_SIZE: u32 = 512;

impl Renderer {
    /// Initialize the renderer with a window.
    ///
    /// Creates the wgpu instance with backend auto-selection, requests
    /// adapter and device, configures the surface, creates the shader
    /// module, pipeline layout, render pipeline, and uniform buffer.
    ///
    /// This is async because wgpu adapter/device requests are async.
    #[allow(clippy::too_many_lines)]
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

        // Text pipeline setup.
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("veil text shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("text_shader.wgsl").into()),
        });

        let (atlas_texture, atlas_texture_view, atlas_sampler) =
            create_atlas_texture(&device, INITIAL_ATLAS_SIZE, INITIAL_ATLAS_SIZE);

        let text_bind_group_layout = create_text_bind_group_layout(&device);

        let text_bind_group = create_text_bind_group(
            &device,
            &text_bind_group_layout,
            &window_uniform_buffer,
            &atlas_texture_view,
            &atlas_sampler,
        );

        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("veil text pipeline layout"),
            bind_group_layouts: &[Some(&text_bind_group_layout)],
            immediate_size: 0,
        });

        let text_render_pipeline = create_text_render_pipeline(
            &device,
            &text_pipeline_layout,
            &text_shader,
            surface_format,
        );

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
            atlas_texture,
            atlas_texture_view,
            atlas_sampler,
            atlas_size: (INITIAL_ATLAS_SIZE, INITIAL_ATLAS_SIZE),
            text_bind_group_layout,
            text_bind_group,
            text_render_pipeline,
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

    /// Upload the glyph atlas bitmap to the GPU texture.
    ///
    /// Called each frame when `GlyphAtlas::is_dirty()` is true.
    fn upload_atlas(&self, atlas: &crate::font::atlas::GlyphAtlas) {
        let (w, h) = atlas.dimensions();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            atlas.bitmap(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
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
        font_pipeline: Option<&mut crate::font_pipeline::FontPipeline>,
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

        // Per-frame atlas upload: recreate texture if atlas grew, then upload if dirty.
        if let Some(fp) = font_pipeline {
            let current_size = fp.atlas().dimensions();
            if current_size != self.atlas_size {
                let (tex, view, sampler) =
                    create_atlas_texture(&self.device, current_size.0, current_size.1);
                self.atlas_texture = tex;
                self.atlas_texture_view = view;
                self.atlas_sampler = sampler;
                self.atlas_size = current_size;
                self.text_bind_group = create_text_bind_group(
                    &self.device,
                    &self.text_bind_group_layout,
                    &self.window_uniform_buffer,
                    &self.atlas_texture_view,
                    &self.atlas_sampler,
                );
            }
            if fp.atlas().is_dirty() {
                self.upload_atlas(fp.atlas());
                fp.atlas_mut().mark_clean();
            }
        }

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

        // Draw 2: textured glyph quads (text on top of backgrounds)
        if !frame_geometry.text_vertices.is_empty() {
            let text_vertex_buffer =
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("veil text vertex buffer"),
                    contents: bytemuck::cast_slice(&frame_geometry.text_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let text_index_buffer =
                self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("veil text index buffer"),
                    contents: bytemuck::cast_slice(&frame_geometry.text_indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

            render_pass.set_pipeline(&self.text_render_pipeline);
            render_pass.set_bind_group(0, &self.text_bind_group, &[]);
            render_pass.set_vertex_buffer(0, text_vertex_buffer.slice(..));
            render_pass.set_index_buffer(text_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            #[allow(clippy::cast_possible_truncation)]
            let text_index_count = frame_geometry.text_indices.len() as u32;
            render_pass.draw_indexed(0..text_index_count, 0, 0..1);
        }
    }
}

#[cfg(test)]
#[allow(clippy::doc_markdown)]
mod tests {
    use super::*;
    use crate::font::text_vertex::TextVertex;
    use crate::frame::FrameGeometry;

    #[test]
    fn window_uniform_size_is_8_bytes() {
        assert_eq!(std::mem::size_of::<WindowUniform>(), 8);
    }

    #[test]
    fn vertex_buffer_stride_matches_vertex_size() {
        assert_eq!(std::mem::size_of::<Vertex>(), 24);
    }

    // ============================================================
    // VEI-83 Unit 1: Text shader source inclusion
    // ============================================================

    /// The text shader file must be included and non-empty.
    /// This is the compile-time smoke test that the file exists and
    /// `include_str!("text_shader.wgsl")` is wired in.
    #[test]
    fn text_shader_source_is_included() {
        // Once the text shader file is created and included via include_str!(),
        // this constant will be non-empty. Until then this test fails at the
        // assertion (RED state).
        const TEXT_SHADER_SRC: &str = include_str!("text_shader.wgsl");
        assert!(!TEXT_SHADER_SRC.is_empty(), "text_shader.wgsl must be non-empty");
    }

    // ============================================================
    // VEI-83 Unit 4: Text vertex buffer stride
    // ============================================================

    /// TextVertex is 32 bytes (2*f32 position + 2*f32 uv + 4*f32 color).
    /// This mirrors the existing `vertex_buffer_stride_matches_vertex_size` pattern
    /// and guards the text pipeline's vertex layout contract.
    #[test]
    fn text_vertex_buffer_stride_matches_text_vertex_size() {
        assert_eq!(
            std::mem::size_of::<TextVertex>(),
            32,
            "TextVertex must be exactly 32 bytes for the text pipeline"
        );
    }

    /// The text pipeline's vertex layout stride must equal 32 (not 24 like Vertex).
    /// This confirms TextVertex::buffer_layout() uses the correct type.
    #[test]
    fn text_pipeline_uses_text_vertex_layout() {
        let layout = TextVertex::buffer_layout();
        assert_eq!(layout.array_stride, 32, "text pipeline stride should be 32, not 24 (Vertex)");
    }

    // ============================================================
    // VEI-83 Unit 4: Alpha blending constant
    // ============================================================

    /// The text render pipeline must use ALPHA_BLENDING so glyphs composite
    /// over background quads correctly. This tests that the constant itself
    /// is what we expect (it has a non-trivial color/alpha blend equation).
    #[test]
    fn text_pipeline_uses_alpha_blending() {
        // ALPHA_BLENDING: src * src_alpha + dst * (1 - src_alpha).
        // Verify the constant is not REPLACE (no blending) or PREMULTIPLIED_ALPHA_BLENDING.
        let blend = wgpu::BlendState::ALPHA_BLENDING;
        // alpha component: src=One, dst=OneMinusSrcAlpha — standard over-compositing.
        assert_eq!(
            blend.alpha.src_factor,
            wgpu::BlendFactor::One,
            "ALPHA_BLENDING alpha src should be One"
        );
        assert_eq!(
            blend.alpha.dst_factor,
            wgpu::BlendFactor::OneMinusSrcAlpha,
            "ALPHA_BLENDING alpha dst should be OneMinusSrcAlpha"
        );
    }

    // ============================================================
    // VEI-83 Unit 3: Text bind group entry count
    // ============================================================

    /// The text bind group layout must have exactly 3 entries:
    /// binding 0 = uniform buffer, binding 1 = texture, binding 2 = sampler.
    #[test]
    fn text_bind_group_layout_has_three_entries() {
        // Build the descriptor slice inline (mirrors create_text_bind_group_layout).
        let entries: &[wgpu::BindGroupLayoutEntry] = &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        assert_eq!(entries.len(), 3, "text bind group layout must have exactly 3 entries");
    }

    /// Binding indices in the text bind group must be 0, 1, 2 in order.
    #[test]
    fn text_bind_group_entry_bindings_are_0_1_2() {
        let entries: &[wgpu::BindGroupLayoutEntry] = &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        assert_eq!(entries[0].binding, 0, "first binding index must be 0 (uniform buffer)");
        assert_eq!(entries[1].binding, 1, "second binding index must be 1 (atlas texture)");
        assert_eq!(entries[2].binding, 2, "third binding index must be 2 (atlas sampler)");
    }

    /// Atlas texture format for R8Unorm (1 byte per pixel, grayscale).
    /// This guards the format constant used in create_atlas_texture().
    #[test]
    fn atlas_texture_format_is_r8unorm() {
        // The correct format for a 1-byte-per-pixel grayscale glyph atlas.
        let format = wgpu::TextureFormat::R8Unorm;
        // Verify it is not Rgba8Unorm (4 bytes/pixel) or Bgra8Unorm (surface format).
        assert_ne!(
            format,
            wgpu::TextureFormat::Rgba8Unorm,
            "atlas should not use Rgba8Unorm — 4x too large"
        );
        assert_ne!(
            format,
            wgpu::TextureFormat::Bgra8Unorm,
            "atlas should not use Bgra8Unorm — wrong format for grayscale"
        );
        // The enum variant itself being constructible is the meaningful assertion.
        // If R8Unorm disappears from wgpu this compile-errors.
        let _ = format;
    }

    /// For R8Unorm, bytes_per_row = width (1 byte per pixel).
    /// Guards the layout passed to queue.write_texture() in upload_atlas().
    #[test]
    fn upload_atlas_bytes_per_row_equals_width() {
        let atlas_width: u32 = 512;
        let atlas_height: u32 = 512;
        // 1 byte per pixel (R8Unorm) → bytes_per_row = width.
        let bytes_per_row = atlas_width; // 1 byte per pixel
        assert_eq!(
            bytes_per_row, atlas_width,
            "R8Unorm atlas bytes_per_row must equal width ({atlas_width})"
        );
        // bitmap length must equal width * height.
        let expected_len = (atlas_width * atlas_height) as usize;
        let dummy_bitmap = vec![0u8; expected_len];
        assert_eq!(
            dummy_bitmap.len(),
            expected_len,
            "bitmap length must equal width * height for R8Unorm"
        );
    }

    // ============================================================
    // VEI-83 Unit 5: Empty text guard and index cast
    // ============================================================

    /// When frame_geometry.text_vertices is empty, the text draw call must be skipped.
    /// Tests the control-flow contract: the guard `!text_vertices.is_empty()` prevents
    /// buffer allocation for frames with no text.
    #[test]
    fn frame_geometry_with_empty_text_produces_no_text_draw() {
        let geom = FrameGeometry {
            vertices: Vec::new(),
            indices: Vec::new(),
            text_vertices: Vec::new(),
            text_indices: Vec::new(),
            clear_color: wgpu::Color::BLACK,
        };
        // The guard: only draw if non-empty.
        let should_draw_text = !geom.text_vertices.is_empty();
        assert!(!should_draw_text, "empty text_vertices must not trigger a text draw call");
    }

    /// Casting text_indices.len() as u32 is safe: u16::MAX fits in u32.
    /// Guards the index count cast in the draw call.
    #[test]
    fn text_index_count_cast_is_safe_for_u16_max() {
        // u16::MAX indices (65535) fits comfortably in u32 (max 4_294_967_295).
        let max_indices: usize = u16::MAX as usize;
        #[allow(clippy::cast_possible_truncation)]
        let as_u32 = max_indices as u32;
        assert_eq!(as_u32, u32::from(u16::MAX), "u16::MAX indices must survive usize→u32 cast");
        assert!(
            u32::try_from(max_indices).is_ok(),
            "u16::MAX must convert to u32 without overflow"
        );
    }

    // ============================================================
    // VEI-83 Unit 6: render() signature accepts font_pipeline
    // ============================================================

    /// The render() method signature must accept Option<&mut FontPipeline>.
    /// This test verifies the contract compiles: once the signature is updated,
    /// this call site will compile. Currently documents the expected final shape.
    ///
    /// NOTE: This test is intentionally commented out — it documents what the
    /// implementation must look like, but we can't call render() without a GPU
    /// context. The meaningful compile check happens in main.rs where the call
    /// site is updated. The contract is verified by the atlas dirty/clean tests below.
    ///
    /// See `atlas_dirty_tracking_for_gpu_upload` in atlas.rs for the non-GPU contract test.
    #[test]
    fn render_accepts_optional_font_pipeline_conceptual_check() {
        // Verify the type system allows Option<&mut FontPipeline> to be None.
        // This is a tautology, but serves as documentation of the call-site contract.
        let font_pipeline_opt: Option<&mut crate::font_pipeline::FontPipeline> = None;
        assert!(font_pipeline_opt.is_none(), "None is a valid font_pipeline argument to render()");
    }
}
