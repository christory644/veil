//! Font pipeline -- bundles font loading, shaping, rasterization, and atlas
//! packing into a single struct for use by the frame builder.

use crate::font::atlas::{AtlasRegion, GlyphAtlas};
use crate::font::loader::{FontConfig, FontData};
use crate::font::rasterizer::Rasterizer;
use crate::font::shaper::Shaper;

/// Bundles all font resources needed for text rendering.
pub struct FontPipeline {
    font_data: FontData,
    shaper: Shaper,
    rasterizer: Rasterizer,
    atlas: GlyphAtlas,
}

impl FontPipeline {
    /// Create a new font pipeline from a font configuration.
    ///
    /// Loads the font, creates a shaper, rasterizer, and a 512x512 glyph atlas.
    pub fn new(config: &FontConfig) -> anyhow::Result<Self> {
        let font_data = FontData::load(config)?;
        let shaper = Shaper::new(&font_data)?;
        let rasterizer = Rasterizer::new();
        let atlas = GlyphAtlas::new(512, 512);
        Ok(Self { font_data, shaper, rasterizer, atlas })
    }

    /// Ensure a glyph for the given character is in the atlas.
    ///
    /// If the glyph is already cached, returns the existing region.
    /// Otherwise, shapes and rasterizes the character and inserts it.
    ///
    /// Stub: returns `None` -- real implementation will shape, rasterize,
    /// and insert the glyph into the atlas.
    #[allow(clippy::unused_self)]
    pub fn ensure_glyph(&mut self, _ch: char) -> Option<AtlasRegion> {
        None
    }

    /// Cell width in pixels.
    pub fn cell_width(&self) -> f32 {
        self.font_data.cell_width()
    }

    /// Cell height in pixels.
    pub fn cell_height(&self) -> f32 {
        self.font_data.cell_height()
    }

    /// Font ascent in pixels.
    pub fn ascent(&self) -> f32 {
        self.font_data.ascent()
    }

    /// Get a reference to the glyph atlas (for GPU upload).
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// Get a mutable reference to the glyph atlas (for marking clean).
    pub fn atlas_mut(&mut self) -> &mut GlyphAtlas {
        &mut self.atlas
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Path to the test font fixture (Hack Regular, MIT license).
    fn test_font_path() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/test_fixtures/test_font.ttf"))
    }

    /// Standard config for tests: 14pt at 96 DPI.
    fn test_config() -> FontConfig {
        FontConfig { path: Some(test_font_path()), size_pt: 14.0, dpi: 96.0 }
    }

    /// Create a `FontPipeline` from the test font fixture.
    fn test_pipeline() -> FontPipeline {
        FontPipeline::new(&test_config()).expect("font pipeline should initialize")
    }

    // ============================================================
    // FontPipeline::new — happy path
    // ============================================================

    #[test]
    fn new_pipeline_initializes_successfully() {
        let _pipeline = test_pipeline();
    }

    #[test]
    fn pipeline_has_positive_cell_width() {
        let pipeline = test_pipeline();
        assert!(pipeline.cell_width() > 0.0, "cell_width should be positive");
    }

    #[test]
    fn pipeline_has_positive_cell_height() {
        let pipeline = test_pipeline();
        assert!(pipeline.cell_height() > 0.0, "cell_height should be positive");
    }

    #[test]
    fn pipeline_has_positive_ascent() {
        let pipeline = test_pipeline();
        assert!(pipeline.ascent() > 0.0, "ascent should be positive");
    }

    #[test]
    fn pipeline_atlas_starts_empty_and_clean() {
        let pipeline = test_pipeline();
        assert!(!pipeline.atlas().is_dirty(), "fresh atlas should not be dirty");
    }

    // ============================================================
    // FontPipeline::new — error cases
    // ============================================================

    #[test]
    fn new_pipeline_with_bad_path_returns_err() {
        let config = FontConfig {
            path: Some(PathBuf::from("/nonexistent/font.ttf")),
            size_pt: 14.0,
            dpi: 96.0,
        };
        assert!(FontPipeline::new(&config).is_err());
    }

    // ============================================================
    // FontPipeline::ensure_glyph — happy path
    // ============================================================

    #[test]
    fn ensure_glyph_letter_a_returns_some() {
        let mut pipeline = test_pipeline();
        let region = pipeline.ensure_glyph('A');
        assert!(region.is_some(), "ensure_glyph('A') should return Some with a valid atlas region");
    }

    #[test]
    fn ensure_glyph_returns_nonzero_dimensions() {
        let mut pipeline = test_pipeline();
        let region = pipeline.ensure_glyph('A');
        assert!(region.is_some(), "'A' should be rasterizable");
        let region = region.unwrap();
        assert!(region.width > 0, "glyph 'A' should have non-zero width");
        assert!(region.height > 0, "glyph 'A' should have non-zero height");
    }

    #[test]
    fn ensure_glyph_caches_on_second_call() {
        let mut pipeline = test_pipeline();
        let region1 = pipeline.ensure_glyph('A');
        let region2 = pipeline.ensure_glyph('A');
        assert!(region1.is_some(), "first call should return Some");
        assert!(region2.is_some(), "second call should return Some");
        let r1 = region1.unwrap();
        let r2 = region2.unwrap();
        assert!(
            (r1.u_min - r2.u_min).abs() < f32::EPSILON
                && (r1.v_min - r2.v_min).abs() < f32::EPSILON
                && (r1.u_max - r2.u_max).abs() < f32::EPSILON
                && (r1.v_max - r2.v_max).abs() < f32::EPSILON,
            "second call should return the same UV coords (cached)"
        );
    }

    #[test]
    fn ensure_glyph_multiple_characters() {
        let mut pipeline = test_pipeline();
        for ch in ['A', 'B', 'C', 'x', 'y', 'z', '0', '1', '@'] {
            let region = pipeline.ensure_glyph(ch);
            assert!(region.is_some(), "ensure_glyph('{ch}') should return Some");
        }
    }

    // ============================================================
    // FontPipeline::ensure_glyph — edge cases
    // ============================================================

    #[test]
    fn ensure_glyph_space_returns_zero_dimension_region() {
        let mut pipeline = test_pipeline();
        let region = pipeline.ensure_glyph(' ');
        // Space glyph has no visible pixels, so region should have zero dimensions.
        if let Some(r) = region {
            assert_eq!(r.width, 0, "space glyph should have zero width");
            assert_eq!(r.height, 0, "space glyph should have zero height");
        }
        // It's also acceptable to return None for a space; but per the spec, it should
        // insert a zero-dimension entry.
        assert!(region.is_some(), "space should return Some with zero-dimension region");
    }

    #[test]
    fn ensure_glyph_control_char_returns_zero_dimension_region() {
        let mut pipeline = test_pipeline();
        // Null character is a control character with no visual representation.
        let region = pipeline.ensure_glyph('\0');
        if let Some(r) = region {
            assert_eq!(r.width, 0, "control char should have zero width");
            assert_eq!(r.height, 0, "control char should have zero height");
        }
        assert!(region.is_some(), "control char should return Some with zero-dimension region");
    }

    #[test]
    fn ensure_glyph_missing_char_returns_notdef() {
        let mut pipeline = test_pipeline();
        // Use a rare CJK character unlikely to be in Hack font.
        let region = pipeline.ensure_glyph('\u{4E00}');
        // Should return Some (notdef glyph) rather than None.
        assert!(region.is_some(), "missing character should return notdef glyph, not None");
    }
}
