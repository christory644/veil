//! Vertex type and quad generation helpers for the wgpu rendering pipeline.
//!
//! Pure data types and geometry math. No GPU dependencies. Fully unit-testable.

/// A vertex with position (2D) and RGBA color.
/// 24 bytes: 2 * f32 (position) + 4 * f32 (color) = 24 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Position in pixel coordinates.
    pub position: [f32; 2],
    /// RGBA color.
    pub color: [f32; 4],
}

/// Byte offset of the `color` field within `Vertex`.
///
/// `position` is `[f32; 2]` = 8 bytes, so `color` starts at offset 8.
const COLOR_OFFSET: u64 = std::mem::size_of::<[f32; 2]>() as u64;

impl Vertex {
    /// Describe the vertex buffer layout for the wgpu pipeline.
    ///
    /// Stride is 24 bytes (2 * f32 position + 4 * f32 color).
    /// Attributes: position (`Float32x2` at offset 0), color (`Float32x4` at offset 8).
    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position: vec2<f32> at offset 0
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color: vec4<f32> at offset 8
                wgpu::VertexAttribute {
                    offset: COLOR_OFFSET,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Generate the 4 vertices for an axis-aligned quad.
///
/// Vertices are in pixel coordinates, ordered:
/// 0: top-left, 1: top-right, 2: bottom-left, 3: bottom-right
pub fn quad_vertices(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> [Vertex; 4] {
    [
        Vertex { position: [x, y], color },
        Vertex { position: [x + width, y], color },
        Vertex { position: [x, y + height], color },
        Vertex { position: [x + width, y + height], color },
    ]
}

/// Generate the 6 index values for a quad (two triangles),
/// offset by `base` to allow batching multiple quads into one
/// index buffer.
///
/// Triangles: (base+0, base+2, base+1), (base+1, base+2, base+3)
pub fn quad_indices(base: u16) -> [u16; 6] {
    [base, base + 2, base + 1, base + 1, base + 2, base + 3]
}

/// Compute the vertex base index for the Nth quad in a batch.
///
/// Each quad uses 4 vertices, so quad N starts at vertex index `N * 4`.
/// With `u16` indices, the maximum supported quad count is 16383
/// (`u16::MAX / 4`).
#[allow(clippy::cast_possible_truncation)]
pub fn vertex_base(quad_index: usize) -> u16 {
    (quad_index * 4) as u16
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    // ============================================================
    // Vertex struct properties
    // ============================================================

    #[test]
    fn vertex_size_is_24_bytes() {
        assert_eq!(std::mem::size_of::<Vertex>(), 24);
    }

    // ============================================================
    // quad_vertices — happy path
    // ============================================================

    #[test]
    fn quad_vertices_produces_four_vertices() {
        let color = [1.0, 0.0, 0.0, 1.0];
        let verts = quad_vertices(10.0, 20.0, 100.0, 50.0, color);
        assert_eq!(verts.len(), 4);
        // All should have the same color
        for v in &verts {
            assert_eq!(v.color, color);
        }
    }

    #[test]
    fn quad_vertices_positions_correct() {
        let verts = quad_vertices(10.0, 20.0, 100.0, 50.0, [1.0; 4]);
        // 0: top-left
        assert_eq!(verts[0].position, [10.0, 20.0]);
        // 1: top-right
        assert_eq!(verts[1].position, [110.0, 20.0]);
        // 2: bottom-left
        assert_eq!(verts[2].position, [10.0, 70.0]);
        // 3: bottom-right
        assert_eq!(verts[3].position, [110.0, 70.0]);
    }

    #[test]
    fn quad_vertices_all_same_color() {
        let color = [0.5, 0.3, 0.1, 0.8];
        let verts = quad_vertices(0.0, 0.0, 10.0, 10.0, color);
        for v in &verts {
            assert_eq!(v.color, color);
        }
    }

    // ============================================================
    // quad_indices — happy path
    // ============================================================

    #[test]
    fn quad_indices_base_zero() {
        let indices = quad_indices(0);
        assert_eq!(indices, [0, 2, 1, 1, 2, 3]);
    }

    #[test]
    fn quad_indices_base_four() {
        let indices = quad_indices(4);
        assert_eq!(indices, [4, 6, 5, 5, 6, 7]);
    }

    // ============================================================
    // vertex_base — happy path
    // ============================================================

    #[test]
    fn vertex_base_computation() {
        assert_eq!(vertex_base(0), 0);
        assert_eq!(vertex_base(1), 4);
        assert_eq!(vertex_base(5), 20);
    }

    // ============================================================
    // Edge cases
    // ============================================================

    #[test]
    fn quad_vertices_zero_dimensions() {
        // Should not panic; produces degenerate quad where all positions collapse
        let verts = quad_vertices(5.0, 5.0, 0.0, 0.0, [1.0; 4]);
        assert_eq!(verts.len(), 4);
        // top-left and top-right have same position (width=0)
        assert_eq!(verts[0].position, [5.0, 5.0]);
        assert_eq!(verts[1].position, [5.0, 5.0]);
        assert_eq!(verts[2].position, [5.0, 5.0]);
        assert_eq!(verts[3].position, [5.0, 5.0]);
    }

    #[test]
    fn quad_vertices_negative_coords() {
        let verts = quad_vertices(-10.0, -20.0, 30.0, 40.0, [1.0; 4]);
        assert_eq!(verts[0].position, [-10.0, -20.0]);
        assert_eq!(verts[1].position, [20.0, -20.0]);
        assert_eq!(verts[2].position, [-10.0, 20.0]);
        assert_eq!(verts[3].position, [20.0, 20.0]);
    }
}
