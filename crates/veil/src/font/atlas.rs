//! CPU-side glyph atlas with shelf packing.
//!
//! Accumulates rasterized glyph bitmaps into a single grayscale texture.
//! Uses a shelf-packing algorithm: glyphs are placed left-to-right in
//! rows ("shelves"), each shelf as tall as its tallest glyph. When a
//! shelf runs out of horizontal space, a new shelf starts below.

use std::collections::HashMap;

use crate::font::rasterizer::RasterizedGlyph;

/// Padding in pixels between glyphs to prevent texture filtering artifacts.
const PADDING: u32 = 1;

/// Maximum atlas dimension (width or height) in pixels.
/// Prevents runaway growth if an unexpectedly large glyph is inserted.
const MAX_ATLAS_DIMENSION: u32 = 16384;

/// UV coordinates for a glyph within the atlas texture.
/// Normalized to [0.0, 1.0] range.
#[derive(Debug, Clone, Copy)]
pub struct AtlasRegion {
    /// Left edge U coordinate.
    pub u_min: f32,
    /// Top edge V coordinate.
    pub v_min: f32,
    /// Right edge U coordinate.
    pub u_max: f32,
    /// Bottom edge V coordinate.
    pub v_max: f32,
    /// Pixel width of the glyph in the atlas.
    pub width: u32,
    /// Pixel height of the glyph in the atlas.
    pub height: u32,
    /// Bearing X from the rasterized glyph.
    pub bearing_x: i32,
    /// Bearing Y from the rasterized glyph.
    pub bearing_y: i32,
}

/// A shelf (row) within the atlas for the packing algorithm.
struct Shelf {
    /// Y position of this shelf's top edge.
    y: u32,
    /// Current X cursor (next glyph goes here).
    x: u32,
    /// Height of this shelf (tallest glyph placed so far).
    height: u32,
}

/// CPU-side glyph atlas with shelf packing.
pub struct GlyphAtlas {
    /// Grayscale bitmap data (1 byte per pixel).
    bitmap: Vec<u8>,
    /// Atlas width in pixels.
    width: u32,
    /// Atlas height in pixels.
    height: u32,
    /// Packed shelves.
    shelves: Vec<Shelf>,
    /// Map from `glyph_id` to atlas region.
    entries: HashMap<u16, AtlasRegion>,
    /// Whether the atlas data has changed since last GPU upload.
    dirty: bool,
}

