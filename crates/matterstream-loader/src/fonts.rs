// ─── Binary format (always available) ──────────────────────────────────────────

pub const FONT_ATLAS_MAGIC: [u8; 4] = *b"FNTa";
pub const FONT_ATLAS_COLOR_RGBA: [u8; 4] = *b"RGBA";
pub const FONT_TYPE_MONOSPACED: u32 = 1;
pub const FONT_ATLAS_VERSION: u32 = 1;

/// Header for the serialised font atlas binary (`FNTa` format).
///
/// Binary layout (all integers little-endian):
/// ```text
///  0.. 4   magic       b"FNTa"
///  4.. 8   version     u32
///  8..12   font_type   u32  (1 = monospaced)
/// 12..16   atlas_rows  u32  (glyph-cell rows in the atlas grid)
/// 16..20   atlas_cols  u32  (glyph-cell columns in the atlas grid)
/// 20..24   glyph_rows  u32  (pixel height of one glyph cell)
/// 24..28   glyph_cols  u32  (pixel width of one glyph cell)
/// 28..32   color_depth [u8;4]  FourCC  (e.g. b"RGBA")
/// 32..     data        raw bitmap bytes
/// ```
pub struct FontAtlasHeader {
    pub version: u32,
    pub font_type: u32,
    pub atlas_rows: u32,
    pub atlas_cols: u32,
    pub glyph_rows: u32,
    pub glyph_cols: u32,
    pub color_depth: [u8; 4],
}

pub struct FontAtlasBin {
    pub header: FontAtlasHeader,
    pub data: Vec<u8>,
}

impl FontAtlasBin {
    pub fn new(
        font_type: u32,
        atlas_rows: u32,
        atlas_cols: u32,
        glyph_rows: u32,
        glyph_cols: u32,
        data: Vec<u8>,
    ) -> Self {
        Self {
            header: FontAtlasHeader {
                version: FONT_ATLAS_VERSION,
                font_type,
                atlas_rows,
                atlas_cols,
                glyph_rows,
                glyph_cols,
                color_depth: FONT_ATLAS_COLOR_RGBA,
            },
            data,
        }
    }

    /// Serialise to the `FNTa` binary format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let h = &self.header;
        let mut out = Vec::with_capacity(32 + self.data.len());
        out.extend_from_slice(&FONT_ATLAS_MAGIC);
        out.extend_from_slice(&h.version.to_le_bytes());
        out.extend_from_slice(&h.font_type.to_le_bytes());
        out.extend_from_slice(&h.atlas_rows.to_le_bytes());
        out.extend_from_slice(&h.atlas_cols.to_le_bytes());
        out.extend_from_slice(&h.glyph_rows.to_le_bytes());
        out.extend_from_slice(&h.glyph_cols.to_le_bytes());
        out.extend_from_slice(&h.color_depth);
        out.extend_from_slice(&self.data);
        out
    }
}

// ─── TTF rasterisation (feature = "ttf") ───────────────────────────────────────

#[cfg(feature = "ttf")]
use rusttype::{Font, Scale};
#[cfg(feature = "ttf")]
use image::{ImageBuffer, Rgba};
#[cfg(feature = "ttf")]
use std::collections::HashMap;

#[cfg(feature = "ttf")]
pub struct GlyphInfo {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub uv_x: f32,
    pub uv_y: f32,
    pub uv_width: f32,
    pub uv_height: f32,
    pub advance_width: f32,
}

#[cfg(feature = "ttf")]
pub struct FontAtlas {
    pub font_data: Vec<u8>,
    pub texture: ImageBuffer<Rgba<u8>, Vec<u8>>,
    pub glyph_map: HashMap<char, GlyphInfo>,
}

#[cfg(feature = "ttf")]
impl FontAtlas {
    pub fn new(font_data: Vec<u8>, font_size: f32) -> Self {
        let _font = Font::try_from_bytes(&font_data).expect("failed to load font");
        let _scale = Scale::uniform(font_size);
        let texture = ImageBuffer::new(256, 256);
        let glyph_map = HashMap::new();

        Self { font_data, texture, glyph_map }
    }

    pub fn get_glyph_info(&self, c: char) -> Option<&GlyphInfo> {
        self.glyph_map.get(&c)
    }

    /// Produce a serialisable `FontAtlasBin` from this atlas.
    ///
    /// `glyph_cols` and `glyph_rows` are the pixel dimensions of one glyph cell;
    /// the grid dimensions are derived from the texture size.
    pub fn to_bin(&self, glyph_cols: u32, glyph_rows: u32) -> FontAtlasBin {
        let (tex_w, tex_h) = self.texture.dimensions();
        FontAtlasBin::new(
            FONT_TYPE_MONOSPACED,
            tex_h / glyph_rows,
            tex_w / glyph_cols,
            glyph_rows,
            glyph_cols,
            self.texture.as_raw().clone(),
        )
    }
}
