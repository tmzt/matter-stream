//! Softbuffer CPU rasterizer — `Rasterizer` implementation for `&mut [u32]` buffers.
//!
//! Pixel format: `0x00RRGGBB` (softbuffer convention).
//! Input colors: `0xRRGGBBAA` (MatterStream RGBA convention).
//!
//! Usage: `render_ui_draws_with_font::<SoftRenderer>(...)`

use matterstream_common::{Rasterizer, rgba_unpack, SdfDrawCmd, sdf_eval};

/// Softbuffer CPU rasterizer. Zero-sized type — all methods are static.
pub struct SoftRenderer;

impl Rasterizer for SoftRenderer {
    fn blend_pixel(dst: u32, src_rgba: u32) -> u32 {
        blend_pixel(dst, src_rgba)
    }

    fn draw_rect(buf: &mut [u32], width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32) {
        draw_filled_rect(buf, width, height, x, y, w, h, color);
    }

    fn draw_rounded_rect(buf: &mut [u32], width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, radius: u32, color: u32) {
        draw_rounded_rect(buf, width, height, x, y, w, h, radius, color);
    }

    fn draw_circle(buf: &mut [u32], width: u32, height: u32, cx: i32, cy: i32, r: u32, color: u32) {
        draw_filled_circle(buf, width, height, cx, cy, r, color);
    }

    fn draw_line(buf: &mut [u32], width: u32, height: u32, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
        draw_line(buf, width, height, x1, y1, x2, y2, color);
    }
}

// ── Pixel-level drawing primitives ──────────────────────────────────────

/// Alpha-composite `src_rgba` (0xRRGGBBAA) over `dst` (0x00RRGGBB softbuffer format).
pub fn blend_pixel(dst: u32, src_rgba: u32) -> u32 {
    let (sr, sg, sb, sa) = rgba_unpack(src_rgba);
    if sa == 0 { return dst; }
    if sa == 255 { return (sr as u32) << 16 | (sg as u32) << 8 | sb as u32; }
    let dr = (dst >> 16) as u8;
    let dg = (dst >> 8) as u8;
    let db = dst as u8;
    let a = sa as u32;
    let inv_a = 255 - a;
    let r = (sr as u32 * a + dr as u32 * inv_a) / 255;
    let g = (sg as u32 * a + dg as u32 * inv_a) / 255;
    let b = (sb as u32 * a + db as u32 * inv_a) / 255;
    (r << 16) | (g << 8) | b
}

