// ─── Binary format (always available) ──────────────────────────────────────────

pub const FONT_ATLAS_MAGIC: [u8; 4] = *b"FNTa";
pub const FONT_ATLAS_PIXEL_FORMAT_RGBA: [u8; 4] = *b"RGBA";
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
/// 28..32   pixel_format [u8;4]  FourCC  (e.g. b"RGBA")
/// 32..     data        raw bitmap bytes
/// ```
#[derive(Debug)]
pub struct FontAtlasHeader {
    pub version: u32,
    pub font_type: u32,
    pub atlas_rows: u32,
    pub atlas_cols: u32,
    pub glyph_rows: u32,
    pub glyph_cols: u32,
    pub pixel_format: [u8; 4],
}

#[derive(Debug)]
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
                pixel_format: FONT_ATLAS_PIXEL_FORMAT_RGBA,
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
        out.extend_from_slice(&h.pixel_format);
        out.extend_from_slice(&self.data);
        out
    }

    /// Minimum size of a valid `FNTa` binary blob (header only, no data).
    pub const HEADER_SIZE: usize = 32;

    /// Deserialise from the `FNTa` binary format.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FontAtlasError> {
        if bytes.len() < Self::HEADER_SIZE {
            return Err(FontAtlasError::TooShort(bytes.len()));
        }
        let magic = &bytes[0..4];
        if magic != &FONT_ATLAS_MAGIC {
            return Err(FontAtlasError::BadMagic([
                magic[0], magic[1], magic[2], magic[3],
            ]));
        }
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if version != FONT_ATLAS_VERSION {
            return Err(FontAtlasError::UnsupportedVersion(version));
        }
        let font_type = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let atlas_rows = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let atlas_cols = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let glyph_rows = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        let glyph_cols = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        if glyph_rows == 0 || glyph_cols == 0 {
            return Err(FontAtlasError::ZeroGlyphDimension);
        }
        let pixel_format: [u8; 4] = bytes[28..32].try_into().unwrap();

        let bytes_per_pixel: u32 = match &pixel_format {
            b"RGBA" => 4,
            b"RGB\0" => 3,
            b"L\0\0\0" => 1,
            _ => return Err(FontAtlasError::UnknownPixelFormat(pixel_format)),
        };

        let data = &bytes[32..];
        let expected_len = (atlas_rows as u64)
            * (atlas_cols as u64)
            * (glyph_rows as u64)
            * (glyph_cols as u64)
            * (bytes_per_pixel as u64);
        if data.len() as u64 != expected_len {
            return Err(FontAtlasError::DataLengthMismatch {
                expected: expected_len,
                actual: data.len() as u64,
            });
        }

        Ok(Self {
            header: FontAtlasHeader {
                version,
                font_type,
                atlas_rows,
                atlas_cols,
                glyph_rows,
                glyph_cols,
                pixel_format,
            },
            data: data.to_vec(),
        })
    }

    /// Return the bytes-per-pixel for this atlas's pixel format, or `None`
    /// for unrecognised formats.
    pub fn bytes_per_pixel(&self) -> Option<u32> {
        match &self.header.pixel_format {
            b"RGBA" => Some(4),
            b"RGB\0" => Some(3),
            b"L\0\0\0" => Some(1),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum FontAtlasError {
    TooShort(usize),
    BadMagic([u8; 4]),
    UnsupportedVersion(u32),
    ZeroGlyphDimension,
    UnknownPixelFormat([u8; 4]),
    DataLengthMismatch { expected: u64, actual: u64 },
}

impl core::fmt::Display for FontAtlasError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort(n) => write!(f, "buffer too short ({n} bytes, need at least 32)"),
            Self::BadMagic(m) => write!(f, "bad magic: {:?}", m),
            Self::UnsupportedVersion(v) => write!(f, "unsupported version: {v}"),
            Self::ZeroGlyphDimension => write!(f, "glyph_rows and glyph_cols must be non-zero"),
            Self::UnknownPixelFormat(p) => write!(f, "unknown pixel format: {:?}", p),
            Self::DataLengthMismatch { expected, actual } => {
                write!(f, "data length mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2x2 grid, 4x4 glyph cells, RGBA (4 bpp) → 2*2*4*4*4 = 256 bytes of data
    fn sample_bin() -> FontAtlasBin {
        FontAtlasBin::new(
            FONT_TYPE_MONOSPACED,
            /*atlas_rows*/ 2,
            /*atlas_cols*/ 2,
            /*glyph_rows*/ 4,
            /*glyph_cols*/ 4,
            vec![0xAA; 256],
        )
    }

    #[test]
    fn to_bytes_starts_with_magic() {
        let bytes = sample_bin().to_bytes();
        assert_eq!(&bytes[0..4], b"FNTa");
    }

    #[test]
    fn to_bytes_header_length() {
        let bytes = sample_bin().to_bytes();
        assert_eq!(bytes.len(), FontAtlasBin::HEADER_SIZE + 256);
    }

    #[test]
    fn to_bytes_header_fields() {
        let bytes = sample_bin().to_bytes();
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), FONT_ATLAS_VERSION);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), FONT_TYPE_MONOSPACED);
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 2); // atlas_rows
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 2); // atlas_cols
        assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 4); // glyph_rows
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 4); // glyph_cols
        assert_eq!(&bytes[28..32], b"RGBA");
    }

    #[test]
    fn to_bytes_data_section() {
        let bytes = sample_bin().to_bytes();
        assert!(bytes[32..].iter().all(|&b| b == 0xAA));
    }

    #[test]
    fn from_bytes_round_trip() {
        let original = sample_bin();
        let bytes = original.to_bytes();
        let parsed = FontAtlasBin::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.header.version, original.header.version);
        assert_eq!(parsed.header.font_type, original.header.font_type);
        assert_eq!(parsed.header.atlas_rows, original.header.atlas_rows);
        assert_eq!(parsed.header.atlas_cols, original.header.atlas_cols);
        assert_eq!(parsed.header.glyph_rows, original.header.glyph_rows);
        assert_eq!(parsed.header.glyph_cols, original.header.glyph_cols);
        assert_eq!(parsed.header.pixel_format, original.header.pixel_format);
        assert_eq!(parsed.data, original.data);
    }

    #[test]
    fn from_bytes_empty_data() {
        // 0 atlas rows → expected data = 0 bytes
        let bin = FontAtlasBin::new(FONT_TYPE_MONOSPACED, 0, 0, 1, 1, vec![]);
        let bytes = bin.to_bytes();
        let parsed = FontAtlasBin::from_bytes(&bytes).unwrap();
        assert!(parsed.data.is_empty());
    }

    #[test]
    fn from_bytes_too_short() {
        let err = FontAtlasBin::from_bytes(&[0u8; 16]).unwrap_err();
        assert_eq!(err, FontAtlasError::TooShort(16));
    }

    #[test]
    fn from_bytes_bad_magic() {
        let mut bytes = sample_bin().to_bytes();
        bytes[0..4].copy_from_slice(b"NOPE");
        let err = FontAtlasBin::from_bytes(&bytes).unwrap_err();
        assert_eq!(err, FontAtlasError::BadMagic(*b"NOPE"));
    }

    #[test]
    fn from_bytes_unsupported_version() {
        let mut bytes = sample_bin().to_bytes();
        bytes[4..8].copy_from_slice(&99u32.to_le_bytes());
        let err = FontAtlasBin::from_bytes(&bytes).unwrap_err();
        assert_eq!(err, FontAtlasError::UnsupportedVersion(99));
    }

    #[test]
    fn from_bytes_zero_glyph_rows() {
        let mut bytes = sample_bin().to_bytes();
        bytes[20..24].copy_from_slice(&0u32.to_le_bytes()); // glyph_rows = 0
        let err = FontAtlasBin::from_bytes(&bytes).unwrap_err();
        assert_eq!(err, FontAtlasError::ZeroGlyphDimension);
    }

    #[test]
    fn from_bytes_zero_glyph_cols() {
        let mut bytes = sample_bin().to_bytes();
        bytes[24..28].copy_from_slice(&0u32.to_le_bytes()); // glyph_cols = 0
        let err = FontAtlasBin::from_bytes(&bytes).unwrap_err();
        assert_eq!(err, FontAtlasError::ZeroGlyphDimension);
    }

    #[test]
    fn from_bytes_unknown_pixel_format() {
        let mut bytes = sample_bin().to_bytes();
        bytes[28..32].copy_from_slice(b"BGRA");
        let err = FontAtlasBin::from_bytes(&bytes).unwrap_err();
        assert_eq!(err, FontAtlasError::UnknownPixelFormat(*b"BGRA"));
    }

    #[test]
    fn from_bytes_data_length_mismatch() {
        let mut bytes = sample_bin().to_bytes();
        // Truncate one byte from the data section
        bytes.pop();
        let err = FontAtlasBin::from_bytes(&bytes).unwrap_err();
        assert_eq!(
            err,
            FontAtlasError::DataLengthMismatch {
                expected: 256,
                actual: 255,
            }
        );
    }

    #[test]
    fn from_bytes_data_too_long() {
        let mut bytes = sample_bin().to_bytes();
        bytes.push(0xFF); // one extra byte
        let err = FontAtlasBin::from_bytes(&bytes).unwrap_err();
        assert_eq!(
            err,
            FontAtlasError::DataLengthMismatch {
                expected: 256,
                actual: 257,
            }
        );
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
