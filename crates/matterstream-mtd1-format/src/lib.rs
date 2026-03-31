//! mtd1 binary format: FourCC `mtd1` (0x3164746D LE)
//!
//! A flat, binary bytecode stream optimized for 120fps continuous GPU ingestion.
//! Layout: Header | Style Bank | 32-bit Instruction Stream

/// FourCC magic: `mtd1` in little-endian = 0x3164746D
pub const MTD1_MAGIC: u32 = 0x3164746D;

// ── Header ──────────────────────────────────────────────────────────────────

/// 16-byte fixed header at the start of every `.mtd1` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Mtd1Header {
    /// Magic number: must be `MTD1_MAGIC`
    pub magic: u32,
    /// Total file size in bytes
    pub total_size: u32,
    /// Byte offset of the Style Bank from file start
    pub style_bank_offset: u32,
    /// Byte offset of the Bytecode Stream from file start
    pub bytecode_offset: u32,
}

impl Mtd1Header {
    pub const SIZE: usize = 16;

    pub fn new(style_bank_offset: u32, bytecode_offset: u32, total_size: u32) -> Self {
        Self {
            magic: MTD1_MAGIC,
            total_size,
            style_bank_offset,
            bytecode_offset,
        }
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&self.magic.to_le_bytes());
        buf[4..8].copy_from_slice(&self.total_size.to_le_bytes());
        buf[8..12].copy_from_slice(&self.style_bank_offset.to_le_bytes());
        buf[12..16].copy_from_slice(&self.bytecode_offset.to_le_bytes());
        buf
    }

    pub fn from_bytes(buf: &[u8; 16]) -> Result<Self, Mtd1Error> {
        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != MTD1_MAGIC {
            return Err(Mtd1Error::BadMagic(magic));
        }
        Ok(Self {
            magic,
            total_size: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            style_bank_offset: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
            bytecode_offset: u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
        })
    }
}

// ── Style Bank ──────────────────────────────────────────────────────────────

/// 64-bit style entry: `[32b RGBA][8b Stroke][8b Behavior][8b Shape][8b FontIndex]`
///
/// `FontIndex` selects which font/atlas to use for text rendering:
/// - `0` = legacy bitmap font (backwards compatible default)
/// - `1-255` = index into the Glyph Atlas Bank (MSDF fonts)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BankedStyle(pub u64);

impl BankedStyle {
    /// Pack a style from components (font_index defaults to 0 = bitmap).
    pub fn new(rgba: u32, stroke_weight: u8, behavior_id: u8, shape_mode: u8) -> Self {
        Self::with_font(rgba, stroke_weight, behavior_id, shape_mode, 0)
    }

    /// Pack a style with an explicit font index for MSDF atlas selection.
    pub fn with_font(
        rgba: u32,
        stroke_weight: u8,
        behavior_id: u8,
        shape_mode: u8,
        font_index: u8,
    ) -> Self {
        let val = (rgba as u64) << 32
            | (stroke_weight as u64) << 24
            | (behavior_id as u64) << 16
            | (shape_mode as u64) << 8
            | (font_index as u64);
        Self(val)
    }

    pub fn rgba(self) -> u32 {
        (self.0 >> 32) as u32
    }

    pub fn stroke_weight(self) -> u8 {
        ((self.0 >> 24) & 0xFF) as u8
    }

    pub fn behavior_id(self) -> u8 {
        ((self.0 >> 16) & 0xFF) as u8
    }

    pub fn shape_mode(self) -> u8 {
        ((self.0 >> 8) & 0xFF) as u8
    }

    /// Font index: 0 = legacy bitmap, 1+ = MSDF atlas index.
    pub fn font_index(self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    pub fn to_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }

    pub fn from_bytes(buf: &[u8; 8]) -> Self {
        Self(u64::from_le_bytes(*buf))
    }
}

// ── 32-bit ISA ──────────────────────────────────────────────────────────────

/// Opcode constants (4-bit, upper nibble of the u32).
pub mod opcode {
    pub const OP_DRAW_GLYPH: u32 = 0x0;
    pub const OP_DRAW_SHAPE: u32 = 0x1;
    pub const OP_SET_STYLE: u32 = 0x2;
    pub const OP_SET_CURSOR: u32 = 0x3;
    pub const OP_SET_TOKEN: u32 = 0x5;
}

/// A single 32-bit instruction in the mtd1 ISA.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Command32(pub u32);

impl std::fmt::Debug for Command32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Command32(0x{:08X} = {})", self.0, self.disassemble())
    }
}

impl Command32 {
    /// Extract the 4-bit opcode (bits 31..28).
    #[inline]
    pub fn opcode(self) -> u32 {
        self.0 >> 28
    }