#[allow(clippy::too_many_arguments)]
pub fn draw_filled_rect(buf: &mut [u32], width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32) {
    let x0 = x.max(0) as u32;
    let y0 = y.max(0) as u32;
    let x1 = ((x as i64 + w as i64) as u32).min(width);
    let y1 = ((y as i64 + h as i64) as u32).min(height);
    for py in y0..y1 {
        for px in x0..x1 {
            let idx = (py * width + px) as usize;
            buf[idx] = blend_pixel(buf[idx], color);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_rounded_rect(buf: &mut [u32], width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, radius: u32, color: u32) {
    let x0 = x.max(0) as u32;
    let y0 = y.max(0) as u32;
    let x1 = ((x as i64 + w as i64) as u32).min(width);
    let y1 = ((y as i64 + h as i64) as u32).min(height);
    let r = radius.min(w / 2).min(h / 2);
    let r_sq = (r * r) as i64;
    for py in y0..y1 {
        for px in x0..x1 {
            let lx = (px as i32 - x) as i64;
            let ly = (py as i32 - y) as i64;
            let in_corner = {
                let cx = if lx < r as i64 { r as i64 - lx } else if lx >= (w - r) as i64 { lx - (w - r - 1) as i64 } else { 0 };
                let cy = if ly < r as i64 { r as i64 - ly } else if ly >= (h - r) as i64 { ly - (h - r - 1) as i64 } else { 0 };
                cx > 0 && cy > 0 && cx * cx + cy * cy > r_sq
            };
            if !in_corner {
                let idx = (py * width + px) as usize;
                buf[idx] = blend_pixel(buf[idx], color);
            }
        }
    }
}

pub fn draw_filled_circle(buf: &mut [u32], width: u32, height: u32, cx: i32, cy: i32, r: u32, color: u32) {
    let ri = r as i32;
    let x0 = (cx - ri).max(0) as u32;
    let y0 = (cy - ri).max(0) as u32;
    let x1 = ((cx + ri + 1) as u32).min(width);
    let y1 = ((cy + ri + 1) as u32).min(height);
    let r_sq = (r * r) as i64;
    for py in y0..y1 {
        let dy = py as i64 - cy as i64;
        for px in x0..x1 {
            let dx = px as i64 - cx as i64;
            if dx * dx + dy * dy <= r_sq {
                let idx = (py * width + px) as usize;
                buf[idx] = blend_pixel(buf[idx], color);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_line(buf: &mut [u32], width: u32, height: u32, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
    let mut x = x1 as i64;
    let mut y = y1 as i64;
    let dx = (x2 as i64 - x1 as i64).abs();
    let dy = -(y2 as i64 - y1 as i64).abs();
    let sx: i64 = if x1 < x2 { 1 } else { -1 };
    let sy: i64 = if y1 < y2 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < height {
            let idx = (y as u32 * width + x as u32) as usize;
            buf[idx] = blend_pixel(buf[idx], color);
        }
        if x == x2 as i64 && y == y2 as i64 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x += sx; }
        if e2 <= dx { err += dx; y += sy; }
    }
}

// ── SDF-based rendering (new primary path) ──────────────────────────────

/// Render SdfDrawCmd list to a softbuffer pixel buffer using per-pixel SDF evaluation.
/// Produces pixel-identical output to the GPU fragment shader.
///
/// `buf` is in softbuffer format (0x00RRGGBB), `width` × `height` pixels.
pub fn render_sdf(draws: &[SdfDrawCmd], buf: &mut [u32], width: u32, height: u32) {
    for py in 0..height {
        for px in 0..width {
            let fx = px as f32 + 0.5;
            let fy = py as f32 + 0.5;
            let idx = (py * width + px) as usize;

            // Evaluate each draw command front-to-back, blend over
            for cmd in draws {
                let (d, color) = sdf_eval(cmd, fx, fy);
                if d < 1.0 {
                    // Smoothstep antialiasing at the edge (matching shader)
                    let alpha = if d < 0.0 { color[3] } else { color[3] * (1.0 - d) };
                    if alpha > 0.001 {
                        let src_rgba = f32_color_to_softbuffer(color[0], color[1], color[2], alpha);
                        buf[idx] = blend_pixel(buf[idx], src_rgba);
                    }
                }
            }
        }
    }
}

/// Convert f32 RGBA (0.0-1.0) to u32 0xRRGGBBAA for blend_pixel input.
fn f32_color_to_softbuffer(r: f32, g: f32, b: f32, a: f32) -> u32 {
    let ri = (r * 255.0).clamp(0.0, 255.0) as u32;
    let gi = (g * 255.0).clamp(0.0, 255.0) as u32;
    let bi = (b * 255.0).clamp(0.0, 255.0) as u32;
    let ai = (a * 255.0).clamp(0.0, 255.0) as u32;
    (ri << 24) | (gi << 16) | (bi << 8) | ai
}

#[cfg(test)]
mod tests {
    use super::*;
    use matterstream_common::rgba;

    #[test]
    fn blend_pixel_fully_transparent() {
        let dst = 0x00AABBCC;
        assert_eq!(blend_pixel(dst, rgba(255, 0, 0, 0)), dst);
    }

    #[test]
    fn blend_pixel_fully_opaque() {
        let dst = 0x00AABBCC;
        assert_eq!(blend_pixel(dst, rgba(0x11, 0x22, 0x33, 255)), 0x00112233);
    }

    #[test]
    fn alpha_blending() {
        let dst = 0x00FFFFFF;
        let blended = blend_pixel(dst, rgba(255, 0, 0, 128));
        let r = (blended >> 16) as u8;
        assert!(r > 200);
    }

    #[test]
    fn render_box_pixels() {
        let (w, h) = (4u32, 4u32);
        let mut buf = vec![0u32; (w * h) as usize];
        draw_filled_rect(&mut buf, w, h, 1, 1, 2, 2, rgba(255, 0, 0, 255));
        assert_eq!(buf[(w + 1) as usize], 0x00FF0000);
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn render_circle_pixels() {
        let (w, h) = (11u32, 11u32);
        let mut buf = vec![0u32; (w * h) as usize];
        draw_filled_circle(&mut buf, w, h, 5, 5, 2, rgba(0, 255, 0, 255));
        assert_eq!(buf[(5 * w + 5) as usize], 0x0000FF00);
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn sdf_render_box() {
        let draws = vec![SdfDrawCmd {
            pos: [2.0, 2.0],
            size: [6.0, 6.0],
            color: [1.0, 0.0, 0.0, 1.0],
            params: [matterstream_common::DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
        }];
        let (w, h) = (10u32, 10u32);
        let mut buf = vec![0u32; (w * h) as usize];
        render_sdf(&draws, &mut buf, w, h);
        // Center of the box should be red
        assert_eq!(buf[(5 * w + 5) as usize], 0x00FF0000);
        // Well outside should be black
        assert_eq!(buf[(0 * w + 0) as usize], 0);
        assert_eq!(buf[(9 * w + 9) as usize], 0);
    }

    #[test]
    fn sdf_render_slab() {
        let draws = vec![SdfDrawCmd {
            pos: [0.0, 0.0],
            size: [10.0, 10.0],
            color: [0.0, 1.0, 0.0, 1.0],
            params: [matterstream_common::DRAW_TYPE_SLAB, 2.0, 0.0, 0.0],
        }];
        let (w, h) = (10u32, 10u32);
        let mut buf = vec![0u32; (w * h) as usize];
        render_sdf(&draws, &mut buf, w, h);
        // Center should be green
        assert_eq!(buf[(5 * w + 5) as usize], 0x0000FF00);
    }

    #[test]
    fn sdf_render_multiple() {
        let draws = vec![
            SdfDrawCmd {
                pos: [0.0, 0.0], size: [20.0, 20.0],
                color: [1.0, 0.0, 0.0, 1.0],
                params: [matterstream_common::DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
            },
            SdfDrawCmd {
                pos: [5.0, 5.0], size: [10.0, 10.0],
                color: [0.0, 0.0, 1.0, 1.0],
                params: [matterstream_common::DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
            },
        ];
        let (w, h) = (20u32, 20u32);
        let mut buf = vec![0u32; (w * h) as usize];
        render_sdf(&draws, &mut buf, w, h);
        // Center of inner box should be blue (drawn on top of red)
        assert_eq!(buf[(10 * w + 10) as usize], 0x000000FF);
        // Corner should be red (only outer box)
        assert_eq!(buf[(1 * w + 1) as usize], 0x00FF0000);
    }
}
