//! GPU-uploadable font descriptor and text packing.
//! Zero external deps — takes raw data, not FontAtlas.

/// Maximum fonts in the bank.
pub const MAX_FONTS: usize = 4;

/// Font descriptor — stored in FontBank, GPU-uploadable.
/// 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuFont {
    pub glyph_w: u32,
    pub glyph_h: u32,
    pub first_cp: u32,
    pub last_cp: u32,
}

impl GpuFont {
    pub const NONE: Self = Self { glyph_w: 0, glyph_h: 0, first_cp: 0, last_cp: 0 };

    pub fn new(glyph_w: u8, glyph_h: u8, first_cp: u8, last_cp: u8) -> Self {
        Self {
            glyph_w: glyph_w as u32,
            glyph_h: glyph_h as u32,
            first_cp: first_cp as u32,
            last_cp: last_cp as u32,
        }
    }
}

/// Pack a raw bitmap (&[u8]) into u32s for GPU storage buffer.
/// Each byte becomes one u32 for easy shader indexing.
pub fn pack_bitmap(bitmap: &[u8]) -> Vec<u32> {
    bitmap.iter().map(|&b| b as u32).collect()
}

/// String offset for GPU text rendering: (start index into char_buffer, length).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StringOffset {
    pub start: u32,
    pub len: u32,
}

/// Pack a string table into GPU buffers.
/// Returns (char_buffer as u32 codepoints, string_offsets).
pub fn pack_strings(strings: &[String]) -> (Vec<u32>, Vec<StringOffset>) {
    let mut chars = Vec::new();
    let mut offsets = Vec::new();
    for s in strings {
        let start = chars.len() as u32;
        for ch in s.bytes() {
            chars.push(ch as u32);
        }
        offsets.push(StringOffset { start, len: chars.len() as u32 - start });
    }
    (chars, offsets)
}
