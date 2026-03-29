//! Convert mtd1 `Command32` instruction streams to `SdfDrawCmd` for GPU rendering.
//!
//! Walks the bytecode, tracks cursor/style state, and emits GPU-uploadable
//! `SdfDrawCmd` entries with packed text references into a char_buffer.

use matterstream_common::sdf::{SdfDrawCmd, DRAW_TYPE_BOX, DRAW_TYPE_TEXT, DRAW_TYPE_MSDF_TEXT};

use crate::mtd1_format::{BankedStyle, Mtd1Document, opcode};

/// Result of converting an mtd1 document to GPU-renderable SdfDrawCmd list.
pub struct SdfFrame {
    pub draws: Vec<SdfDrawCmd>,
    pub char_buffer: Vec<u32>,
}

/// Convert an `Mtd1Document` to a GPU-renderable `SdfFrame`.
///
/// Text glyphs are batched: consecutive DRAW_GLYPH instructions at the same
/// Y position are merged into a single Text SdfDrawCmd with packed char_buffer
/// references.
pub fn mtd1_to_sdf(doc: &Mtd1Document) -> SdfFrame {
    let mut draws = Vec::new();
    let mut char_buffer: Vec<u32> = Vec::new();

    let mut cursor_x: f32 = 0.0;
    let mut cursor_y: f32 = 0.0;
    let mut current_color: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

    // Text batching state
    let mut text_batch_start_x: f32 = 0.0;
    let mut text_batch_y: f32 = 0.0;
    let mut text_batch_char_offset: u32 = 0;
    let mut text_batch_char_count: u32 = 0;
    let mut text_batch_color: [f32; 4] = [1.0; 4];
    let mut in_text_batch = false;

    let flush_text = |draws: &mut Vec<SdfDrawCmd>,
                      start_x: f32,
                      y: f32,
                      char_offset: u32,
                      char_count: u32,
                      color: [f32; 4],
                      glyph_h: f32| {
        if char_count == 0 {
            return;
        }
        let packed_slot = (char_offset << 16) | (char_count & 0xFFFF);
        let total_width = char_count as f32 * 8.0; // approximate, shader uses its own advance
        draws.push(SdfDrawCmd {
            pos: [start_x, y],
            size: [total_width, glyph_h],
            color,
            params: [DRAW_TYPE_TEXT, 0.0, 0.0, f32::from_bits(packed_slot)],
        });
    };

    for cmd in &doc.instructions {
        match cmd.opcode() {
            opcode::OP_SET_CURSOR => {
                // Flush any pending text batch
                if in_text_batch {
                    flush_text(
                        &mut draws,
                        text_batch_start_x,
                        text_batch_y,
                        text_batch_char_offset,
                        text_batch_char_count,
                        text_batch_color,
                        12.0,
                    );
                    in_text_batch = false;
                }

                let (y, x) = cmd.decode_cursor();
                cursor_x = x as f32;
                cursor_y = y as f32;
            }

            opcode::OP_SET_STYLE => {
                // Flush text on style change
                if in_text_batch {
                    flush_text(
                        &mut draws,
                        text_batch_start_x,
                        text_batch_y,
                        text_batch_char_offset,
                        text_batch_char_count,
                        text_batch_color,
                        12.0,
                    );
                    in_text_batch = false;
                }

                let idx = cmd.decode_style() as usize;
                if idx < doc.styles.len() {
                    current_color = banked_style_to_color(&doc.styles[idx]);
                }
            }

            opcode::OP_DRAW_GLYPH => {
                let (advance, glyph_id) = cmd.decode_glyph();

                if !in_text_batch {
                    // Start a new text batch
                    text_batch_start_x = cursor_x;
                    text_batch_y = cursor_y;
                    text_batch_char_offset = char_buffer.len() as u32;
                    text_batch_char_count = 0;
                    text_batch_color = current_color;
                    in_text_batch = true;
                }

                char_buffer.push(glyph_id as u32);
                text_batch_char_count += 1;
                cursor_x += advance as f32;
            }

            opcode::OP_DRAW_SHAPE => {
                // Flush text before shape
                if in_text_batch {
                    flush_text(
                        &mut draws,
                        text_batch_start_x,
                        text_batch_y,
                        text_batch_char_offset,
                        text_batch_char_count,
                        text_batch_color,
                        12.0,
                    );
                    in_text_batch = false;
                }

                let (h, w) = cmd.decode_shape();
                draws.push(SdfDrawCmd {
                    pos: [cursor_x, cursor_y],
                    size: [w as f32, h as f32],
                    color: current_color,
                    params: [DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
                });
            }

            opcode::OP_SET_TOKEN => {
                // Metadata only — no GPU output
            }

            _ => {}
        }
    }

    // Flush final text batch
    if in_text_batch {
        flush_text(
            &mut draws,
            text_batch_start_x,
            text_batch_y,
            text_batch_char_offset,
            text_batch_char_count,
            text_batch_color,
            12.0,
        );
    }

    SdfFrame { draws, char_buffer }
}

/// Convert an `Mtd1Document` to GPU-renderable `SdfFrame` using MSDF text.
///
/// Instead of bitmap text (type 4), this emits MSDF text draws (type 8).
/// The `glyph_id_to_table_index` maps font glyph IDs to indices in the
/// GPU glyph_table (populated from `FontAtlas.glyphs`).
///
/// `char_buffer` entries for MSDF are packed as: `[16b glyph_table_index | 16b x_advance_px_fixed]`
/// where x_advance_px_fixed is in 4.12 fixed-point (value * 16).
pub fn mtd1_to_sdf_msdf(
    doc: &Mtd1Document,
    glyph_id_to_table_index: &std::collections::HashMap<u16, u16>,
    font_size: f32,
    px_range: f32,
) -> SdfFrame {
    let mut draws = Vec::new();
    let mut char_buffer: Vec<u32> = Vec::new();

    let mut cursor_x: f32 = 0.0;
    let mut cursor_y: f32 = 0.0;
    let mut current_color: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    let mut current_font_index: u8 = 0;

    // MSDF text batching state
    let mut batch_start_x: f32 = 0.0;
    let mut batch_y: f32 = 0.0;
    let mut batch_char_offset: u32 = 0;
    let mut batch_char_count: u32 = 0;
    let mut batch_color: [f32; 4] = [1.0; 4];
    let mut in_batch = false;

    let flush = |draws: &mut Vec<SdfDrawCmd>,
                 start_x: f32,
                 y: f32,
                 char_offset: u32,
                 char_count: u32,
                 color: [f32; 4],
                 font_size: f32,
                 px_range: f32,
                 is_msdf: bool| {
        if char_count == 0 {
            return;
        }
        let packed_slot = (char_offset << 16) | (char_count & 0xFFFF);
        let total_width = char_count as f32 * font_size * 0.6;
        if is_msdf {
            draws.push(SdfDrawCmd {
                pos: [start_x, y],
                size: [total_width, font_size],
                color,
                params: [DRAW_TYPE_MSDF_TEXT, px_range, 0.0, f32::from_bits(packed_slot)],
            });
        } else {
            draws.push(SdfDrawCmd {
                pos: [start_x, y],
                size: [total_width, font_size],
                color,
                params: [DRAW_TYPE_TEXT, 0.0, 0.0, f32::from_bits(packed_slot)],
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
                        batch_color, font_size, px_range, current_font_index > 0,
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
                        batch_color, font_size, px_range, current_font_index > 0,
                    );
                    in_batch = false;
                }
                let idx = cmd.decode_style() as usize;
                if idx < doc.styles.len() {
                    current_color = banked_style_to_color(&doc.styles[idx]);
                    current_font_index = doc.styles[idx].font_index();
                }
            }

            opcode::OP_DRAW_GLYPH => {
                let (advance, glyph_id) = cmd.decode_glyph();

                if !in_batch {
                    batch_start_x = cursor_x;
                    batch_y = cursor_y;
                    batch_char_offset = char_buffer.len() as u32;
                    batch_char_count = 0;
                    batch_color = current_color;
                    in_batch = true;
                }

                if current_font_index > 0 {
                    // MSDF path: pack [glyph_table_index << 16 | advance_fixed]
                    let gt_idx = glyph_id_to_table_index
                        .get(&glyph_id)
                        .copied()
                        .unwrap_or(0);
                    let advance_fixed = ((advance as f32) * 16.0) as u32 & 0xFFFF;
                    char_buffer.push((gt_idx as u32) << 16 | advance_fixed);
                } else {
                    // Bitmap path: store glyph_id as codepoint
                    char_buffer.push(glyph_id as u32);
                }
                batch_char_count += 1;
                cursor_x += advance as f32;
            }

            opcode::OP_DRAW_SHAPE => {
                if in_batch {
                    flush(
                        &mut draws, batch_start_x, batch_y,
                        batch_char_offset, batch_char_count,
                        batch_color, font_size, px_range, current_font_index > 0,
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
            batch_color, font_size, px_range, current_font_index > 0,
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

/// Generate a minimal 5x7 bitmap font for ASCII 0x20..0x7E.
/// Returns (bitmap as packed u32 rows, GpuFont params).
pub fn generate_mini_font() -> (Vec<u32>, [u32; 4]) {
    let glyph_w: u32 = 5;
    let glyph_h: u32 = 7;
    let first_cp: u32 = 0x20;
    let last_cp: u32 = 0x7E;
    let num_glyphs = (last_cp - first_cp + 1) as usize;

    // Each glyph is glyph_h rows of u32, where each u32 has bit patterns
    // for a 5-wide glyph. We'll generate simple recognizable patterns.
    let mut bitmap = vec![0u32; num_glyphs * glyph_h as usize];

    // Helper to set a glyph's rows
    let set_glyph = |bitmap: &mut [u32], cp: u32, rows: &[u8; 7]| {
        let idx = (cp - first_cp) as usize;
        let base = idx * glyph_h as usize;
        for (r, &row) in rows.iter().enumerate() {
            bitmap[base + r] = row as u32;
        }
    };

    // Space
    set_glyph(&mut bitmap, b' ' as u32, &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // Letters A-Z
    let letters: &[(u8, [u8; 7])] = &[
        (b'A', [0x04, 0x0A, 0x11, 0x1F, 0x11, 0x11, 0x00]),
        (b'B', [0x1E, 0x11, 0x1E, 0x11, 0x11, 0x1E, 0x00]),
        (b'C', [0x0E, 0x11, 0x10, 0x10, 0x11, 0x0E, 0x00]),
        (b'D', [0x1E, 0x11, 0x11, 0x11, 0x11, 0x1E, 0x00]),
        (b'E', [0x1F, 0x10, 0x1E, 0x10, 0x10, 0x1F, 0x00]),
        (b'F', [0x1F, 0x10, 0x1E, 0x10, 0x10, 0x10, 0x00]),
        (b'G', [0x0E, 0x11, 0x10, 0x17, 0x11, 0x0E, 0x00]),
        (b'H', [0x11, 0x11, 0x1F, 0x11, 0x11, 0x11, 0x00]),
        (b'I', [0x0E, 0x04, 0x04, 0x04, 0x04, 0x0E, 0x00]),
        (b'J', [0x07, 0x02, 0x02, 0x02, 0x12, 0x0C, 0x00]),
        (b'K', [0x11, 0x12, 0x1C, 0x12, 0x11, 0x11, 0x00]),
        (b'L', [0x10, 0x10, 0x10, 0x10, 0x10, 0x1F, 0x00]),
        (b'M', [0x11, 0x1B, 0x15, 0x11, 0x11, 0x11, 0x00]),
        (b'N', [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x00]),
        (b'O', [0x0E, 0x11, 0x11, 0x11, 0x11, 0x0E, 0x00]),
        (b'P', [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x00]),
        (b'Q', [0x0E, 0x11, 0x11, 0x15, 0x12, 0x0D, 0x00]),
        (b'R', [0x1E, 0x11, 0x11, 0x1E, 0x12, 0x11, 0x00]),
        (b'S', [0x0E, 0x10, 0x0E, 0x01, 0x11, 0x0E, 0x00]),
        (b'T', [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x00]),
        (b'U', [0x11, 0x11, 0x11, 0x11, 0x11, 0x0E, 0x00]),
        (b'V', [0x11, 0x11, 0x11, 0x11, 0x0A, 0x04, 0x00]),
        (b'W', [0x11, 0x11, 0x11, 0x15, 0x1B, 0x11, 0x00]),
        (b'X', [0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11, 0x00]),
        (b'Y', [0x11, 0x0A, 0x04, 0x04, 0x04, 0x04, 0x00]),
        (b'Z', [0x1F, 0x02, 0x04, 0x08, 0x10, 0x1F, 0x00]),
    ];
    for &(ch, ref rows) in letters {
        set_glyph(&mut bitmap, ch as u32, rows);
    }

    // Lowercase a-z (simple versions)
    let lowercase: &[(u8, [u8; 7])] = &[
        (b'a', [0x00, 0x00, 0x0E, 0x01, 0x0F, 0x0E, 0x00]),
        (b'b', [0x10, 0x10, 0x1E, 0x11, 0x11, 0x1E, 0x00]),
        (b'c', [0x00, 0x00, 0x0E, 0x10, 0x10, 0x0E, 0x00]),
        (b'd', [0x01, 0x01, 0x0F, 0x11, 0x11, 0x0F, 0x00]),
        (b'e', [0x00, 0x00, 0x0E, 0x1F, 0x10, 0x0E, 0x00]),
        (b'f', [0x06, 0x08, 0x1C, 0x08, 0x08, 0x08, 0x00]),
        (b'g', [0x00, 0x00, 0x0F, 0x11, 0x0F, 0x01, 0x0E]),
        (b'h', [0x10, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x00]),
        (b'i', [0x04, 0x00, 0x0C, 0x04, 0x04, 0x0E, 0x00]),
        (b'j', [0x02, 0x00, 0x02, 0x02, 0x02, 0x12, 0x0C]),
        (b'k', [0x10, 0x10, 0x12, 0x1C, 0x12, 0x11, 0x00]),
        (b'l', [0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E, 0x00]),
        (b'm', [0x00, 0x00, 0x1A, 0x15, 0x15, 0x11, 0x00]),
        (b'n', [0x00, 0x00, 0x1E, 0x11, 0x11, 0x11, 0x00]),
        (b'o', [0x00, 0x00, 0x0E, 0x11, 0x11, 0x0E, 0x00]),
        (b'p', [0x00, 0x00, 0x1E, 0x11, 0x1E, 0x10, 0x10]),
        (b'q', [0x00, 0x00, 0x0F, 0x11, 0x0F, 0x01, 0x01]),
        (b'r', [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x00]),
        (b's', [0x00, 0x00, 0x0F, 0x10, 0x0E, 0x1E, 0x00]),
        (b't', [0x08, 0x08, 0x1C, 0x08, 0x08, 0x06, 0x00]),
        (b'u', [0x00, 0x00, 0x11, 0x11, 0x11, 0x0F, 0x00]),
        (b'v', [0x00, 0x00, 0x11, 0x11, 0x0A, 0x04, 0x00]),
        (b'w', [0x00, 0x00, 0x11, 0x15, 0x15, 0x0A, 0x00]),
        (b'x', [0x00, 0x00, 0x11, 0x0A, 0x0A, 0x11, 0x00]),
        (b'y', [0x00, 0x00, 0x11, 0x0A, 0x04, 0x08, 0x10]),
        (b'z', [0x00, 0x00, 0x1F, 0x02, 0x04, 0x1F, 0x00]),
    ];
    for &(ch, ref rows) in lowercase {
        set_glyph(&mut bitmap, ch as u32, rows);
    }

    // Digits 0-9
    let digits: &[(u8, [u8; 7])] = &[
        (b'0', [0x0E, 0x13, 0x15, 0x19, 0x11, 0x0E, 0x00]),
        (b'1', [0x04, 0x0C, 0x04, 0x04, 0x04, 0x0E, 0x00]),
        (b'2', [0x0E, 0x11, 0x02, 0x04, 0x08, 0x1F, 0x00]),
        (b'3', [0x0E, 0x11, 0x06, 0x01, 0x11, 0x0E, 0x00]),
        (b'4', [0x02, 0x06, 0x0A, 0x1F, 0x02, 0x02, 0x00]),
        (b'5', [0x1F, 0x10, 0x1E, 0x01, 0x11, 0x0E, 0x00]),
        (b'6', [0x06, 0x08, 0x1E, 0x11, 0x11, 0x0E, 0x00]),
        (b'7', [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x00]),
        (b'8', [0x0E, 0x11, 0x0E, 0x11, 0x11, 0x0E, 0x00]),
        (b'9', [0x0E, 0x11, 0x0F, 0x01, 0x02, 0x0C, 0x00]),
    ];
    for &(ch, ref rows) in digits {
        set_glyph(&mut bitmap, ch as u32, rows);
    }

    // Common punctuation
    let punct: &[(u8, [u8; 7])] = &[
        (b'.', [0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00]),
        (b',', [0x00, 0x00, 0x00, 0x00, 0x04, 0x04, 0x08]),
        (b'!', [0x04, 0x04, 0x04, 0x04, 0x00, 0x04, 0x00]),
        (b'?', [0x0E, 0x11, 0x02, 0x04, 0x00, 0x04, 0x00]),
        (b'+', [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00]),
        (b'-', [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00]),
        (b'$', [0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04]),
        (b'%', [0x19, 0x1A, 0x02, 0x04, 0x0B, 0x13, 0x00]),
        (b':', [0x00, 0x04, 0x00, 0x00, 0x04, 0x00, 0x00]),
    ];
    for &(ch, ref rows) in punct {
        set_glyph(&mut bitmap, ch as u32, rows);
    }

    (bitmap, [glyph_w, glyph_h, first_cp, last_cp])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pretext_rs::FontMetrics;
    use crate::tsx_to_mtd1::{TsxNode, compile_tsx};

    #[test]
    fn mtd1_to_sdf_produces_draws() {
        let fm = FontMetrics::monospace(8, 16);
        let tree = vec![TsxNode::TufteCard {
            x: 20,
            y: 10,
            width: 500,
            children: vec![
                TsxNode::Story {
                    text: "Hello World".into(),
                    token: None,
                },
                TsxNode::Path {
                    segments: vec![(2, 40), (4, 40)],
                },
            ],
        }];
        let doc = compile_tsx(&tree, &fm);
        let frame = mtd1_to_sdf(&doc);

        assert!(!frame.draws.is_empty(), "should produce SdfDrawCmds");
        assert!(!frame.char_buffer.is_empty(), "should have chars for text");

        // Check we have text and box draw types
        let has_text = frame.draws.iter().any(|d| d.draw_type() == DRAW_TYPE_TEXT);
        let has_box = frame.draws.iter().any(|d| d.draw_type() == DRAW_TYPE_BOX);
        assert!(has_text, "should have text draws");
        assert!(has_box, "should have box draws for shapes");
    }

    #[test]
    fn mini_font_generates_bitmap() {
        let (bitmap, font_params) = generate_mini_font();
        assert_eq!(font_params[0], 5); // glyph_w
        assert_eq!(font_params[1], 7); // glyph_h
        assert_eq!(font_params[2], 0x20); // first_cp
        assert_eq!(font_params[3], 0x7E); // last_cp

        let num_glyphs = (font_params[3] - font_params[2] + 1) as usize;
        assert_eq!(bitmap.len(), num_glyphs * 7);

        // 'A' (0x41) should have non-zero rows
        let a_idx = (0x41 - 0x20) as usize;
        let a_rows = &bitmap[a_idx * 7..(a_idx + 1) * 7];
        assert!(a_rows.iter().any(|&r| r != 0), "'A' should have non-zero bitmap rows");
    }
}
