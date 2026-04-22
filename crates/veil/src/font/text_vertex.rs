//! Text vertex type and textured quad generation for the glyph pipeline.
//!
//! Parallels `vertex.rs` and `quad_builder.rs` but for text rendering.
//! Each text vertex carries position, UV coordinates (into the glyph atlas),
//! and a foreground RGBA color.

use crate::font::atlas::AtlasRegion;

/// Byte offset of the `uv` field within `TextVertex`.
///
/// `position` is `[f32; 2]` = 8 bytes, so `uv` starts at offset 8.
const UV_OFFSET: u64 = std::mem::size_of::<[f32; 2]>() as u64;

/// Byte offset of the `color` field within `TextVertex`.
///
/// `position` is 8 bytes + `uv` is 8 bytes = 16 bytes.
const COLOR_OFFSET: u64 =
    (std::mem::size_of::<[f32; 2]>() + std::mem::size_of::<[f32; 2]>()) as u64;

/// A vertex with position (2D), texture coordinates (UV), and foreground color.
/// 32 bytes: 2*f32 (position) + 2*f32 (uv) + 4*f32 (color) = 32 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextVertex {
    /// Position in pixel coordinates.
    pub position: [f32; 2],
    /// Texture coordinates (UV) into the glyph atlas.
    pub uv: [f32; 2],
    /// Foreground RGBA color.
    pub color: [f32; 4],
}