impl GlyphAtlas {
    /// Create a new empty atlas with the given initial dimensions.
    /// Dimensions should be powers of 2 (e.g., 512x512 or 1024x1024).
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            bitmap: vec![0; (width * height) as usize],
            width,
            height,
            shelves: Vec::new(),
            entries: HashMap::new(),
            dirty: false,
        }
    }

    /// Look up a glyph in the atlas. Returns `None` if not yet packed.
    pub fn get(&self, glyph_id: u16) -> Option<&AtlasRegion> {
        self.entries.get(&glyph_id)
    }

    /// Insert a rasterized glyph into the atlas.
    ///
    /// Returns the `AtlasRegion` where the glyph was placed.
    /// If the glyph is already in the atlas, returns the existing region.
    /// If there is no room, grows the atlas and retries.
    ///
    /// # Panics
    ///
    /// Panics if the glyph dimensions exceed `MAX_ATLAS_DIMENSION` (16384).
    #[allow(clippy::cast_precision_loss)] // Atlas pixel coords fit comfortably in f32.
    pub fn insert(&mut self, glyph: &RasterizedGlyph) -> AtlasRegion {
        // Return existing entry if already packed.
        if let Some(region) = self.entries.get(&glyph.glyph_id) {
            return *region;
        }

        // Handle zero-dimension glyphs: store a zero-area entry.
        if glyph.width == 0 || glyph.height == 0 {
            let region = AtlasRegion {
                u_min: 0.0,
                v_min: 0.0,
                u_max: 0.0,
                v_max: 0.0,
                width: 0,
                height: 0,
                bearing_x: glyph.bearing_x,
                bearing_y: glyph.bearing_y,
            };
            self.entries.insert(glyph.glyph_id, region);
            self.dirty = true;
            return region;
        }

        assert!(
            glyph.width <= MAX_ATLAS_DIMENSION && glyph.height <= MAX_ATLAS_DIMENSION,
            "glyph dimensions {}x{} exceed maximum atlas dimension {MAX_ATLAS_DIMENSION}",
            glyph.width,
            glyph.height,
        );

        // Ensure atlas is wide enough for the glyph (grow() only doubles height).
        while glyph.width > self.width {
            self.grow_width();
        }

        // Find placement using shelf packing.
        let (px, py) = loop {
            if let Some(pos) = self.try_place(glyph.width, glyph.height) {
                break pos;
            }
            // No room -- grow the atlas and retry.
            self.grow();
        };

        // Copy glyph bitmap into the atlas row by row.
        for row in 0..glyph.height {
            let src_start = (row * glyph.width) as usize;
            let src_end = src_start + glyph.width as usize;
            let dst_start = ((py + row) * self.width + px) as usize;
            let dst_end = dst_start + glyph.width as usize;
            self.bitmap[dst_start..dst_end].copy_from_slice(&glyph.bitmap[src_start..src_end]);
        }

        // Compute UV coordinates normalized to [0, 1].
        let w = self.width as f32;
        let h = self.height as f32;
        let region = AtlasRegion {
            u_min: px as f32 / w,
            v_min: py as f32 / h,
            u_max: (px + glyph.width) as f32 / w,
            v_max: (py + glyph.height) as f32 / h,
            width: glyph.width,
            height: glyph.height,
            bearing_x: glyph.bearing_x,
            bearing_y: glyph.bearing_y,
        };
        self.entries.insert(glyph.glyph_id, region);
        self.dirty = true;
        region
    }

    /// Try to place a glyph of the given dimensions on the current shelf
    /// or a new shelf. Returns `Some((x, y))` pixel position on success,
    /// or `None` if the atlas needs to grow.
    fn try_place(&mut self, glyph_w: u32, glyph_h: u32) -> Option<(u32, u32)> {
        // Try the current (last) shelf.
        if let Some(shelf) = self.shelves.last_mut() {
            let fits_with_padding = shelf.x + glyph_w + PADDING <= self.width;
            let exact_fit = shelf.x + glyph_w == self.width;
            if fits_with_padding || exact_fit {
                let px = shelf.x;
                let py = shelf.y;
                shelf.x += glyph_w + PADDING;
                shelf.height = shelf.height.max(glyph_h);
                return Some((px, py));
            }
            // Doesn't fit horizontally -- need a new shelf.
        }

        // Start a new shelf below the last one.
        let new_y =
            if let Some(last) = self.shelves.last() { last.y + last.height + PADDING } else { 0 };

        // Check if the new shelf fits vertically.
        if new_y + glyph_h > self.height {
            return None; // Need to grow.
        }

        self.shelves.push(Shelf { y: new_y, x: glyph_w + PADDING, height: glyph_h });

        Some((0, new_y))
    }

    /// Grow the atlas by doubling its width. Copies existing rows into
    /// the wider bitmap and rescales U coordinates.
    #[allow(clippy::cast_precision_loss)] // Atlas dimensions fit comfortably in f32.
    fn grow_width(&mut self) {
        let old_width = self.width;
        let new_width = old_width * 2;

        let mut new_bitmap = vec![0u8; (new_width * self.height) as usize];
        // Copy each row from the old bitmap into the wider one.
        for row in 0..self.height {
            let src_start = (row * old_width) as usize;
            let src_end = src_start + old_width as usize;
            let dst_start = (row * new_width) as usize;
            let dst_end = dst_start + old_width as usize;
            new_bitmap[dst_start..dst_end].copy_from_slice(&self.bitmap[src_start..src_end]);
        }
        self.bitmap = new_bitmap;
        self.width = new_width;

        // Rescale U coordinates: u_new = u_old * old_width / new_width
        let scale = old_width as f32 / new_width as f32;
        for region in self.entries.values_mut() {
            region.u_min *= scale;
            region.u_max *= scale;
        }
    }

    /// Grow the atlas by doubling its height. Copies existing bitmap data
    /// and updates all existing entries' UV coordinates to account for the
    /// new height.
    #[allow(clippy::cast_precision_loss)] // Atlas dimensions fit comfortably in f32.
    fn grow(&mut self) {
        let old_height = self.height;
        let new_height = old_height * 2;

        // Allocate new bitmap and copy old data.
        let mut new_bitmap = vec![0u8; (self.width * new_height) as usize];
        new_bitmap[..self.bitmap.len()].copy_from_slice(&self.bitmap);
        self.bitmap = new_bitmap;
        self.height = new_height;

        // Update all existing entries' V coordinates since the atlas height changed.
        // v_new = v_old * old_height / new_height
        let scale = old_height as f32 / new_height as f32;
        for region in self.entries.values_mut() {
            region.v_min *= scale;
            region.v_max *= scale;
        }
    }

    /// Returns `true` if the atlas has been modified since the last
    /// call to `mark_clean`.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the atlas as clean (call after uploading to GPU).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Get the raw bitmap data for GPU upload.
    pub fn bitmap(&self) -> &[u8] {
        &self.bitmap
    }

    /// Get the atlas dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
