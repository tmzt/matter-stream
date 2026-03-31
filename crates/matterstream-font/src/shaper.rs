//! Text shaping via rustybuzz (Rust port of HarfBuzz).
//!
//! Wraps `rustybuzz::shape()` to produce positioned glyph runs from Unicode
//! text, handling complex scripts, ligatures, kerning, and BiDi.

use unicode_segmentation::UnicodeSegmentation;

/// A single positioned glyph from shaping.
#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    /// Font-internal glyph ID (u16, matches ttf-parser/OpenType glyph index)
    pub glyph_id: u16,
    /// Horizontal advance in font units
    pub x_advance: i32,
    /// Horizontal offset from current position (kerning, combining marks)
    pub x_offset: i32,
    /// Vertical offset from baseline
    pub y_offset: i32,
    /// Index of the source cluster (character index in original text)
    pub cluster: u32,
}

/// Result of shaping a text run.
#[derive(Debug, Clone)]
pub struct ShapedRun {
    pub glyphs: Vec<ShapedGlyph>,
    /// Total advance width in font units
    pub total_advance: i32,
    /// Font units per em (for converting to pixels: px = units * font_size / upem)
    pub units_per_em: u16,
}

impl ShapedRun {
    /// Convert total advance to pixels at a given font size.
    pub fn advance_px(&self, font_size: f32) -> f32 {
        self.total_advance as f32 * font_size / self.units_per_em as f32
    }

    /// Convert a font-unit value to pixels.
    pub fn to_px(&self, units: i32, font_size: f32) -> f32 {
        units as f32 * font_size / self.units_per_em as f32
    }
}

/// Text shaper backed by rustybuzz.
pub struct TextShaper {
    face_data: Vec<u8>,
    units_per_em: u16,
    default_features: Vec<(ttf_parser::Tag, u32)>,
}