impl TextVertex {
    /// Describe the vertex buffer layout for the text pipeline.
    ///
    /// Stride is 32 bytes. Attributes:
    /// - position: `Float32x2` at offset 0
    /// - uv: `Float32x2` at offset 8
    /// - color: `Float32x4` at offset 16
    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TextVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position: vec2<f32> at offset 0
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // uv: vec2<f32> at offset 8
                wgpu::VertexAttribute {
                    offset: UV_OFFSET,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color: vec4<f32> at offset 16
                wgpu::VertexAttribute {
                    offset: COLOR_OFFSET,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Generate the 4 vertices for a textured glyph quad.
///
/// `cell_x`, `cell_y`: top-left corner of the cell in pixel coordinates.
/// `region`: atlas region with UV coords and glyph dimensions/bearings.
/// `ascent`: font ascent in pixels (baseline offset from top of cell).
/// `color`: foreground RGBA color.
#[allow(clippy::cast_precision_loss)] // Glyph bearings and dimensions fit comfortably in f32.
pub fn text_quad_vertices(
    cell_x: f32,
    cell_y: f32,
    region: &AtlasRegion,
    ascent: f32,
    color: [f32; 4],
) -> [TextVertex; 4] {
    let quad_x = cell_x + region.bearing_x as f32;
    let quad_y = cell_y + ascent - region.bearing_y as f32;
    let quad_w = region.width as f32;
    let quad_h = region.height as f32;

    [
        // [0] top-left
        TextVertex { position: [quad_x, quad_y], uv: [region.u_min, region.v_min], color },
        // [1] top-right
        TextVertex { position: [quad_x + quad_w, quad_y], uv: [region.u_max, region.v_min], color },
        // [2] bottom-left
        TextVertex { position: [quad_x, quad_y + quad_h], uv: [region.u_min, region.v_max], color },
        // [3] bottom-right
        TextVertex {
            position: [quad_x + quad_w, quad_y + quad_h],
            uv: [region.u_max, region.v_max],
            color,
        },
    ]
}

/// Generate 6 indices for a textured quad (two triangles).
///
/// Triangles: (base+0, base+2, base+1), (base+1, base+2, base+3)
pub fn text_quad_indices(base: u16) -> [u16; 6] {
    [base, base + 2, base + 1, base + 1, base + 2, base + 3]
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::cast_precision_loss)]
mod tests {
    use super::*;
    use crate::font::atlas::AtlasRegion;
    use bytemuck::Zeroable;

    /// Helper to create a test atlas region.
    fn test_region() -> AtlasRegion {
        AtlasRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 0.125,   // 8/64
            v_max: 0.21875, // 14/64
            width: 8,
            height: 14,
            bearing_x: 1,
            bearing_y: 12,
        }
    }

    // ============================================================
    // TextVertex struct properties
    // ============================================================

    #[test]
    fn text_vertex_size_is_32_bytes() {
        assert_eq!(std::mem::size_of::<TextVertex>(), 32);
    }

    #[test]
    fn text_vertex_is_pod_and_zeroable() {
        // These compile if and only if the derives are correct.
        let zeroed = TextVertex::zeroed();
        assert_eq!(zeroed.position, [0.0, 0.0]);
        assert_eq!(zeroed.uv, [0.0, 0.0]);
        assert_eq!(zeroed.color, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn text_vertex_buffer_layout_stride() {
        let layout = TextVertex::buffer_layout();
        assert_eq!(layout.array_stride, 32, "stride should be 32 bytes");
    }

    #[test]
    fn text_vertex_buffer_layout_attribute_count() {
        let layout = TextVertex::buffer_layout();
        assert_eq!(layout.attributes.len(), 3, "should have 3 attributes: position, uv, color");
    }

    #[test]
    fn text_vertex_buffer_layout_attribute_offsets() {
        let layout = TextVertex::buffer_layout();
        assert_eq!(layout.attributes[0].offset, 0, "position at offset 0");
        assert_eq!(layout.attributes[1].offset, 8, "uv at offset 8");
        assert_eq!(layout.attributes[2].offset, 16, "color at offset 16");
    }

    #[test]
    fn text_vertex_buffer_layout_attribute_formats() {
        let layout = TextVertex::buffer_layout();
        assert_eq!(layout.attributes[0].format, wgpu::VertexFormat::Float32x2, "position format");
        assert_eq!(layout.attributes[1].format, wgpu::VertexFormat::Float32x2, "uv format");
        assert_eq!(layout.attributes[2].format, wgpu::VertexFormat::Float32x4, "color format");
    }

    // ============================================================
    // text_quad_vertices — happy path
    // ============================================================

    #[test]
    fn text_quad_vertices_produces_four_vertices() {
        let region = test_region();
        let verts = text_quad_vertices(100.0, 200.0, &region, 14.0, [1.0; 4]);
        assert_eq!(verts.len(), 4);
    }

    #[test]
    fn text_quad_vertices_uv_matches_region() {
        let region = test_region();
        let verts = text_quad_vertices(100.0, 200.0, &region, 14.0, [1.0; 4]);

        // Top-left vertex should have (u_min, v_min)
        assert_eq!(verts[0].uv, [region.u_min, region.v_min], "top-left UV");
        // Top-right vertex should have (u_max, v_min)
        assert_eq!(verts[1].uv, [region.u_max, region.v_min], "top-right UV");
        // Bottom-left vertex should have (u_min, v_max)
        assert_eq!(verts[2].uv, [region.u_min, region.v_max], "bottom-left UV");
        // Bottom-right vertex should have (u_max, v_max)
        assert_eq!(verts[3].uv, [region.u_max, region.v_max], "bottom-right UV");
    }

    #[test]
    fn text_quad_vertices_positions_use_bearing_and_ascent() {
        let region = test_region();
        let cell_x = 100.0;
        let cell_y = 200.0;
        let ascent = 14.0;
        let verts = text_quad_vertices(cell_x, cell_y, &region, ascent, [1.0; 4]);

        // Expected quad position:
        // quad_x = cell_x + bearing_x = 100 + 1 = 101
        // quad_y = cell_y + ascent - bearing_y = 200 + 14 - 12 = 202
        // quad_width = region.width = 8
        // quad_height = region.height = 14
        let expected_x = cell_x + region.bearing_x as f32;
        let expected_y = cell_y + ascent - region.bearing_y as f32;

        // Top-left
        assert_eq!(verts[0].position, [expected_x, expected_y], "top-left position");
        // Top-right
        assert_eq!(
            verts[1].position,
            [expected_x + region.width as f32, expected_y],
            "top-right position"
        );
        // Bottom-left
        assert_eq!(
            verts[2].position,
            [expected_x, expected_y + region.height as f32],
            "bottom-left position"
        );
        // Bottom-right
        assert_eq!(
            verts[3].position,
            [expected_x + region.width as f32, expected_y + region.height as f32],
            "bottom-right position"
        );
    }

    #[test]
    fn text_quad_vertices_all_same_color() {
        let region = test_region();
        let color = [0.8, 0.9, 1.0, 1.0];
        let verts = text_quad_vertices(0.0, 0.0, &region, 14.0, color);
        for (i, v) in verts.iter().enumerate() {
            assert_eq!(v.color, color, "vertex {i} should have the specified color");
        }
    }

    // ============================================================
    // text_quad_indices — happy path
    // ============================================================

    #[test]
    fn text_quad_indices_base_zero() {
        let indices = text_quad_indices(0);
        assert_eq!(indices, [0, 2, 1, 1, 2, 3]);
    }

    #[test]
    fn text_quad_indices_base_four() {
        let indices = text_quad_indices(4);
        assert_eq!(indices, [4, 6, 5, 5, 6, 7]);
    }

    // ============================================================
    // Edge cases
    // ============================================================

    #[test]
    fn text_quad_vertices_zero_dimension_region() {
        let region = AtlasRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 0.0,
            v_max: 0.0,
            width: 0,
            height: 0,
            bearing_x: 0,
            bearing_y: 0,
        };
        // Should not panic, produces degenerate quad.
        let verts = text_quad_vertices(50.0, 50.0, &region, 14.0, [1.0; 4]);
        assert_eq!(verts.len(), 4);
    }

    #[test]
    fn text_quad_vertices_negative_bearing_x() {
        let region = AtlasRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 0.125,
            v_max: 0.21875,
            width: 8,
            height: 14,
            bearing_x: -2, // negative: glyph extends left of cell
            bearing_y: 12,
        };
        let verts = text_quad_vertices(100.0, 200.0, &region, 14.0, [1.0; 4]);
        // quad_x = 100 + (-2) = 98 -- glyph extends left of cell origin
        assert_eq!(verts[0].position[0], 98.0, "negative bearing_x should shift left");
    }

    #[test]
    fn text_quad_vertices_bearing_y_larger_than_ascent() {
        let region = AtlasRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 0.125,
            v_max: 0.21875,
            width: 8,
            height: 14,
            bearing_x: 1,
            bearing_y: 20, // larger than ascent (14)
        };
        let verts = text_quad_vertices(100.0, 200.0, &region, 14.0, [1.0; 4]);
        // quad_y = 200 + 14 - 20 = 194 -- glyph extends above cell top
        assert_eq!(verts[0].position[1], 194.0, "large bearing_y should extend above cell");
    }
}
