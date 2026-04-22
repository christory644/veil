//! Text shaping with rustybuzz.
//!
//! Takes a string and a `FontData` reference, runs it through rustybuzz,
//! and returns a list of positioned glyph IDs. For the initial implementation,
//! shapes per-cell (single character at a time) without ligature support.

use crate::font::loader::FontData;

/// A shaped glyph with its ID and positioning offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShapedGlyph {
    /// Glyph ID within the font.
    pub glyph_id: u16,
    /// Cluster index (maps back to the source character index).
    pub cluster: u32,
    /// Horizontal offset from the cell origin in pixels.
    pub x_offset: i32,
    /// Vertical offset from the baseline in pixels.
    pub y_offset: i32,
    /// Horizontal advance in pixels (should equal `cell_width` for monospace).
    pub x_advance: i32,
}

/// Shapes text into positioned glyphs using rustybuzz.
pub struct Shaper {
    /// Owned copy of the font data for `rustybuzz::Face`.
    /// `rustybuzz::Face` borrows from this.
    face_data: Vec<u8>,
    /// Font index within the file.
    face_index: u32,
    /// Font size in pixels.
    size_px: f32,
}

impl Shaper {
    /// Create a new shaper from font data.
    pub fn new(font_data: &FontData) -> anyhow::Result<Self> {
        let face_data = font_data.data().to_vec();
        let face_index = font_data.index();

        // Validate that the data can produce a valid rustybuzz Face.
        rustybuzz::Face::from_slice(&face_data, face_index)
            .ok_or_else(|| anyhow::anyhow!("failed to parse font data for shaping"))?;

        Ok(Self { face_data, face_index, size_px: font_data.size_px() })
    }