    // ── Constructors ────────────────────────────────────────────────────

    /// `OP_DRAW_GLYPH (0x0)`: `[4b Op][12b Advance X][16b Glyph ID]`
    pub fn draw_glyph(advance_x: u16, glyph_id: u16) -> Self {
        let advance = (advance_x as u32) & 0xFFF; // 12 bits
        let gid = glyph_id as u32; // 16 bits
        Self((opcode::OP_DRAW_GLYPH << 28) | (advance << 16) | gid)
    }

    /// `OP_DRAW_SHAPE (0x1)`: `[4b Op][14b Height][14b Width]`
    pub fn draw_shape(height: u16, width: u16) -> Self {
        let h = (height as u32) & 0x3FFF; // 14 bits
        let w = (width as u32) & 0x3FFF; // 14 bits
        Self((opcode::OP_DRAW_SHAPE << 28) | (h << 14) | w)
    }

    /// `OP_SET_STYLE (0x2)`: `[4b Op][28b Style Bank Index]`
    pub fn set_style(index: u32) -> Self {
        Self((opcode::OP_SET_STYLE << 28) | (index & 0x0FFF_FFFF))
    }

    /// `OP_SET_CURSOR (0x3)`: `[4b Op][14b Signed Y][14b Signed X]`
    ///
    /// Y and X are 14-bit two's complement values (-8192..8191).
    pub fn set_cursor(y: i16, x: i16) -> Self {
        let y14 = (y as u32) & 0x3FFF;
        let x14 = (x as u32) & 0x3FFF;
        Self((opcode::OP_SET_CURSOR << 28) | (y14 << 14) | x14)
    }

    /// `OP_SET_TOKEN (0x5)`: `[4b Op][28b Semantic Token ID]`
    pub fn set_token(token_id: u32) -> Self {
        Self((opcode::OP_SET_TOKEN << 28) | (token_id & 0x0FFF_FFFF))
    }

    // ── Decoders ────────────────────────────────────────────────────────

    /// Decode DRAW_GLYPH fields: (advance_x, glyph_id)
    pub fn decode_glyph(self) -> (u16, u16) {
        let advance = ((self.0 >> 16) & 0xFFF) as u16;
        let glyph_id = (self.0 & 0xFFFF) as u16;
        (advance, glyph_id)
    }

    /// Decode DRAW_SHAPE fields: (height, width)
    pub fn decode_shape(self) -> (u16, u16) {
        let h = ((self.0 >> 14) & 0x3FFF) as u16;
        let w = (self.0 & 0x3FFF) as u16;
        (h, w)
    }

    /// Decode SET_STYLE field: style bank index
    pub fn decode_style(self) -> u32 {
        self.0 & 0x0FFF_FFFF
    }

    /// Decode SET_CURSOR fields: (y, x) as signed 14-bit
    pub fn decode_cursor(self) -> (i16, i16) {
        let y_raw = ((self.0 >> 14) & 0x3FFF) as u16;
        let x_raw = (self.0 & 0x3FFF) as u16;
        // Sign-extend 14-bit to i16
        let y = if y_raw & 0x2000 != 0 {
            (y_raw | 0xC000) as i16
        } else {
            y_raw as i16
        };
        let x = if x_raw & 0x2000 != 0 {
            (x_raw | 0xC000) as i16
        } else {
            x_raw as i16
        };
        (y, x)
    }

    /// Decode SET_TOKEN field: semantic token ID
    pub fn decode_token(self) -> u32 {
        self.0 & 0x0FFF_FFFF
    }

    // ── Disassembly ─────────────────────────────────────────────────────

    /// Human-readable disassembly string.
    pub fn disassemble(self) -> String {
        match self.opcode() {
            opcode::OP_DRAW_GLYPH => {
                let (adv, gid) = self.decode_glyph();
                format!("DRAW_GLYPH id:{}, adv:{}", gid, adv)
            }
            opcode::OP_DRAW_SHAPE => {
                let (h, w) = self.decode_shape();
                format!("DRAW_SHAPE w:{}, h:{}", w, h)
            }
            opcode::OP_SET_STYLE => {
                format!("SET_STYLE idx:{}", self.decode_style())
            }
            opcode::OP_SET_CURSOR => {
                let (y, x) = self.decode_cursor();
                format!("SET_CURSOR x:{}, y:{}", x, y)
            }
            opcode::OP_SET_TOKEN => {
                format!("SET_TOKEN id:{}", self.decode_token())
            }
            op => format!("UNKNOWN(0x{:X})", op),
        }
    }
}

// ── Serialization ───────────────────────────────────────────────────────────

