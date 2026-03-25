//! UI draw command dispatch and font glyph layout.
//!
//! Generic over `RenderBackend` — pixel-level drawing is pluggable.
//! The softbuffer implementation lives in `matterstream-ui-soft`.

use matterstream_common::RenderBackend;
use crate::types::UiDrawCmd;

/// Render draw commands using backend `R`, without font support.
pub fn render_ui_draws<R: RenderBackend>(
    draws: &[UiDrawCmd],
    buf: &mut [u32],
    width: u32,
    height: u32,
) {
    render_ui_draws_with_font::<R>(draws, buf, width, height, &[], None);
}

/// Render draw commands using backend `R`, with optional font atlas for text.
pub fn render_ui_draws_with_font<R: RenderBackend>(
    draws: &[UiDrawCmd],
    buf: &mut [u32],
    width: u32,
    height: u32,
    string_table: &[String],
    font: Option<&matterstream_packaging::fnta::FontAtlas>,
) {
    for cmd in draws {
        match cmd {
            UiDrawCmd::Box { x, y, w, h, color } => {
                R::draw_rect(buf, width, height, *x, *y, *w, *h, *color);
            }
            UiDrawCmd::Slab {
                x, y, w, h, radius, color,
            } => {
                R::draw_rounded_rect(buf, width, height, *x, *y, *w, *h, *radius, *color);
            }
            UiDrawCmd::Circle { x, y, r, color } => {
                R::draw_circle(buf, width, height, *x, *y, *r, *color);
            }
            UiDrawCmd::Text {
                x, y, size, slot: _, color,
            } => {
                // Slot-based text — placeholder rectangle
                R::draw_rect(buf, width, height, *x, *y, *size * 4, *size, *color);
            }
            UiDrawCmd::TextStr {
                x, y, size, str_idx, color,
            } => {
                if let (Some(font), Some(text)) =
                    (font, string_table.get(*str_idx as usize))
                {
                    draw_text_glyphs::<R>(buf, width, height, *x, *y, *size, text, *color, font);
                } else {
                    R::draw_rect(buf, width, height, *x, *y, *size * 4, *size, *color);
                }
            }
            UiDrawCmd::Line {
                x1, y1, x2, y2, color,
            } => {
                R::draw_line(buf, width, height, *x1, *y1, *x2, *y2, *color);
            }
            UiDrawCmd::Action { .. } => {
                // Action regions are metadata — not rendered visually.
            }
        }
    }
}

/// Render a text string using bitmap glyphs from a font atlas.
/// Glyph layout stays here; pixel blitting uses `R::blend_pixel`.
#[allow(clippy::too_many_arguments)]
fn draw_text_glyphs<R: RenderBackend>(
    buf: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    size: u32,
    text: &str,
    color: u32,
    font: &matterstream_packaging::fnta::FontAtlas,
) {
    let gw = font.glyph_w as u32;
    let gh = font.glyph_h as u32;
    if gw == 0 || gh == 0 {
        return;
    }
    let scale = (size / gh).max(1);
    let advance = (gw + 1) * scale;

    let mut cursor_x = x;
    for ch in text.bytes() {
        let cp = if ch >= font.first_cp && ch <= font.last_cp {
            ch
        } else {
            b'?'
        };
        if let Some(rows) = font.glyph_rows(cp) {
            for (row_idx, &row_byte) in rows.iter().enumerate() {
                for col in 0..gw {
                    let bit = gw - 1 - col;
                    if row_byte & (1 << bit) != 0 {
                        let px = cursor_x + (col * scale) as i32;
                        let py = y + (row_idx as u32 * scale) as i32;
                        for dy in 0..scale {
                            for dx in 0..scale {
                                let fx = px + dx as i32;
                                let fy = py + dy as i32;
                                if fx >= 0
                                    && fy >= 0
                                    && (fx as u32) < width
                                    && (fy as u32) < height
                                {
                                    let idx = (fy as u32 * width + fx as u32) as usize;
                                    buf[idx] = R::blend_pixel(buf[idx], color);
                                }
                            }
                        }
                    }
                }
            }
        }
        cursor_x += advance as i32;
    }
}
