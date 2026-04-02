//! CPU software rasterizer for MatterStream SDF draw commands.
//!
//! Evaluates SdfDrawCmd list into an RGBA framebuffer on the CPU.
//! The GPU only needs to blit the resulting texture — a single
//! fullscreen quad with a texture sample. No storage buffers,
//! no complex shaders, works on any GPU including Mali-G31.
//!
//! Supports: Box, Slab (rounded rect), Circle, Line, bitmap Text.
//! MSDF text falls back to bitmap rendering.

use matterstream_common::{
    SdfDrawCmd, Anim, GpuFont, RenderFrame,
    DRAW_TYPE_BOX, DRAW_TYPE_SLAB, DRAW_TYPE_CIRCLE, DRAW_TYPE_LINE,
    DRAW_TYPE_TEXT, DRAW_TYPE_RIBBON_BEGIN, DRAW_TYPE_RIBBON_END,
    sd_rounded_box, sd_box, sd_circle, sd_segment,
};

/// RGBA framebuffer produced by the CPU rasterizer.
pub struct Framebuffer {
    pub width: u32,
    pub height: u32,
    /// RGBA8 pixel data, row-major, top-to-bottom. Length = width * height * 4.
    pub pixels: Vec<u8>,
}

impl Framebuffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0u8; (width * height * 4) as usize],
        }
    }

    fn set_pixel(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8, a: u8) {
        let idx = ((y * self.width + x) * 4) as usize;
        self.pixels[idx] = r;
        self.pixels[idx + 1] = g;
        self.pixels[idx + 2] = b;
        self.pixels[idx + 3] = a;
    }

    fn blend_pixel(&mut self, x: u32, y: u32, sr: f32, sg: f32, sb: f32, sa: f32) {
        if sa < 0.001 { return; }
        let idx = ((y * self.width + x) * 4) as usize;
        let dr = self.pixels[idx] as f32 / 255.0;
        let dg = self.pixels[idx + 1] as f32 / 255.0;
        let db = self.pixels[idx + 2] as f32 / 255.0;
        let da = self.pixels[idx + 3] as f32 / 255.0;
        let out_a = sa + da * (1.0 - sa);
        if out_a < 0.001 { return; }
        let out_r = (sr * sa + dr * da * (1.0 - sa)) / out_a;
        let out_g = (sg * sa + dg * da * (1.0 - sa)) / out_a;
        let out_b = (sb * sa + db * da * (1.0 - sa)) / out_a;
        self.pixels[idx] = (out_r * 255.0).min(255.0) as u8;
        self.pixels[idx + 1] = (out_g * 255.0).min(255.0) as u8;
        self.pixels[idx + 2] = (out_b * 255.0).min(255.0) as u8;
        self.pixels[idx + 3] = (out_a * 255.0).min(255.0) as u8;
    }
}

/// Rasterize a RenderFrame into a framebuffer.
///
/// Unlike the GPU shader which evaluates all commands per pixel,
/// this iterates commands and rasterizes each into its bounding box.
pub fn rasterize_frame(frame: &RenderFrame, dark_theme: bool) -> Framebuffer {
    let w = (frame.width as f32 / frame.scale).ceil() as u32;
    let h = (frame.height as f32 / frame.scale).ceil() as u32;
    let mut fb = Framebuffer::new(w, h);

    // Fill background
    let bg = if dark_theme { (14u8, 14, 22) } else { (242, 242, 245) };
    for y in 0..h {
        for x in 0..w {
            fb.set_pixel(x, y, bg.0, bg.1, bg.2, 255);
        }
    }

    // Track ribbon clipping state
    let mut ribbon_clip: Option<([f32; 2], [f32; 2])> = None;
    let mut ribbon_scroll = [0.0f32; 2];

    for cmd in &frame.draws {
        let ty = cmd.params[0] as u32;

        // Ribbon begin/end
        if ty == DRAW_TYPE_RIBBON_BEGIN as u32 {
            ribbon_clip = Some((cmd.pos, [cmd.pos[0] + cmd.size[0], cmd.pos[1] + cmd.size[1]]));
            let slot = cmd.params[1] as u32 as usize;
            let scroll_val = if slot < 16 { frame.scalar_bank[slot] } else { 0.0 };
            let dir = cmd.params[2];
            ribbon_scroll = if dir > 0.5 { [0.0, scroll_val] } else { [scroll_val, 0.0] };
            continue;
        }
        if ty == DRAW_TYPE_RIBBON_END as u32 {
            ribbon_clip = None;
            ribbon_scroll = [0.0; 2];
            continue;
        }

        match ty {
            0 | 1 | 2 | 3 => rasterize_sdf_cmd(&mut fb, cmd, &ribbon_clip, &ribbon_scroll, &frame.anim_bank, &frame.int_bank, frame.time_ms),
            4 => rasterize_text_cmd(&mut fb, cmd, &frame.char_buffer, &frame.glyph_bitmap, &frame.font, &ribbon_clip, &ribbon_scroll),
            8 => rasterize_text_cmd(&mut fb, cmd, &frame.char_buffer, &frame.glyph_bitmap, &frame.font, &ribbon_clip, &ribbon_scroll), // MSDF fallback to bitmap
            _ => {} // textures, etc. — skip for now
        }
    }

    fb
}