/// Errors during mtd1 parsing.
#[derive(Debug, Clone)]
pub enum Mtd1Error {
    BadMagic(u32),
    TooShort,
    InvalidOffset,
}

impl std::fmt::Display for Mtd1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMagic(m) => write!(f, "bad magic: 0x{:08X}, expected 0x{:08X}", m, MTD1_MAGIC),
            Self::TooShort => write!(f, "buffer too short for mtd1 header"),
            Self::InvalidOffset => write!(f, "invalid offset in mtd1 header"),
        }
    }
}

impl std::error::Error for Mtd1Error {}

/// Complete mtd1 document in memory.
#[derive(Debug, Clone)]
pub struct Mtd1Document {
    pub styles: Vec<BankedStyle>,
    pub instructions: Vec<Command32>,
    /// Optional serialized glyph atlas data (MSDF). When present, styles with
    /// `font_index > 0` reference glyphs from this atlas.
    pub glyph_atlas: Option<Vec<u8>>,
}

impl Mtd1Document {
    pub fn new() -> Self {
        Self {
            styles: Vec::new(),
            instructions: Vec::new(),
            glyph_atlas: None,
        }
    }

    /// Serialize to binary `.mtd1` format.
    ///
    /// Layout: `Header | Style Bank | [Glyph Atlas] | Bytecode Stream`
    pub fn to_bytes(&self) -> Vec<u8> {
        let style_bank_offset = Mtd1Header::SIZE as u32;
        let style_bank_size = (self.styles.len() * 8) as u32;
        let atlas_size = self.glyph_atlas.as_ref().map_or(0, |a| a.len() as u32);
        let bytecode_offset = style_bank_offset + style_bank_size + atlas_size;
        let bytecode_size = (self.instructions.len() * 4) as u32;
        let total_size = bytecode_offset + bytecode_size;

        let header = Mtd1Header::new(style_bank_offset, bytecode_offset, total_size);

        let mut buf = Vec::with_capacity(total_size as usize);
        buf.extend_from_slice(&header.to_bytes());

        for style in &self.styles {
            buf.extend_from_slice(&style.to_bytes());
        }

        if let Some(atlas) = &self.glyph_atlas {
            buf.extend_from_slice(atlas);
        }

        for cmd in &self.instructions {
            buf.extend_from_slice(&cmd.0.to_le_bytes());
        }

        buf
    }

    /// Deserialize from binary `.mtd1` format.
    pub fn from_bytes(data: &[u8]) -> Result<Self, Mtd1Error> {
        if data.len() < Mtd1Header::SIZE {
            return Err(Mtd1Error::TooShort);
        }

        let header = Mtd1Header::from_bytes(data[..16].try_into().unwrap())?;

        let style_start = header.style_bank_offset as usize;
        let bytecode_start = header.bytecode_offset as usize;
        let total = header.total_size as usize;

        if bytecode_start < style_start || total < bytecode_start || total > data.len() {
            return Err(Mtd1Error::InvalidOffset);
        }

        // Parse styles — styles end where atlas or bytecode begins
        // Styles are 8 bytes each; the atlas (if present) sits between styles and bytecode
        // The atlas starts after the last complete 8-byte style entry before bytecode_start
        // We detect atlas presence by checking if there's data between the style region and bytecode
        let raw_style_region = bytecode_start - style_start;
        let num_styles = raw_style_region / 8;
        // If the style region isn't evenly divisible by 8, the remainder is atlas data
        // But with the new format, atlas is explicitly between styles and bytecode.
        // For backwards compat: if no atlas, style_region == num_styles * 8.
        let style_data_end = style_start + num_styles * 8;

        let mut styles = Vec::with_capacity(num_styles);
        for i in 0..num_styles {
            let offset = style_start + i * 8;
            let arr: [u8; 8] = data[offset..offset + 8].try_into().unwrap();
            styles.push(BankedStyle::from_bytes(&arr));
        }

        // Any data between end of styles and bytecode is glyph atlas
        let glyph_atlas = if style_data_end < bytecode_start {
            Some(data[style_data_end..bytecode_start].to_vec())
        } else {
            None
        };

        // Parse instructions
        let instr_bytes = &data[bytecode_start..total];
        let num_instrs = instr_bytes.len() / 4;
        let mut instructions = Vec::with_capacity(num_instrs);
        for i in 0..num_instrs {
            let offset = i * 4;
            let val = u32::from_le_bytes(instr_bytes[offset..offset + 4].try_into().unwrap());
            instructions.push(Command32(val));
        }

        Ok(Self {
            styles,
            instructions,
            glyph_atlas,
        })
    }