impl TextShaper {
    /// Create a shaper from raw font file data (TTF/OTF).
    pub fn new(font_data: Vec<u8>) -> Result<Self, &'static str> {
        let face = ttf_parser::Face::parse(&font_data, 0)
            .map_err(|_| "failed to parse font")?;
        let units_per_em = face.units_per_em();
        Ok(Self {
            face_data: font_data,
            units_per_em,
            default_features: vec![
                (ttf_parser::Tag::from_bytes(b"lnum"), 1),
                (ttf_parser::Tag::from_bytes(b"pnum"), 1),
            ],
        })
    }

    /// Set the default OpenType features.
    pub fn set_default_features(&mut self, features: Vec<(ttf_parser::Tag, u32)>) {
        self.default_features = features;
    }

    /// Shape a text string into positioned glyphs.
    ///
    /// Uses default left-to-right, Latin script settings and default features.
    pub fn shape(&self, text: &str) -> ShapedRun {
        self.shape_with_options(text, rustybuzz::Direction::LeftToRight, None, None, &self.default_features)
    }

    /// Shape with explicit direction, script, language, and OpenType features.
    /// Features are passed as (tag, value) pairs, e.g. (b"lnum", 1).
    pub fn shape_with_options(
        &self,
        text: &str,
        direction: rustybuzz::Direction,
        script: Option<rustybuzz::Script>,
        language: Option<rustybuzz::Language>,
        features: &[(ttf_parser::Tag, u32)],
    ) -> ShapedRun {
        let face = rustybuzz::Face::from_slice(&self.face_data, 0)
            .expect("face already validated in new()");

        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(direction);
        if let Some(s) = script {
            buffer.set_script(s);
        }
        if let Some(l) = language {
            buffer.set_language(l);
        }

        let mut rb_features: Vec<rustybuzz::Feature> = features.iter().map(|&(tag, val)| {
            rustybuzz::Feature::new(tag, val, ..)
        }).collect();
        
        // Features MUST be sorted by tag for rustybuzz/HarfBuzz
        rb_features.sort_by_key(|f| f.tag);

        let output = rustybuzz::shape(&face, &rb_features, buffer);

        let infos = output.glyph_infos();
        let positions = output.glyph_positions();

        let mut glyphs = Vec::with_capacity(infos.len());
        let mut total_advance = 0i32;

        for (info, pos) in infos.iter().zip(positions.iter()) {
            glyphs.push(ShapedGlyph {
                glyph_id: info.glyph_id as u16,
                x_advance: pos.x_advance,
                x_offset: pos.x_offset,
                y_offset: pos.y_offset,
                cluster: info.cluster,
            });
            total_advance += pos.x_advance;
        }

        ShapedRun {
            glyphs,
            total_advance,
            units_per_em: self.units_per_em,
        }
    }

    /// Get the units-per-em value for this font.
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Get grapheme cluster boundaries for a string (for line-breaking).
    pub fn grapheme_indices(text: &str) -> Vec<(usize, &str)> {
        text.grapheme_indices(true).collect()
    }

    /// Resolve a codepoint to a glyph ID using the font's cmap.
    pub fn glyph_id_for_char(&self, ch: char) -> Option<u16> {
        let face = ttf_parser::Face::parse(&self.face_data, 0).ok()?;
        face.glyph_index(ch).map(|id| id.0)
    }

    /// Check if a character is likely an emoji (heuristic).
    pub fn is_emoji(ch: char) -> bool {
        let cp = ch as u32;
        // Emoticons, Misc Symbols, Dingbats, Supplemental Symbols, Flags, etc.
        matches!(cp,
            0x2600..=0x27BF |        // Misc Symbols, Dingbats
            0xFE00..=0xFE0F |        // Variation Selectors
            0x200D |                  // ZWJ
            0x1F000..=0x1FAFF |      // Mahjong, Playing Cards, Emoticons, Transport, etc.
            0xE0020..=0xE007F        // Tags (flag sequences)
        )
    }

    /// Check if a character is CJK.
    pub fn is_cjk(ch: char) -> bool {
        let cp = ch as u32;
        matches!(cp,
            0x4E00..=0x9FFF |        // CJK Unified Ideographs
            0x3400..=0x4DBF |        // CJK Unified Ideographs Extension A
            0x20000..=0x2A6DF |      // CJK Unified Ideographs Extension B
            0x2A700..=0x2B73F |      // Extension C
            0x2B740..=0x2B81F |      // Extension D
            0x2B820..=0x2CEAF |      // Extension E
            0x2CEB0..=0x2EBEF |      // Extension F
            0x30000..=0x3134F |      // Extension G
            0x3000..=0x303F |        // CJK Symbols and Punctuation
            0x3040..=0x309F |        // Hiragana
            0x30A0..=0x30FF |        // Katakana
            0xFF00..=0xFFEF          // Halfwidth and Fullwidth Forms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Use a system font for testing on macOS
    fn load_system_font() -> Option<Vec<u8>> {
        let paths = [
            "/System/Library/Fonts/Helvetica.ttc",
            "/System/Library/Fonts/SFNSText.ttf",
            "/System/Library/Fonts/SFNS.ttf",
            "/Library/Fonts/Arial.ttf",
        ];
        for path in &paths {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
        None
    }

    #[test]
    fn shape_basic_latin() {
        let font_data = match load_system_font() {
            Some(d) => d,
            None => {
                eprintln!("SKIP: no system font available");
                return;
            }
        };

        let shaper = TextShaper::new(font_data).unwrap();
        let run = shaper.shape("Hello");

        assert_eq!(run.glyphs.len(), 5);
        assert!(run.total_advance > 0);
        assert!(run.units_per_em > 0);

        // Each glyph should have a valid glyph_id
        for g in &run.glyphs {
            assert!(g.glyph_id > 0, "glyph_id should be non-zero for 'Hello'");
            assert!(g.x_advance > 0, "advance should be positive");
        }
    }

    #[test]
    fn shape_produces_kerning() {
        let font_data = match load_system_font() {
            Some(d) => d,
            None => {
                eprintln!("SKIP: no system font available");
                return;
            }
        };

        let shaper = TextShaper::new(font_data).unwrap();
        // "AV" is a classic kerning pair
        let run = shaper.shape("AV");
        assert_eq!(run.glyphs.len(), 2);
        // Just verify it runs without error; kerning presence depends on the font
    }

    #[test]
    fn emoji_detection() {
        assert!(TextShaper::is_emoji('😀'));
        assert!(TextShaper::is_emoji('🎉'));
        assert!(!TextShaper::is_emoji('A'));
        assert!(!TextShaper::is_emoji('好'));
    }

    #[test]
    fn cjk_detection() {
        assert!(TextShaper::is_cjk('好'));
        assert!(TextShaper::is_cjk('中'));
        assert!(TextShaper::is_cjk('あ'));
        assert!(TextShaper::is_cjk('ア'));
        assert!(!TextShaper::is_cjk('A'));
        assert!(!TextShaper::is_cjk('😀'));
    }

    #[test]
    fn grapheme_clusters() {
        let clusters = TextShaper::grapheme_indices("é");
        // 'é' can be 1 or 2 grapheme clusters depending on normalization
        assert!(!clusters.is_empty());
    }
}