mod tests {
    use super::*;
    use crate::font::rasterizer::RasterizedGlyph;

    /// Create a test rasterized glyph with known data.
    fn test_glyph(glyph_id: u16, width: u32, height: u32) -> RasterizedGlyph {
        RasterizedGlyph {
            glyph_id,
            bitmap: vec![128; (width * height) as usize],
            width,
            height,
            bearing_x: 1,
            bearing_y: 10,
        }
    }

    // ============================================================
    // Happy path
    // ============================================================

    #[test]
    fn new_atlas_has_correct_dimensions() {
        let atlas = GlyphAtlas::new(512, 512);
        assert_eq!(atlas.dimensions(), (512, 512));
    }

    #[test]
    fn new_atlas_bitmap_length_matches_dimensions() {
        let atlas = GlyphAtlas::new(512, 512);
        assert_eq!(atlas.bitmap().len(), 512 * 512);
    }

    #[test]
    fn new_atlas_is_not_dirty() {
        let atlas = GlyphAtlas::new(512, 512);
        assert!(!atlas.is_dirty(), "new atlas should not be dirty");
    }

    #[test]
    fn insert_glyph_then_get_returns_some() {
        let mut atlas = GlyphAtlas::new(512, 512);
        let glyph = test_glyph(42, 8, 14);
        atlas.insert(&glyph);
        let region = atlas.get(42);
        assert!(region.is_some(), "inserted glyph should be retrievable");
    }

    #[test]
    fn inserted_glyph_has_correct_dimensions() {
        let mut atlas = GlyphAtlas::new(512, 512);
        let glyph = test_glyph(42, 8, 14);
        let region = atlas.insert(&glyph);
        assert_eq!(region.width, 8, "atlas region width should match glyph width");
        assert_eq!(region.height, 14, "atlas region height should match glyph height");
    }

    #[test]
    fn insert_same_glyph_twice_returns_same_region() {
        let mut atlas = GlyphAtlas::new(512, 512);
        let glyph = test_glyph(42, 8, 14);
        let region1 = atlas.insert(&glyph);
        let region2 = atlas.insert(&glyph);
        assert_eq!(region1.u_min, region2.u_min, "duplicate insert should return same u_min");
        assert_eq!(region1.v_min, region2.v_min, "duplicate insert should return same v_min");
        assert_eq!(region1.u_max, region2.u_max, "duplicate insert should return same u_max");
        assert_eq!(region1.v_max, region2.v_max, "duplicate insert should return same v_max");
    }

    #[test]
    fn insert_multiple_glyphs_all_retrievable() {
        let mut atlas = GlyphAtlas::new(512, 512);
        for id in 1..=10_u16 {
            let glyph = test_glyph(id, 8, 14);
            atlas.insert(&glyph);
        }
        for id in 1..=10_u16 {
            assert!(atlas.get(id).is_some(), "glyph {id} should be retrievable after insert");
        }
    }