    /// Shape a text string, returning positioned glyphs.
    ///
    /// For this initial implementation, shapes the full string as a single
    /// run with default script/language detection. Each character maps to
    /// one glyph (ligatures are VEI-44).
    pub fn shape(&self, text: &str) -> Vec<ShapedGlyph> {
        if text.is_empty() {
            return Vec::new();
        }

        let face = rustybuzz::Face::from_slice(&self.face_data, self.face_index)
            .expect("Face was validated in Shaper::new");

        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(rustybuzz::Direction::LeftToRight);

        let glyph_buffer = rustybuzz::shape(&face, &[], buffer);

        #[allow(clippy::cast_precision_loss)]
        let scale = self.size_px / face.units_per_em() as f32;
        let infos = glyph_buffer.glyph_infos();
        let positions = glyph_buffer.glyph_positions();

        infos
            .iter()
            .zip(positions.iter())
            .map(|(info, pos)| {
                #[allow(clippy::cast_possible_truncation)]
                let glyph_id = info.glyph_id as u16;
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let x_offset = (f64::from(pos.x_offset) * f64::from(scale)).round() as i32;
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let y_offset = (f64::from(pos.y_offset) * f64::from(scale)).round() as i32;
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let x_advance = (f64::from(pos.x_advance) * f64::from(scale)).round() as i32;
                ShapedGlyph { glyph_id, cluster: info.cluster, x_offset, y_offset, x_advance }
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::font::loader::{FontConfig, FontData};
    use std::path::PathBuf;

    /// Path to the test font fixture (Hack Regular, MIT license).
    fn test_font_path() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/test_fixtures/test_font.ttf"))
    }

    /// Load the test font data at 14pt / 96 DPI.
    fn test_font_data() -> FontData {
        let config = FontConfig { path: Some(test_font_path()), size_pt: 14.0, dpi: 96.0 };
        FontData::load(&config).expect("test font should load")
    }

    /// Create a shaper from the test font.
    fn test_shaper() -> Shaper {
        let font_data = test_font_data();
        Shaper::new(&font_data).expect("shaper creation should succeed")
    }

    // ============================================================
    // Happy path
    // ============================================================

    #[test]
    fn shape_single_char_returns_one_glyph() {
        let shaper = test_shaper();
        let glyphs = shaper.shape("A");
        assert_eq!(glyphs.len(), 1, "single char should produce 1 glyph");
    }

    #[test]
    fn shape_single_char_has_nonzero_glyph_id() {
        let shaper = test_shaper();
        let glyphs = shaper.shape("A");
        assert_ne!(glyphs[0].glyph_id, 0, "'A' should not be notdef (glyph_id 0)");
    }

    #[test]
    fn shape_single_char_has_cluster_zero() {
        let shaper = test_shaper();
        let glyphs = shaper.shape("A");
        assert_eq!(glyphs[0].cluster, 0, "first char should have cluster index 0");
    }

    #[test]
    fn shape_hello_returns_five_glyphs() {
        let shaper = test_shaper();
        let glyphs = shaper.shape("Hello");
        assert_eq!(glyphs.len(), 5, "'Hello' should produce 5 glyphs");
    }

    #[test]
    fn shape_hello_clusters_are_sequential() {
        let shaper = test_shaper();
        let glyphs = shaper.shape("Hello");
        let clusters: Vec<u32> = glyphs.iter().map(|g| g.cluster).collect();
        // Clusters should be monotonically increasing for LTR text
        for i in 1..clusters.len() {
            assert!(
                clusters[i] >= clusters[i - 1],
                "clusters should be non-decreasing: {clusters:?}"
            );
        }
    }

    #[test]
    fn shape_hello_all_glyph_ids_nonzero() {
        let shaper = test_shaper();
        let glyphs = shaper.shape("Hello");
        for (i, g) in glyphs.iter().enumerate() {
            assert_ne!(g.glyph_id, 0, "glyph {i} should have non-zero glyph_id");
        }
    }

    #[test]
    fn shape_empty_string_returns_empty_vec() {
        let shaper = test_shaper();
        let glyphs = shaper.shape("");
        assert!(glyphs.is_empty(), "empty string should produce no glyphs");
    }

    #[test]
    fn shape_monospace_advances_are_consistent() {
        let shaper = test_shaper();
        let glyphs = shaper.shape("Hello");
        // All x_advance values should be the same for a monospace font.
        let advances: Vec<i32> = glyphs.iter().map(|g| g.x_advance).collect();
        let first = advances[0];
        for (i, &advance) in advances.iter().enumerate() {
            assert_eq!(advance, first, "glyph {i} advance {advance} != first advance {first}");
        }
    }

    // ============================================================
    // Error cases
    // ============================================================

    #[test]
    fn shape_missing_glyph_returns_notdef() {
        let shaper = test_shaper();
        // Use a rare CJK character unlikely to be in Hack font
        let glyphs = shaper.shape("\u{4E00}");
        // Should return at least one glyph (the notdef glyph, glyph_id 0)
        assert!(!glyphs.is_empty(), "missing glyph should still produce output");
    }

    // ============================================================
    // Edge cases
    // ============================================================

    #[test]
    fn shape_space_returns_one_glyph() {
        let shaper = test_shaper();
        let glyphs = shaper.shape(" ");
        assert_eq!(glyphs.len(), 1, "space should produce 1 glyph");
    }

    #[test]
    fn shape_space_has_expected_advance() {
        let shaper = test_shaper();
        let space_glyphs = shaper.shape(" ");
        let letter_glyphs = shaper.shape("A");
        // For monospace, space advance should equal letter advance.
        assert_eq!(
            space_glyphs[0].x_advance, letter_glyphs[0].x_advance,
            "space and letter should have same advance in monospace"
        );
    }

    #[test]
    fn shape_control_chars_does_not_panic() {
        let shaper = test_shaper();
        // Newline and other control characters should not panic.
        let glyphs = shaper.shape("\n\t\r");
        // We just verify it doesn't panic; output may vary.
        let _ = glyphs;
    }

    #[test]
    fn shape_combining_accent_produces_output() {
        let shaper = test_shaper();
        // 'e' followed by combining acute accent (U+0301)
        let glyphs = shaper.shape("e\u{0301}");
        // Should produce at least one glyph (may merge into one or stay separate).
        assert!(!glyphs.is_empty(), "combining accent sequence should produce output");
    }
}
