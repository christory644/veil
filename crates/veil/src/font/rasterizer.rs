//! Glyph rasterization with swash.
//!
//! Takes a glyph ID and a `FontData` reference, rasterizes it to a grayscale
//! bitmap using swash's `ScaleContext`. Returns the bitmap data, dimensions,
//! and bearing offsets needed for correct placement.

use crate::font::loader::FontData;

/// A rasterized glyph bitmap with placement metadata.
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    /// Glyph ID this bitmap was rasterized from.
    pub glyph_id: u16,
    /// Grayscale bitmap data (1 byte per pixel, alpha/coverage values).
    pub bitmap: Vec<u8>,
    /// Bitmap width in pixels.
    pub width: u32,
    /// Bitmap height in pixels.
    pub height: u32,
    /// Horizontal bearing: offset from the cell origin to the left edge
    /// of the bitmap, in pixels.
    pub bearing_x: i32,
    /// Vertical bearing: offset from the baseline to the top edge of
    /// the bitmap, in pixels (positive upward in font coordinates).
    pub bearing_y: i32,
}

/// Rasterizes glyphs using swash.
///
/// Holds a `ScaleContext` which caches internal scaling state for
/// performance across multiple rasterize calls with the same font.
pub struct Rasterizer {
    context: swash::scale::ScaleContext,
}

impl Rasterizer {
    /// Create a new rasterizer.
    pub fn new() -> Self {
        Self { context: swash::scale::ScaleContext::new() }
    }

    /// Rasterize a single glyph at the given size.
    ///
    /// Returns `None` if the glyph cannot be rasterized (e.g., notdef
    /// glyph with no outline, or a glyph ID that doesn't exist).
    pub fn rasterize(&mut self, font_data: &FontData, glyph_id: u16) -> Option<RasterizedGlyph> {
        let font_ref = font_data.font_ref();
        let mut scaler = self.context.builder(font_ref).size(font_data.size_px()).build();
        let image = swash::scale::Render::new(&[
            swash::scale::Source::ColorOutline(0),
            swash::scale::Source::Outline,
        ])
        .format(swash::zeno::Format::Alpha)
        .render(&mut scaler, glyph_id)?;

        Some(RasterizedGlyph {
            glyph_id,
            bitmap: image.data,
            width: image.placement.width,
            height: image.placement.height,
            bearing_x: image.placement.left,
            bearing_y: image.placement.top,
        })
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::font::loader::{FontConfig, FontData};
    use std::path::PathBuf;

    /// Get the glyph ID for a character from a loaded font.
    ///
    /// Useful in tests to get glyph IDs without going through the shaper.
    fn glyph_id_for_char(font_data: &FontData, ch: char) -> u16 {
        let font_ref = font_data.font_ref();
        font_ref.charmap().map(ch)
    }

    /// Path to the test font fixture.
    fn test_font_path() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/test_fixtures/test_font.ttf"))
    }

    /// Load the test font data at 14pt / 96 DPI.
    fn test_font_data() -> FontData {
        let config = FontConfig { path: Some(test_font_path()), size_pt: 14.0, dpi: 96.0 };
        FontData::load(&config).expect("test font should load")
    }

    // ============================================================
    // Happy path
    // ============================================================

    #[test]
    fn rasterize_letter_a_returns_some() {
        let font_data = test_font_data();
        let glyph_id = glyph_id_for_char(&font_data, 'A');
        assert_ne!(glyph_id, 0, "'A' should have a non-zero glyph ID");

        let mut rasterizer = Rasterizer::new();
        let result = rasterizer.rasterize(&font_data, glyph_id);
        assert!(result.is_some(), "rasterizing 'A' should return Some");
    }

    #[test]
    fn rasterized_glyph_has_nonzero_dimensions() {
        let font_data = test_font_data();
        let glyph_id = glyph_id_for_char(&font_data, 'A');
        let mut rasterizer = Rasterizer::new();
        let glyph = rasterizer.rasterize(&font_data, glyph_id).unwrap();
        assert!(glyph.width > 0, "rasterized glyph should have non-zero width");
        assert!(glyph.height > 0, "rasterized glyph should have non-zero height");
    }

