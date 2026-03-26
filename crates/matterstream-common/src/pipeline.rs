//! Render pipeline types and traits.
//!
//! `RenderFrame` is the output of the compute stage and input to the render stage.
//! `RenderBackend` is implemented by GPU (wgpu) and CPU (softbuffer) renderers.

use crate::sdf::{SdfDrawCmd, Anim, GpuTexture};
use crate::font::GpuFont;

/// Fully prepared frame — output of compute stage, input to render stage.
/// All strings packed, all offsets encoded, all data GPU-uploadable.
pub struct RenderFrame {
    pub draws: Vec<SdfDrawCmd>,       // string offsets already in params[3]
    pub char_buffer: Vec<u32>,        // packed codepoints
    pub anim_bank: Vec<Anim>,
    pub texture_bank: Vec<GpuTexture>, // texture descriptors
    pub font: GpuFont,
    pub glyph_bitmap: Vec<u32>,       // packed bitmap
    pub scalar_bank: [f32; 16],
    pub int_bank: [i32; 16],
    pub time_ms: f32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}