fn rasterize_sdf_cmd(
    fb: &mut Framebuffer,
    cmd: &SdfDrawCmd,
    ribbon_clip: &Option<([f32; 2], [f32; 2])>,
    ribbon_scroll: &[f32; 2],
    anim_bank: &[Anim],
    int_bank: &[i32; 16],
    time_ms: f32,
) {
    let ty = cmd.params[0] as u32;

    // Animation alpha
    let anim_alpha = eval_anim(cmd.params[2] as u32, anim_bank, int_bank, time_ms);
    if anim_alpha < 0.01 { return; }

    // Bounding box with margin for shadows + antialiasing
    let margin = 6.0;
    let x0 = (cmd.pos[0] - margin).max(0.0) as u32;
    let y0 = (cmd.pos[1] - margin).max(0.0) as u32;
    let x1 = ((cmd.pos[0] + cmd.size[0] + margin).ceil() as u32).min(fb.width);
    let y1 = ((cmd.pos[1] + cmd.size[1] + margin).ceil() as u32).min(fb.height);

    let center_x = cmd.pos[0] + cmd.size[0] * 0.5;
    let center_y = cmd.pos[1] + cmd.size[1] * 0.5;
    let half_w = cmd.size[0] * 0.5;
    let half_h = cmd.size[1] * 0.5;
    let radius = cmd.params[1];

    for y in y0..y1 {
        for x in x0..x1 {
            let mut px = x as f32 + 0.5;
            let mut py = y as f32 + 0.5;

            // Ribbon clipping
            if let Some((clip_min, clip_max)) = ribbon_clip {
                if px < clip_min[0] || px >= clip_max[0] || py < clip_min[1] || py >= clip_max[1] {
                    continue;
                }
                px -= ribbon_scroll[0];
                py -= ribbon_scroll[1];
            }

            let lx = px - center_x;
            let ly = py - center_y;

            let d = match ty {
                0 => sd_box(lx, ly, half_w, half_h),
                1 => sd_rounded_box(lx, ly, half_w, half_h, radius),
                2 => sd_circle(lx, ly, radius),
                3 => sd_segment(lx, ly, -half_w, 0.0, half_w, 0.0, half_h),
                _ => 1e6,
            };

            // Shadow (for box/slab/circle)
            if ty <= 2 {
                let shadow_d = match ty {
                    0 => sd_box(lx - 2.0, ly - 3.0, half_w, half_h),
                    1 => sd_rounded_box(lx - 2.0, ly - 3.0, half_w, half_h, radius),
                    2 => sd_circle(lx - 1.5, ly - 2.0, radius),
                    _ => 1e6,
                };
                let shadow_alpha = 0.15 * (1.0 - smoothstep(-1.0, 4.0, shadow_d));
                if shadow_alpha > 0.01 {
                    fb.blend_pixel(x, y, 0.0, 0.0, 0.0, shadow_alpha);
                }
            }

            // SDF fill with 1px antialiasing
            let alpha = (1.0 - smoothstep(-0.5, 0.5, d)) * cmd.color[3] * anim_alpha;
            if alpha > 0.01 {
                fb.blend_pixel(x, y, cmd.color[0], cmd.color[1], cmd.color[2], alpha);
            }
        }
    }
}

