//! GPU-uploadable glyph entry table.
//!
//! Each `GlyphEntry` maps a glyph ID to its atlas rectangle and font metrics.
//! The table is referenced by font index in the `BankedStyle`.

/// Per-glyph atlas entry — geometry + standard advance, 24 bytes packed.
///
/// The `advance_x` is the **standard** (hmtx) advance normalized to the em
/// square. The ISA's `DRAW_GLYPH` 12-bit field encodes only the **delta**
/// from this standard advance (in 1/16 px), capturing kerning/ligature
/// adjustments from shaping. The shader reconstructs:
///   `total_advance = advance_x * text_h + delta / 16.0`
///
/// ```text
/// [2b glyph_id][2b atlas_x][2b atlas_y][2b atlas_w][2b atlas_h][2b _pad]
/// [4b advance_x (f32, normalized)][4b bearing_x (f32)][4b bearing_y (f32)]
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphEntry {
    /// Font-internal glyph ID (matches DRAW_GLYPH instruction)
    pub glyph_id: u16,
    /// X position in atlas texture (pixels)
    pub atlas_x: u16,
    /// Y position in atlas texture (pixels)
    pub atlas_y: u16,
    /// Width in atlas texture (pixels)
    pub atlas_w: u16,
    /// Height in atlas texture (pixels)
    pub atlas_h: u16,
    /// Standard horizontal advance, normalized to em square (0.0–1.0)
    pub advance_x: f32,
    /// Horizontal bearing (left side bearing), normalized to em square
    pub bearing_x: f32,
    /// Vertical bearing (top bearing from baseline), normalized to em square
    pub bearing_y: f32,
}

impl GlyphEntry {
    /// Packed size in bytes for serialization.
    pub const PACKED_SIZE: usize = 24;

    /// Serialize to 24 bytes.
    pub fn to_bytes(&self) -> [u8; 24] {
        let mut buf = [0u8; 24];
        buf[0..2].copy_from_slice(&self.glyph_id.to_le_bytes());
        buf[2..4].copy_from_slice(&self.atlas_x.to_le_bytes());
        buf[4..6].copy_from_slice(&self.atlas_y.to_le_bytes());
        buf[6..8].copy_from_slice(&self.atlas_w.to_le_bytes());
        buf[8..10].copy_from_slice(&self.atlas_h.to_le_bytes());
        // 10..12 = padding
        buf[12..16].copy_from_slice(&self.advance_x.to_le_bytes());
        buf[16..20].copy_from_slice(&self.bearing_x.to_le_bytes());
        buf[20..24].copy_from_slice(&self.bearing_y.to_le_bytes());
        buf
    }

    /// Deserialize from bytes (must be at least 24 bytes).
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 24 {
            return Err("glyph entry too short");
        }
        Ok(Self {
            glyph_id: u16::from_le_bytes([data[0], data[1]]),
            atlas_x: u16::from_le_bytes([data[2], data[3]]),
            atlas_y: u16::from_le_bytes([data[4], data[5]]),
            atlas_w: u16::from_le_bytes([data[6], data[7]]),
            atlas_h: u16::from_le_bytes([data[8], data[9]]),
            advance_x: f32::from_le_bytes([data[12], data[13], data[14], data[15]]),
            bearing_x: f32::from_le_bytes([data[16], data[17], data[18], data[19]]),
            bearing_y: f32::from_le_bytes([data[20], data[21], data[22], data[23]]),
        })
    }

    /// Pack for GPU upload: 2 × vec4<u32> = 32 bytes.
    ///
    /// ```text
    /// vec4<u32>: [glyph_id, atlas_x | atlas_y<<16, atlas_w | atlas_h<<16, 0]
    /// vec4<f32>: [advance_x, bearing_x, bearing_y, 0]
    /// ```
    pub fn to_gpu_u32s(&self) -> [u32; 8] {
        [
            self.glyph_id as u32,
            (self.atlas_x as u32) | ((self.atlas_y as u32) << 16),
            (self.atlas_w as u32) | ((self.atlas_h as u32) << 16),
            0,
            self.advance_x.to_bits(),
            self.bearing_x.to_bits(),
            self.bearing_y.to_bits(),
            0,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_entry_roundtrip() {
        let entry = GlyphEntry {
            glyph_id: 42,
            atlas_x: 100,
            atlas_y: 200,
            atlas_w: 32,
            atlas_h: 32,
            advance_x: 0.58,
            bearing_x: 0.05,
            bearing_y: 0.68,
        };

        let bytes = entry.to_bytes();
        let parsed = GlyphEntry::from_bytes(&bytes).unwrap();
        assert_eq!(entry, parsed);
    }

    #[test]
    fn gpu_packing() {
        let entry = GlyphEntry {
            glyph_id: 65,
            atlas_x: 10,
            atlas_y: 20,
            atlas_w: 32,
            atlas_h: 32,
            advance_x: 0.6,
            bearing_x: 0.02,
            bearing_y: 0.7,
        };

        let packed = entry.to_gpu_u32s();
        assert_eq!(packed[0], 65); // glyph_id
        assert_eq!(packed[1] & 0xFFFF, 10); // atlas_x
        assert_eq!(packed[1] >> 16, 20); // atlas_y
    }
}
