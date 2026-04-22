//! Font loading and metrics extraction using swash.
//!
//! Reads a font file, parses tables, and extracts monospace grid metrics
//! (cell width, cell height, ascent, descent) at a given size and DPI.

use std::path::PathBuf;

/// User-facing font configuration.
pub struct FontConfig {
    /// Path to the font file. None means use the bundled default.
    pub path: Option<PathBuf>,
    /// Font size in points.
    pub size_pt: f32,
    /// Display DPI (pixels per inch). Defaults to 96.0 for 1x displays.
    pub dpi: f32,
}

/// Owned font data with extracted metrics.
///
/// Holds the raw font bytes so that `FontRef` can be derived from them.
pub struct FontData {
    /// Raw font file bytes (kept alive for `FontRef` borrowing).
    data: Vec<u8>,
    /// Index of the font within the file (for .ttc collections).
    index: u32,
    /// Size in pixels (computed from `size_pt` and `dpi`).
    size_px: f32,
    /// Cell width in pixels (advance width of a reference character).
    cell_width: f32,
    /// Cell height in pixels (ascent + descent + line gap).
    cell_height: f32,
    /// Ascent in pixels (distance from baseline to top of cell).
    ascent: f32,
    /// Descent in pixels (distance from baseline to bottom, positive downward).
    descent: f32,
}

impl FontData {
    /// Load a font from a file path at the given configuration.
    ///
    /// Reads the file, parses font tables, extracts metrics at the
    /// configured size and DPI.
    pub fn load(config: &FontConfig) -> anyhow::Result<Self> {
        let path = config
            .path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no font path provided and no bundled default"))?;

        if config.size_pt <= 0.0 {
            anyhow::bail!("font size_pt must be positive, got {}", config.size_pt);
        }
        if config.dpi <= 0.0 {
            anyhow::bail!("font dpi must be positive, got {}", config.dpi);
        }

        let data = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("failed to read font file {}: {e}", path.display()))?;

        if data.is_empty() {
            anyhow::bail!("font file is empty: {}", path.display());
        }

        let font_ref = swash::FontRef::from_index(&data, 0)
            .ok_or_else(|| anyhow::anyhow!("failed to parse font file: {}", path.display()))?;

        let size_px = config.size_pt * config.dpi / 72.0;

        let metrics = font_ref.metrics(&[]).scale(size_px);
        let ascent = metrics.ascent;
        let descent = metrics.descent;
        let leading = metrics.leading;
        let cell_height = ascent + descent + leading;

        // Cell width: use advance of 'M', fall back to space, then estimate.
        let charmap = font_ref.charmap();
        let glyph_metrics = font_ref.glyph_metrics(&[]).scale(size_px);

        let cell_width = ['M', ' ']
            .iter()
            .map(|&ch| charmap.map(ch))
            .find(|&id| id != 0)
            .map_or(size_px * 0.6, |id| glyph_metrics.advance_width(id));