    #[test]
    fn insert_multiple_glyphs_uv_regions_dont_overlap() {
        let mut atlas = GlyphAtlas::new(512, 512);
        let mut regions = Vec::new();
        for id in 1..=5_u16 {
            let glyph = test_glyph(id, 8, 14);
            regions.push(atlas.insert(&glyph));
        }
        // Check that no two regions overlap.
        for i in 0..regions.len() {
            for j in (i + 1)..regions.len() {
                let a = &regions[i];
                let b = &regions[j];
                // Two rectangles don't overlap if one is completely to the left/right/above/below.
                let no_overlap = a.u_max <= b.u_min
                    || b.u_max <= a.u_min
                    || a.v_max <= b.v_min
                    || b.v_max <= a.v_min;
                assert!(
                    no_overlap,
                    "regions for glyphs {} and {} should not overlap: a=({}, {}, {}, {}), b=({}, {}, {}, {})",
                    i + 1, j + 1,
                    a.u_min, a.v_min, a.u_max, a.v_max,
                    b.u_min, b.v_min, b.u_max, b.v_max,
                );
            }
        }
    }

    #[test]
    fn uv_coordinates_in_zero_one_range() {
        let mut atlas = GlyphAtlas::new(512, 512);
        let glyph = test_glyph(42, 8, 14);
        let region = atlas.insert(&glyph);
        assert!(region.u_min >= 0.0, "u_min should be >= 0.0");
        assert!(region.v_min >= 0.0, "v_min should be >= 0.0");
        assert!(region.u_max <= 1.0, "u_max should be <= 1.0");
        assert!(region.v_max <= 1.0, "v_max should be <= 1.0");
        assert!(region.u_min < region.u_max, "u_min should be < u_max");
        assert!(region.v_min < region.v_max, "v_min should be < v_max");
    }

    #[test]
    fn insert_sets_dirty_flag() {
        let mut atlas = GlyphAtlas::new(512, 512);
        let glyph = test_glyph(42, 8, 14);
        atlas.insert(&glyph);
        assert!(atlas.is_dirty(), "atlas should be dirty after insert");
    }

    #[test]
    fn mark_clean_clears_dirty_flag() {
        let mut atlas = GlyphAtlas::new(512, 512);
        let glyph = test_glyph(42, 8, 14);
        atlas.insert(&glyph);
        atlas.mark_clean();
        assert!(!atlas.is_dirty(), "atlas should not be dirty after mark_clean");
    }

    #[test]
    fn glyph_bitmap_copied_into_atlas() {
        let mut atlas = GlyphAtlas::new(512, 512);
        let glyph = test_glyph(42, 8, 14);
        let region = atlas.insert(&glyph);

        // The atlas bitmap should contain the glyph's data at the region's position.
        // Convert UV coords back to pixel positions.
        let atlas_w = atlas.width as f32;
        let atlas_h = atlas.height as f32;
        let px_x = (region.u_min * atlas_w) as u32;
        let px_y = (region.v_min * atlas_h) as u32;

        // Check that at least some pixels in the region are non-zero (128).
        let mut found_nonzero = false;
        for row in 0..region.height {
            for col in 0..region.width {
                let atlas_idx = ((px_y + row) * atlas.width + (px_x + col)) as usize;
                if atlas.bitmap()[atlas_idx] == 128 {
                    found_nonzero = true;
                }
            }
        }
        assert!(found_nonzero, "glyph bitmap data should be copied into atlas");
    }

    #[test]
    fn inserted_region_preserves_bearing() {
        let mut atlas = GlyphAtlas::new(512, 512);
        let glyph = RasterizedGlyph {
            glyph_id: 42,
            bitmap: vec![128; 8 * 14],
            width: 8,
            height: 14,
            bearing_x: 2,
            bearing_y: 12,
        };
        let region = atlas.insert(&glyph);
        assert_eq!(region.bearing_x, 2, "atlas region should preserve bearing_x");
        assert_eq!(region.bearing_y, 12, "atlas region should preserve bearing_y");
    }

    // ============================================================
    // Error cases
    // ============================================================

    #[test]
    fn get_nonexistent_glyph_returns_none() {
        let atlas = GlyphAtlas::new(512, 512);
        assert!(atlas.get(99).is_none(), "non-inserted glyph should return None");
    }

    #[test]
    fn insert_zero_dimension_glyph_does_not_panic() {
        let mut atlas = GlyphAtlas::new(512, 512);
        let glyph = RasterizedGlyph {
            glyph_id: 1,
            bitmap: Vec::new(),
            width: 0,
            height: 0,
            bearing_x: 0,
            bearing_y: 0,
        };
        let _region = atlas.insert(&glyph);
        // Should not panic.
    }

