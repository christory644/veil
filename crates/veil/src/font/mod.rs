//! Font rendering pipeline: loading, shaping, rasterization, atlas packing,
//! and text vertex generation.
//!
//! This module implements the core glyph pipeline that turns terminal cell
//! content into visible glyphs on the GPU.

mod atlas;
mod loader;
mod rasterizer;
mod shaper;
mod text_vertex;

pub use atlas::{AtlasRegion, GlyphAtlas};
pub use loader::{FontConfig, FontData};
pub use rasterizer::{RasterizedGlyph, Rasterizer};
pub use shaper::{ShapedGlyph, Shaper};
pub use text_vertex::{text_quad_indices, text_quad_vertices, TextVertex};
