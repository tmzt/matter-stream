//! GPU-uploadable glyph entry table.
//!
//! Each `GlyphEntry` maps a glyph ID to its atlas rectangle, standard advance,
//! and the per-glyph autoframe projection for mapping em-normalized coordinates
//! to atlas cell pixels.

/// Per-glyph atlas entry with autoframe projection, 32 bytes packed.
///
/// The projection maps **em-normalized** coordinates (screen_delta / font_size)
/// to atlas cell pixels. It is pre-scaled by upem at build time so the shader
/// doesn't need to know the font's units-per-em.
///
/// ```text
/// [2b glyph_id][2b atlas_x][2b atlas_y][2b atlas_w][2b atlas_h][2b _pad]
/// [4b advance_x][4b proj_sx][4b proj_sy][4b proj_tx][4b proj_ty]
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
    /// Projection scale X: em-normalized → atlas cell pixels
    pub proj_sx: f32,
    /// Projection scale Y: em-normalized → atlas cell pixels
    pub proj_sy: f32,
    /// Projection translate X: atlas cell pixel offset
    pub proj_tx: f32,
    /// Projection translate Y: atlas cell pixel offset
    pub proj_ty: f32,
}

impl GlyphEntry {
    /// Packed size in bytes for serialization.
    pub const PACKED_SIZE: usize = 32;

    /// Serialize to 32 bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[0..2].copy_from_slice(&self.glyph_id.to_le_bytes());
        buf[2..4].copy_from_slice(&self.atlas_x.to_le_bytes());
        buf[4..6].copy_from_slice(&self.atlas_y.to_le_bytes());
        buf[6..8].copy_from_slice(&self.atlas_w.to_le_bytes());
        buf[8..10].copy_from_slice(&self.atlas_h.to_le_bytes());
        // 10..12 = padding
        buf[12..16].copy_from_slice(&self.advance_x.to_le_bytes());
        buf[16..20].copy_from_slice(&self.proj_sx.to_le_bytes());
        buf[20..24].copy_from_slice(&self.proj_sy.to_le_bytes());
        buf[24..28].copy_from_slice(&self.proj_tx.to_le_bytes());
        buf[28..32].copy_from_slice(&self.proj_ty.to_le_bytes());
        buf
    }

    /// Deserialize from bytes (must be at least 32 bytes).
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 32 {
            return Err("glyph entry too short");
        }
        Ok(Self {
            glyph_id: u16::from_le_bytes([data[0], data[1]]),
            atlas_x: u16::from_le_bytes([data[2], data[3]]),
            atlas_y: u16::from_le_bytes([data[4], data[5]]),
            atlas_w: u16::from_le_bytes([data[6], data[7]]),
            atlas_h: u16::from_le_bytes([data[8], data[9]]),
            advance_x: f32::from_le_bytes([data[12], data[13], data[14], data[15]]),
            proj_sx: f32::from_le_bytes([data[16], data[17], data[18], data[19]]),
            proj_sy: f32::from_le_bytes([data[20], data[21], data[22], data[23]]),
            proj_tx: f32::from_le_bytes([data[24], data[25], data[26], data[27]]),
            proj_ty: f32::from_le_bytes([data[28], data[29], data[30], data[31]]),
        })
    }

    /// Pack for GPU upload: 2 × vec4<u32> = 32 bytes.
    ///
    /// ```text
    /// g0 = [glyph_id, atlas_xy_packed, atlas_wh_packed, advance_x_bits]
    /// g1 = [proj_sx_bits, proj_sy_bits, proj_tx_bits, proj_ty_bits]
    /// ```
    pub fn to_gpu_u32s(&self) -> [u32; 8] {
        [
            self.glyph_id as u32,
            (self.atlas_x as u32) | ((self.atlas_y as u32) << 16),
            (self.atlas_w as u32) | ((self.atlas_h as u32) << 16),
            self.advance_x.to_bits(),
            self.proj_sx.to_bits(),
            self.proj_sy.to_bits(),
            self.proj_tx.to_bits(),
            self.proj_ty.to_bits(),
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
            atlas_w: 128,
            atlas_h: 128,
            advance_x: 0.58,
            proj_sx: 45.2,
            proj_sy: 45.2,
            proj_tx: 4.0,
            proj_ty: 20.0,
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
            atlas_w: 128,
            atlas_h: 128,
            advance_x: 0.6,
            proj_sx: 50.0,
            proj_sy: 50.0,
            proj_tx: 5.0,
            proj_ty: 25.0,
        };

        let packed = entry.to_gpu_u32s();
        assert_eq!(packed[0], 65); // glyph_id
        assert_eq!(packed[1] & 0xFFFF, 10); // atlas_x
        assert_eq!(packed[1] >> 16, 20); // atlas_y
        assert_eq!(packed[3], entry.advance_x.to_bits()); // advance in g0.w
        assert_eq!(packed[4], entry.proj_sx.to_bits()); // proj in g1
    }
}