        Ok(Self { data, index: 0, size_px, cell_width, cell_height, ascent, descent })
    }

    /// Create a swash `FontRef` borrowing from the internal data.
    /// Used by the shaper and rasterizer.
    pub fn font_ref(&self) -> swash::FontRef<'_> {
        swash::FontRef::from_index(&self.data, self.index as usize)
            .expect("FontData was constructed from valid font data")
    }

    /// Cell width in pixels.
    pub fn cell_width(&self) -> f32 {
        self.cell_width
    }

    /// Cell height in pixels.
    pub fn cell_height(&self) -> f32 {
        self.cell_height
    }

    /// Ascent in pixels (baseline to top of cell).
    pub fn ascent(&self) -> f32 {
        self.ascent
    }

    /// Descent in pixels (positive downward).
    pub fn descent(&self) -> f32 {
        self.descent
    }

    /// Font size in pixels.
    pub fn size_px(&self) -> f32 {
        self.size_px
    }

    /// Raw font data bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Font index within the file.
    pub fn index(&self) -> u32 {
        self.index
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

    // ============================================================
    // Happy path
    // ============================================================

    #[test]
    fn load_valid_font_succeeds() {
        let config = test_config();
        let font_data = FontData::load(&config).expect("should load valid font");
        // Verify the data was loaded (non-empty).
        assert!(!font_data.data.is_empty(), "font data should not be empty");
    }

    #[test]
    fn cell_width_is_positive() {
        let font_data = FontData::load(&test_config()).unwrap();
        assert!(font_data.cell_width() > 0.0, "cell_width should be positive");
    }

    #[test]
    fn cell_height_is_positive() {
        let font_data = FontData::load(&test_config()).unwrap();
        assert!(font_data.cell_height() > 0.0, "cell_height should be positive");
    }

    #[test]
    fn cell_height_at_least_ascent_plus_descent() {
        let font_data = FontData::load(&test_config()).unwrap();
        let ascent = font_data.ascent();
        let descent = font_data.descent();
        let cell_height = font_data.cell_height();
        assert!(
            cell_height >= ascent + descent - 0.01,
            "cell_height ({cell_height}) should be >= ascent ({ascent}) + descent ({descent})"
        );
    }

    #[test]
    fn ascent_is_positive() {
        let font_data = FontData::load(&test_config()).unwrap();
        assert!(font_data.ascent() > 0.0, "ascent should be positive");
    }

    #[test]
    fn size_px_computed_correctly() {
        let config = FontConfig { path: Some(test_font_path()), size_pt: 14.0, dpi: 96.0 };
        let font_data = FontData::load(&config).unwrap();
        let expected_px = 14.0 * 96.0 / 72.0;
        assert!(
            (font_data.size_px() - expected_px).abs() < 0.01,
            "size_px should be size_pt * dpi / 72.0 = {expected_px}, got {}",
            font_data.size_px()
        );
    }

    #[test]
    fn font_ref_resolves_glyph_ids() {
        let font_data = FontData::load(&test_config()).unwrap();
        let font_ref = font_data.font_ref();
        // A valid FontRef should be able to map 'A' to a non-zero glyph ID.
        let charmap = font_ref.charmap();
        let glyph_id = charmap.map('A');
        assert_ne!(glyph_id, 0, "'A' should map to a non-zero glyph ID");
    }

    // ============================================================
    // Error cases
    // ============================================================

    #[test]
    fn load_nonexistent_file_returns_err() {
        let config = FontConfig {
            path: Some(PathBuf::from("/nonexistent/path/to/font.ttf")),
            size_pt: 14.0,
            dpi: 96.0,
        };
        assert!(FontData::load(&config).is_err(), "nonexistent file should return Err");
    }

    #[test]
    fn load_empty_file_returns_err() {
        let dir = tempfile::tempdir().unwrap();
        let empty_path = dir.path().join("empty.ttf");
        std::fs::write(&empty_path, b"").unwrap();
        let config = FontConfig { path: Some(empty_path), size_pt: 14.0, dpi: 96.0 };
        assert!(FontData::load(&config).is_err(), "empty file should return Err");
    }

    #[test]
    fn load_non_font_file_returns_err() {
        let dir = tempfile::tempdir().unwrap();
        let text_path = dir.path().join("not_a_font.ttf");
        std::fs::write(&text_path, b"This is not a font file").unwrap();
        let config = FontConfig { path: Some(text_path), size_pt: 14.0, dpi: 96.0 };
        assert!(FontData::load(&config).is_err(), "non-font file should return Err");
    }

    // ============================================================
    // Edge cases
    // ============================================================

    #[test]
    fn load_very_small_font_size() {
        let config = FontConfig { path: Some(test_font_path()), size_pt: 1.0, dpi: 96.0 };
        let font_data = FontData::load(&config).expect("should load at very small size");
        assert!(font_data.cell_width() > 0.0, "cell_width should be positive even at 1pt");
        assert!(font_data.cell_height() > 0.0, "cell_height should be positive even at 1pt");
    }

    #[test]
    fn load_very_large_font_size() {
        let config = FontConfig { path: Some(test_font_path()), size_pt: 200.0, dpi: 96.0 };
        let font_data = FontData::load(&config).expect("should load at very large size");
        assert!(font_data.cell_width() > 0.0, "cell_width should be positive at 200pt");
        assert!(font_data.cell_height() > 0.0, "cell_height should be positive at 200pt");
        // Should not overflow: metrics should be reasonable (not inf/nan)
        assert!(font_data.cell_width().is_finite(), "cell_width should be finite");
        assert!(font_data.cell_height().is_finite(), "cell_height should be finite");
    }
}