    #[test]
    fn rasterized_bitmap_length_matches_dimensions() {
        let font_data = test_font_data();
        let glyph_id = glyph_id_for_char(&font_data, 'A');
        let mut rasterizer = Rasterizer::new();
        let glyph = rasterizer.rasterize(&font_data, glyph_id).unwrap();
        assert_eq!(
            glyph.bitmap.len(),
            (glyph.width * glyph.height) as usize,
            "bitmap length should equal width * height"
        );
    }

    #[test]
    fn rasterized_bitmap_has_nonzero_pixels() {
        let font_data = test_font_data();
        let glyph_id = glyph_id_for_char(&font_data, 'A');
        let mut rasterizer = Rasterizer::new();
        let glyph = rasterizer.rasterize(&font_data, glyph_id).unwrap();
        let has_visible = glyph.bitmap.iter().any(|&b| b > 0);
        assert!(has_visible, "rasterized 'A' should have visible (non-zero) pixels");
    }

    #[test]
    fn rasterized_bearing_y_is_positive() {
        let font_data = test_font_data();
        let glyph_id = glyph_id_for_char(&font_data, 'A');
        let mut rasterizer = Rasterizer::new();
        let glyph = rasterizer.rasterize(&font_data, glyph_id).unwrap();
        assert!(glyph.bearing_y > 0, "bearing_y for 'A' should be positive (glyph above baseline)");
    }

    #[test]
    fn rasterized_glyph_id_matches_input() {
        let font_data = test_font_data();
        let glyph_id = glyph_id_for_char(&font_data, 'A');
        let mut rasterizer = Rasterizer::new();
        let glyph = rasterizer.rasterize(&font_data, glyph_id).unwrap();
        assert_eq!(glyph.glyph_id, glyph_id, "returned glyph_id should match input");
    }

    // ============================================================
    // Error cases
    // ============================================================

    #[test]
    fn rasterize_notdef_does_not_panic() {
        let font_data = test_font_data();
        let mut rasterizer = Rasterizer::new();
        // glyph_id 0 is the notdef glyph. Should not panic.
        let _result = rasterizer.rasterize(&font_data, 0);
    }

    #[test]
    fn rasterize_out_of_range_glyph_id_returns_none() {
        let font_data = test_font_data();
        let mut rasterizer = Rasterizer::new();
        let result = rasterizer.rasterize(&font_data, 65535);
        assert!(result.is_none(), "out-of-range glyph_id should return None");
    }

    // ============================================================
    // Edge cases
    // ============================================================

    #[test]
    fn rasterize_space_does_not_panic() {
        let font_data = test_font_data();
        let glyph_id = glyph_id_for_char(&font_data, ' ');
        let mut rasterizer = Rasterizer::new();
        // Space may return Some with 0x0 bitmap or None -- both are fine.
        let _result = rasterizer.rasterize(&font_data, glyph_id);
    }

    #[test]
    fn rasterize_at_small_size_does_not_panic() {
        let config = FontConfig { path: Some(test_font_path()), size_pt: 1.0, dpi: 96.0 };
        let font_data = FontData::load(&config).expect("should load at small size");
        let glyph_id = glyph_id_for_char(&font_data, 'A');
        let mut rasterizer = Rasterizer::new();
        let _result = rasterizer.rasterize(&font_data, glyph_id);
    }

    #[test]
    fn rasterize_at_large_size_does_not_panic() {
        let config = FontConfig { path: Some(test_font_path()), size_pt: 200.0, dpi: 96.0 };
        let font_data = FontData::load(&config).expect("should load at large size");
        let glyph_id = glyph_id_for_char(&font_data, 'A');
        let mut rasterizer = Rasterizer::new();
        let result = rasterizer.rasterize(&font_data, glyph_id);
        // Should produce valid output without panic.
        if let Some(glyph) = result {
            assert!(glyph.width > 0, "large glyph should have non-zero width");
            assert!(glyph.height > 0, "large glyph should have non-zero height");
        }
    }

    #[test]
    fn rasterize_multiple_glyphs_uses_same_context() {
        let font_data = test_font_data();
        let mut rasterizer = Rasterizer::new();
        // Rasterize several different glyphs with the same rasterizer instance.
        for ch in ['A', 'B', 'C', 'x', 'y', 'z'] {
            let glyph_id = glyph_id_for_char(&font_data, ch);
            let result = rasterizer.rasterize(&font_data, glyph_id);
            assert!(result.is_some(), "rasterizing '{ch}' should succeed");
        }
    }
}
