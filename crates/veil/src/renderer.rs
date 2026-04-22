//! GPU renderer for Veil's terminal UI.
//!
//! Owns all wgpu state: device, queue, surface, pipeline, buffers.
//! Created once at startup, resized on window resize, renders each frame.
//!
//! Tests for this module are `#[ignore]` because they require a GPU.