fn rasterize_text_cmd(
    fb: &mut Framebuffer,
    cmd: &SdfDrawCmd,
    char_buffer: &[u32],
    glyph_bitmap: &[u32],
    font: &GpuFont,
    ribbon_clip: &Option<([f32; 2], [f32; 2])>,
    ribbon_scroll: &[f32; 2],
) {
    let glyph_w = font.glyph_w;
    let glyph_h = font.glyph_h;
    let first_cp = font.first_cp;
    let last_cp = font.last_cp;
    if glyph_w == 0 || glyph_h == 0 { return; }

    let packed = f32::to_bits(cmd.params[3]);
    let char_offset = (packed >> 16) as usize;
    let char_count = (packed & 0xFFFF) as usize;

    let text_x = cmd.pos[0];
    let text_y = cmd.pos[1];
    let text_size = cmd.size[1];
    let scale_f = (text_size / glyph_h as f32).max(1.0);
    let advance = (glyph_w + 1) as f32 * scale_f;

    for ci in 0..char_count {
        if char_offset + ci >= char_buffer.len() { break; }
        let cp = char_buffer[char_offset + ci];
        let glyph_idx = cp.clamp(first_cp, last_cp) - first_cp;

        let char_x = text_x + ci as f32 * advance;
        let gw = (glyph_w as f32 * scale_f).ceil() as u32;
        let gh = (glyph_h as f32 * scale_f).ceil() as u32;

        let x0 = char_x.max(0.0) as u32;
        let y0 = text_y.max(0.0) as u32;
        let x1 = (x0 + gw).min(fb.width);
        let y1 = (y0 + gh).min(fb.height);

        for y in y0..y1 {
            for x in x0..x1 {
                let mut px = x as f32;
                let mut py = y as f32;

                if let Some((clip_min, clip_max)) = ribbon_clip {
                    if px < clip_min[0] || px >= clip_max[0] || py < clip_min[1] || py >= clip_max[1] {
                        continue;
                    }
                    px -= ribbon_scroll[0];
                    py -= ribbon_scroll[1];
                }

                let local_x = px - char_x;
                let local_y = py - text_y;
                if local_x < 0.0 || local_y < 0.0 { continue; }

                let gx = (local_x / scale_f) as u32;
                let gy = (local_y / scale_f) as u32;
                if gx >= glyph_w || gy >= glyph_h { continue; }

                let bitmap_idx = (glyph_idx * glyph_h + gy) as usize;
                if bitmap_idx >= glyph_bitmap.len() { continue; }
                let row = glyph_bitmap[bitmap_idx];
                let bit = glyph_w - 1 - gx;
                if (row & (1 << bit)) != 0 {
                    fb.blend_pixel(x, y, cmd.color[0], cmd.color[1], cmd.color[2], cmd.color[3]);
                }
            }
        }
    }
}

fn eval_anim(anim_idx: u32, anim_bank: &[Anim], int_bank: &[i32; 16], time_ms: f32) -> f32 {
    if anim_idx == 0 { return 1.0; }
    let idx = (anim_idx - 1) as usize;
    if idx >= anim_bank.len() { return 1.0; }
    let a = &anim_bank[idx];

    // Check enable ref
    if a.enable_ref != 0 {
        let slot = (a.enable_ref & 0xFFFF) as usize;
        if slot < 16 && int_bank[slot] == 0 {
            return 0.0;
        }
    }

    if a.freq < 0.001 { return 1.0; }
    let time_s = time_ms / 1000.0;
    let phase = (time_s * a.freq).fract();
    if phase < a.duty { 1.0 } else { 0.0 }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_frame() {
        let frame = RenderFrame {
            draws: vec![],
            char_buffer: vec![],
            anim_bank: vec![],
            texture_bank: vec![],
            font: GpuFont::NONE,
            glyph_bitmap: vec![],
            scalar_bank: [0.0; 16],
            int_bank: [0; 16],
            time_ms: 0.0,
            width: 100,
            height: 100,
            scale: 1.0,
        };
        let fb = rasterize_frame(&frame, true);
        assert_eq!(fb.width, 100);
        assert_eq!(fb.height, 100);
        // Dark theme background: (14, 14, 22)
        assert_eq!(fb.pixels[0], 14);
        assert_eq!(fb.pixels[2], 22);
    }

    #[test]
    fn red_box() {
        let frame = RenderFrame {
            draws: vec![SdfDrawCmd {
                pos: [10.0, 10.0],
                size: [20.0, 20.0],
                color: [1.0, 0.0, 0.0, 1.0],
                params: [DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
            }],
            char_buffer: vec![],
            anim_bank: vec![],
            texture_bank: vec![],
            font: GpuFont::NONE,
            glyph_bitmap: vec![],
            scalar_bank: [0.0; 16],
            int_bank: [0; 16],
            time_ms: 0.0,
            width: 50,
            height: 50,
            scale: 1.0,
        };
        let fb = rasterize_frame(&frame, true);
        // Center pixel (20, 20) should be red
        let idx = (20 * 50 + 20) as usize * 4;
        assert_eq!(fb.pixels[idx], 255); // R
        assert_eq!(fb.pixels[idx + 1], 0); // G
        assert_eq!(fb.pixels[idx + 2], 0); // B
    }
}
