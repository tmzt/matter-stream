//! Convert mtd1 `Command32` instruction streams to `SdfDrawCmd` for GPU rendering.
//!
//! MSDF (type 8) is the default text rendering path for all characters.
//! Bitmap (type 4) is reserved for pictographic/emoji glyphs that require
//! color rasterization — selected by `font_index == 255` in the style bank.

use matterstream_common::sdf::{SdfDrawCmd, DRAW_TYPE_BOX, DRAW_TYPE_TEXT, DRAW_TYPE_MSDF_TEXT};

use crate::mtd1_format::{BankedStyle, Mtd1Document, opcode};

/// Font index that selects bitmap rendering for pictographic glyphs.
pub const FONT_INDEX_PICTOGRAPHIC: u8 = 255;

/// Result of converting an mtd1 document to GPU-renderable SdfDrawCmd list.
pub struct SdfFrame {
    pub draws: Vec<SdfDrawCmd>,
    pub char_buffer: Vec<u32>,
}

/// Convert an `Mtd1Document` to GPU-renderable `SdfFrame`.
///
/// Uses MSDF (type 8) for all text by default. Falls back to bitmap (type 4)
/// only when the active style has `font_index == 255` (pictographic).
///
/// `glyph_id_to_table_index` maps font glyph IDs to GPU glyph_table indices.
/// `standard_advances` maps glyph IDs to their standard advance (normalized to em).
///
/// MSDF char_buffer entries: `[16b glyph_table_index | 16b advance_delta_biased]`
/// Bitmap char_buffer entries: `[32b codepoint]`
pub fn mtd1_to_sdf(
    doc: &Mtd1Document,
    glyph_id_to_table_index: &std::collections::HashMap<u16, u16>,
    standard_advances: &std::collections::HashMap<u16, f32>,
    font_size: f32,
    px_range: f32,
) -> SdfFrame {
    let mut draws = Vec::new();
    let mut char_buffer: Vec<u32> = Vec::new();

    let mut cursor_x: f32 = 0.0;
    let mut cursor_y: f32 = 0.0;
    let mut current_color: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    let mut is_pictographic = false;

    // Text batching state
    let mut batch_start_x: f32 = 0.0;
    let mut batch_y: f32 = 0.0;
    let mut batch_char_offset: u32 = 0;
    let mut batch_char_count: u32 = 0;
    let mut batch_color: [f32; 4] = [1.0; 4];
    let mut batch_pictographic = false;
    let mut in_batch = false;

    let flush = |draws: &mut Vec<SdfDrawCmd>,
                 start_x: f32,
                 y: f32,
                 char_offset: u32,
                 char_count: u32,
                 color: [f32; 4],
                 font_size: f32,
                 px_range: f32,
                 pictographic: bool| {
        if char_count == 0 {
            return;
        }
        let packed_slot = (char_offset << 16) | (char_count & 0xFFFF);
        let total_width = char_count as f32 * font_size * 0.6;
        if pictographic {
            // Bitmap path for emoji/pictographic
            draws.push(SdfDrawCmd {
                pos: [start_x, y],
                size: [total_width, font_size],
                color,
                params: [DRAW_TYPE_TEXT, 0.0, 0.0, f32::from_bits(packed_slot)],
            });
        } else {
            // MSDF path — default for all text
            draws.push(SdfDrawCmd {
                pos: [start_x, y],
                size: [total_width, font_size],
                color,
                params: [DRAW_TYPE_MSDF_TEXT, px_range, 0.0, f32::from_bits(packed_slot)],
            });
        }
    };

    for cmd in &doc.instructions {
        match cmd.opcode() {
            opcode::OP_SET_CURSOR => {
                if in_batch {
                    flush(
                        &mut draws, batch_start_x, batch_y,
                        batch_char_offset, batch_char_count,
                        batch_color, font_size, px_range, batch_pictographic,
                    );
                    in_batch = false;
                }
                let (y, x) = cmd.decode_cursor();
                cursor_x = x as f32;
                cursor_y = y as f32;
            }

            opcode::OP_SET_STYLE => {
                if in_batch {
                    flush(
                        &mut draws, batch_start_x, batch_y,
                        batch_char_offset, batch_char_count,
                        batch_color, font_size, px_range, batch_pictographic,
                    );
                    in_batch = false;
                }
                let idx = cmd.decode_style() as usize;
                if idx < doc.styles.len() {
                    current_color = banked_style_to_color(&doc.styles[idx]);
                    is_pictographic = doc.styles[idx].font_index() == FONT_INDEX_PICTOGRAPHIC;
                }
            }

            opcode::OP_DRAW_GLYPH => {
                let (advance, glyph_id) = cmd.decode_glyph();

                // Flush if switching between MSDF and pictographic
                if in_batch && batch_pictographic != is_pictographic {
                    flush(
                        &mut draws, batch_start_x, batch_y,
                        batch_char_offset, batch_char_count,
                        batch_color, font_size, px_range, batch_pictographic,
                    );
                    in_batch = false;
                }

                if !in_batch {
                    batch_start_x = cursor_x;
                    batch_y = cursor_y;
                    batch_char_offset = char_buffer.len() as u32;
                    batch_char_count = 0;
                    batch_color = current_color;
                    batch_pictographic = is_pictographic;
                    in_batch = true;
                }

                if is_pictographic {
                    // Bitmap: store codepoint directly
                    char_buffer.push(glyph_id as u32);
                } else {
                    // MSDF: pack [glyph_table_index << 16 | advance_delta_biased]
                    let gt_idx = glyph_id_to_table_index
                        .get(&glyph_id)
                        .copied()
                        .unwrap_or(0);
                    let std_advance_norm = standard_advances.get(&glyph_id).copied().unwrap_or(0.5);
                    let std_advance_px = std_advance_norm * font_size;
                    let delta_px = advance as f32 - std_advance_px;
                    let delta_fixed = ((delta_px * 16.0) as i32 + 2048).clamp(0, 0xFFFF) as u32;
                    char_buffer.push((gt_idx as u32) << 16 | delta_fixed);
                }
                batch_char_count += 1;
                cursor_x += advance as f32;
            }

            opcode::OP_DRAW_SHAPE => {
                if in_batch {
                    flush(
                        &mut draws, batch_start_x, batch_y,
                        batch_char_offset, batch_char_count,
                        batch_color, font_size, px_range, batch_pictographic,
                    );
                    in_batch = false;
                }
                let (h, w) = cmd.decode_shape();
                draws.push(SdfDrawCmd {
                    pos: [cursor_x, cursor_y],
                    size: [w as f32, h as f32],
                    color: current_color,
                    params: [DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
                });
            }

            opcode::OP_SET_TOKEN => {}
            _ => {}
        }
    }

    if in_batch {
        flush(
            &mut draws, batch_start_x, batch_y,
            batch_char_offset, batch_char_count,
            batch_color, font_size, px_range, batch_pictographic,
        );
    }

    SdfFrame { draws, char_buffer }
}

/// Convert a `BankedStyle` RGBA to [f32; 4] color.
fn banked_style_to_color(style: &BankedStyle) -> [f32; 4] {
    let rgba = style.rgba();
    [
        ((rgba >> 24) & 0xFF) as f32 / 255.0,
        ((rgba >> 16) & 0xFF) as f32 / 255.0,
        ((rgba >> 8) & 0xFF) as f32 / 255.0,
        (rgba & 0xFF) as f32 / 255.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mtd1_format::Command32;

    #[test]
    fn default_rendering_is_msdf() {
        let mut doc = Mtd1Document::new();
        // Style with font_index=1 (MSDF)
        doc.styles.push(BankedStyle::with_font(0xFFFFFFFF, 0, 0, 0, 1));
        doc.instructions.push(Command32::set_style(0));
        doc.instructions.push(Command32::set_cursor(10, 20));
        doc.instructions.push(Command32::draw_glyph(8, 65)); // 'A'
        doc.instructions.push(Command32::draw_glyph(8, 66)); // 'B'

        let gid_map = std::collections::HashMap::from([(65u16, 0u16), (66, 1)]);
        let adv_map = std::collections::HashMap::from([(65u16, 0.5f32), (66, 0.5)]);
        let frame = mtd1_to_sdf(&doc, &gid_map, &adv_map, 16.0, 4.0);

        assert!(!frame.draws.is_empty());
        // Should be MSDF, not bitmap
        assert!(
            frame.draws.iter().all(|d| d.draw_type() == DRAW_TYPE_MSDF_TEXT || d.draw_type() == DRAW_TYPE_BOX),
            "default text should use MSDF (type 8), not bitmap (type 4)"
        );
    }

    #[test]
    fn pictographic_uses_bitmap() {
        let mut doc = Mtd1Document::new();
        // Style with font_index=255 (pictographic/bitmap)
        doc.styles.push(BankedStyle::with_font(0xFFFFFFFF, 0, 0, 0, FONT_INDEX_PICTOGRAPHIC));
        doc.instructions.push(Command32::set_style(0));
        doc.instructions.push(Command32::set_cursor(10, 20));
        doc.instructions.push(Command32::draw_glyph(16, 0xFFFE)); // emoji glyph

        let gid_map = std::collections::HashMap::new();
        let adv_map = std::collections::HashMap::new();
        let frame = mtd1_to_sdf(&doc, &gid_map, &adv_map, 16.0, 4.0);

        let has_bitmap = frame.draws.iter().any(|d| d.draw_type() == DRAW_TYPE_TEXT);
        assert!(has_bitmap, "pictographic glyphs should use bitmap (type 4)");
    }

    #[test]
    fn mixed_msdf_and_pictographic() {
        let mut doc = Mtd1Document::new();
        doc.styles.push(BankedStyle::with_font(0xFFFFFFFF, 0, 0, 0, 1)); // MSDF
        doc.styles.push(BankedStyle::with_font(0xFFFFFFFF, 0, 0, 0, FONT_INDEX_PICTOGRAPHIC)); // bitmap

        // MSDF text
        doc.instructions.push(Command32::set_style(0));
        doc.instructions.push(Command32::set_cursor(10, 20));
        doc.instructions.push(Command32::draw_glyph(8, 72)); // 'H'

        // Switch to pictographic
        doc.instructions.push(Command32::set_style(1));
        doc.instructions.push(Command32::draw_glyph(16, 0xFFFE));

        let gid_map = std::collections::HashMap::from([(72u16, 0u16)]);
        let adv_map = std::collections::HashMap::from([(72u16, 0.5f32)]);
        let frame = mtd1_to_sdf(&doc, &gid_map, &adv_map, 16.0, 4.0);

        let msdf_count = frame.draws.iter().filter(|d| d.draw_type() == DRAW_TYPE_MSDF_TEXT).count();
        let bitmap_count = frame.draws.iter().filter(|d| d.draw_type() == DRAW_TYPE_TEXT).count();
        assert_eq!(msdf_count, 1, "should have 1 MSDF draw");
        assert_eq!(bitmap_count, 1, "should have 1 bitmap draw for pictographic");
    }
}