    // ============================================================
    // Edge cases -- atlas growth
    // ============================================================

    #[test]
    fn atlas_grows_when_full() {
        // Use a tiny atlas that will fill up quickly.
        let mut atlas = GlyphAtlas::new(32, 32);
        let initial_height = atlas.height;

        // Insert many large-ish glyphs to force growth.
        for id in 1..=20_u16 {
            let glyph = test_glyph(id, 10, 14);
            atlas.insert(&glyph);
        }

        // After inserting many glyphs, the atlas should have grown.
        assert!(
            atlas.dimensions().1 > initial_height,
            "atlas height should have grown from {initial_height} to accommodate 20 glyphs"
        );
    }

    #[test]
    fn atlas_growth_preserves_existing_entries() {
        let mut atlas = GlyphAtlas::new(32, 32);

        // Insert a glyph before growth.
        let glyph_before = test_glyph(1, 10, 14);
        atlas.insert(&glyph_before);

        // Force growth with more glyphs.
        for id in 2..=20_u16 {
            let glyph = test_glyph(id, 10, 14);
            atlas.insert(&glyph);
        }

        // The first glyph should still be retrievable.
        assert!(atlas.get(1).is_some(), "glyph inserted before growth should still be retrievable");
    }

    #[test]
    fn atlas_growth_updates_uv_coordinates() {
        let mut atlas = GlyphAtlas::new(32, 32);

        let glyph_before = test_glyph(1, 10, 14);
        let region_before = atlas.insert(&glyph_before);
        let v_max_before = region_before.v_max;

        // Force growth.
        for id in 2..=20_u16 {
            let glyph = test_glyph(id, 10, 14);
            atlas.insert(&glyph);
        }

        // After growth (height doubled), the UV coordinates for glyph 1 should
        // change because the atlas height changed (UV is normalized by height).
        let region_after = atlas.get(1).expect("glyph 1 should exist");
        if atlas.dimensions().1 > 32 {
            // If atlas actually grew, v_max should be different (smaller, since
            // the atlas is taller but glyph is at the same pixel position).
            assert!(
                (region_after.v_max - v_max_before).abs() > f32::EPSILON
                    || atlas.dimensions().1 == 32,
                "UV coordinates should update after atlas growth"
            );
        }
    }

    #[test]
    fn single_large_glyph_triggers_growth() {
        // Atlas is 32x32, glyph is 30x30 -- fits one but barely.
        // A second glyph of that size should trigger growth.
        let mut atlas = GlyphAtlas::new(32, 32);
        let glyph1 = test_glyph(1, 30, 30);
        atlas.insert(&glyph1);

        let glyph2 = test_glyph(2, 30, 30);
        atlas.insert(&glyph2);

        // The atlas should have grown to accommodate both.
        assert!(atlas.get(1).is_some(), "glyph 1 should be retrievable");
        assert!(atlas.get(2).is_some(), "glyph 2 should be retrievable");
        assert!(atlas.dimensions().1 > 32, "atlas should have grown to fit two 30x30 glyphs");
    }

    #[test]
    fn shelf_full_starts_new_shelf() {
        let mut atlas = GlyphAtlas::new(64, 64);
        // Insert glyphs that will fill the first shelf (64px wide).
        // Each glyph is 10px wide, so 6 fit on a shelf (6 * (10+1) = 66 > 64, so 5 fit).
        // With 1px padding: each takes 11px, so floor(64/11) = 5 fit per shelf.
        for id in 1..=5_u16 {
            let glyph = test_glyph(id, 10, 14);
            atlas.insert(&glyph);
        }
        // The 6th glyph should start a new shelf.
        let glyph6 = test_glyph(6, 10, 14);
        let region6 = atlas.insert(&glyph6);

        // The 6th glyph should have a different v_min than the first 5
        // (it's on a new shelf, so higher y position).
        let region1 = atlas.get(1).expect("glyph 1 should exist");
        assert!(
            region6.v_min > region1.v_min,
            "6th glyph should be on a new shelf (higher v_min): glyph1.v_min={}, glyph6.v_min={}",
            region1.v_min,
            region6.v_min,
        );
    }
}
