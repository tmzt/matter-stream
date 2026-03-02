//! UI draw commands, draw state, color helpers, and CPU-side softbuffer rasterizer.
//!
//! See `docs/STACKVM_UI_SPEC.md` for the full specification.

/// Maximum UI state stack depth.
pub const UI_STATE_STACK_MAX: usize = 16;

/// Maximum draw commands per execution.
pub const UI_DRAW_CMD_MAX: usize = 4096;

/// A single UI draw command emitted by the RPN VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiDrawCmd {
    Box {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        color: u32,
    },
    Slab {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        radius: u32,
        color: u32,
    },
    Circle {
        x: i32,
        y: i32,
        r: u32,
        color: u32,
    },
    Text {
        x: i32,
        y: i32,
        size: u32,
        slot: u32,
        color: u32,
    },
    Line {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        color: u32,
    },
}

/// UI draw state: current color and translation offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiDrawState {
    pub color: u32,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl Default for UiDrawState {
    fn default() -> Self {
        Self {
            color: 0xFFFFFFFF, // white, fully opaque
            offset_x: 0,
            offset_y: 0,
        }
    }
}

/// Pack RGBA components into a u32 (0xRRGGBBAA).
pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    (r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | a as u32
}

/// Unpack a u32 RGBA into (r, g, b, a).
pub fn rgba_unpack(c: u32) -> (u8, u8, u8, u8) {
    (
        (c >> 24) as u8,
        (c >> 16) as u8,
        (c >> 8) as u8,
        c as u8,
    )
}

/// Alpha-composite `src_rgba` (0xRRGGBBAA) over `dst` (0x00RRGGBB softbuffer format).
/// Returns the blended pixel in 0x00RRGGBB format.
pub fn blend_pixel(dst: u32, src_rgba: u32) -> u32 {
    let (sr, sg, sb, sa) = rgba_unpack(src_rgba);
    if sa == 0 {
        return dst;
    }
    if sa == 255 {
        return (sr as u32) << 16 | (sg as u32) << 8 | sb as u32;
    }
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

/// Render a list of draw commands into a softbuffer pixel buffer.
pub fn render_ui_draws(draws: &[UiDrawCmd], buf: &mut [u32], width: u32, height: u32) {
    for cmd in draws {
        match cmd {
            UiDrawCmd::Box { x, y, w, h, color } => {
                draw_filled_rect(buf, width, height, *x, *y, *w, *h, *color);
            }
            UiDrawCmd::Slab {
                x,
                y,
                w,
                h,
                radius,
                color,
            } => {
                draw_rounded_rect(buf, width, height, *x, *y, *w, *h, *radius, *color);
            }
            UiDrawCmd::Circle { x, y, r, color } => {
                draw_filled_circle(buf, width, height, *x, *y, *r, *color);
            }
            UiDrawCmd::Text {
                x,
                y,
                size,
                slot: _,
                color,
            } => {
                // Placeholder: draw a colored rectangle for the text area
                draw_filled_rect(buf, width, height, *x, *y, *size * 4, *size, *color);
            }
            UiDrawCmd::Line {
                x1,
                y1,
                x2,
                y2,
                color,
            } => {
                draw_line(buf, width, height, *x1, *y1, *x2, *y2, *color);
            }
        }
    }
}

/// Draw a filled rectangle with alpha blending, bounds-clipped.
#[allow(clippy::too_many_arguments)]
pub fn draw_filled_rect(
    buf: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    color: u32,
) {
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

/// Draw a rounded rectangle with alpha blending.
#[allow(clippy::too_many_arguments)]
pub fn draw_rounded_rect(
    buf: &mut [u32],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    radius: u32,
    color: u32,
) {
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

            // Check if pixel is in a corner region
            let in_corner = {
                let cx = if lx < r as i64 {
                    r as i64 - lx
                } else if lx >= (w - r) as i64 {
                    lx - (w - r - 1) as i64
                } else {
                    0
                };
                let cy = if ly < r as i64 {
                    r as i64 - ly
                } else if ly >= (h - r) as i64 {
                    ly - (h - r - 1) as i64
                } else {
                    0
                };
                if cx > 0 && cy > 0 {
                    cx * cx + cy * cy > r_sq
                } else {
                    false
                }
            };

            if !in_corner {
                let idx = (py * width + px) as usize;
                buf[idx] = blend_pixel(buf[idx], color);
            }
        }
    }
}

/// Draw a filled circle with alpha blending.
pub fn draw_filled_circle(
    buf: &mut [u32],
    width: u32,
    height: u32,
    cx: i32,
    cy: i32,
    r: u32,
    color: u32,
) {
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

/// Draw a line segment using Bresenham's algorithm with alpha blending.
#[allow(clippy::too_many_arguments)]
pub fn draw_line(
    buf: &mut [u32],
    width: u32,
    height: u32,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: u32,
) {
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
        if x == x2 as i64 && y == y2 as i64 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}
