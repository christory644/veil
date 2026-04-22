//! Font rendering pipeline: loading, shaping, rasterization, atlas packing,
//! and text vertex generation.
//!
//! This module implements the core glyph pipeline that turns terminal cell
//! content into visible glyphs on the GPU.

pub(crate) mod atlas;
pub(crate) mod loader;
pub(crate) mod rasterizer;
pub(crate) mod shaper;
pub(crate) mod text_vertex;