    /// Debug-dump the instruction stream as assembly-like text.
    pub fn debug_dump(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "=== mtd1 Debug Dump ===\nStyles: {}\nInstructions: {}\n\n",
            self.styles.len(),
            self.instructions.len()
        ));

        out.push_str("-- Style Bank --\n");
        for (i, style) in self.styles.iter().enumerate() {
            out.push_str(&format!(
                "  [{:04}] RGBA:#{:08X} stroke:{} behavior:{} shape:{} font:{}\n",
                i,
                style.rgba(),
                style.stroke_weight(),
                style.behavior_id(),
                style.shape_mode(),
                style.font_index(),
            ));
        }

        out.push_str("\n-- Bytecode Stream --\n");
        for (i, cmd) in self.instructions.iter().enumerate() {
            out.push_str(&format!("  0x{:04X}: {}\n", i, cmd.disassemble()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_header() {
        let header = Mtd1Header::new(16, 80, 200);
        let bytes = header.to_bytes();
        let parsed = Mtd1Header::from_bytes(&bytes).unwrap();
        assert_eq!(header, parsed);
    }

    #[test]
    fn bad_magic_rejected() {
        let mut bytes = Mtd1Header::new(16, 80, 200).to_bytes();
        bytes[0] = 0xFF;
        assert!(Mtd1Header::from_bytes(&bytes).is_err());
    }

    #[test]
    fn banked_style_roundtrip() {
        let style = BankedStyle::new(0xFF0088AA, 3, 7, 2);
        assert_eq!(style.rgba(), 0xFF0088AA);
        assert_eq!(style.stroke_weight(), 3);
        assert_eq!(style.behavior_id(), 7);
        assert_eq!(style.shape_mode(), 2);

        let bytes = style.to_bytes();
        let parsed = BankedStyle::from_bytes(&bytes);
        assert_eq!(style, parsed);
    }

    #[test]
    fn draw_glyph_encode_decode() {
        let cmd = Command32::draw_glyph(8, 42);
        assert_eq!(cmd.opcode(), opcode::OP_DRAW_GLYPH);
        let (adv, gid) = cmd.decode_glyph();
        assert_eq!(adv, 8);
        assert_eq!(gid, 42);
    }

    #[test]
    fn draw_shape_encode_decode() {
        let cmd = Command32::draw_shape(100, 200);
        assert_eq!(cmd.opcode(), opcode::OP_DRAW_SHAPE);
        let (h, w) = cmd.decode_shape();
        assert_eq!(h, 100);
        assert_eq!(w, 200);
    }

    #[test]
    fn set_style_encode_decode() {
        let cmd = Command32::set_style(42);
        assert_eq!(cmd.opcode(), opcode::OP_SET_STYLE);
        assert_eq!(cmd.decode_style(), 42);
    }

    #[test]
    fn set_cursor_signed() {
        // Positive values
        let cmd = Command32::set_cursor(10, 20);
        assert_eq!(cmd.opcode(), opcode::OP_SET_CURSOR);
        let (y, x) = cmd.decode_cursor();
        assert_eq!(y, 10);
        assert_eq!(x, 20);

        // Negative values
        let cmd = Command32::set_cursor(-50, -100);
        let (y, x) = cmd.decode_cursor();
        assert_eq!(y, -50);
        assert_eq!(x, -100);
    }

    #[test]
    fn set_token_encode_decode() {
        let cmd = Command32::set_token(0x00ABCDEF);
        assert_eq!(cmd.opcode(), opcode::OP_SET_TOKEN);
        assert_eq!(cmd.decode_token(), 0x00ABCDEF);
    }

    #[test]
    fn document_roundtrip() {
        let mut doc = Mtd1Document::new();
        doc.styles.push(BankedStyle::new(0xFF000000, 1, 0, 0));
        doc.styles.push(BankedStyle::new(0x336699FF, 2, 1, 1));
        doc.instructions.push(Command32::set_style(0));
        doc.instructions.push(Command32::set_cursor(10, 20));
        doc.instructions.push(Command32::draw_glyph(8, 72));
        doc.instructions.push(Command32::draw_glyph(7, 101));
        doc.instructions.push(Command32::draw_shape(50, 200));

        let bytes = doc.to_bytes();
        let parsed = Mtd1Document::from_bytes(&bytes).unwrap();

        assert_eq!(doc.styles.len(), parsed.styles.len());
        assert_eq!(doc.instructions.len(), parsed.instructions.len());
        for (a, b) in doc.styles.iter().zip(parsed.styles.iter()) {
            assert_eq!(a, b);
        }
        for (a, b) in doc.instructions.iter().zip(parsed.instructions.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn disassembly_output() {
        let cmd = Command32::set_cursor(10, 20);
        assert_eq!(cmd.disassemble(), "SET_CURSOR x:20, y:10");
    }
}
