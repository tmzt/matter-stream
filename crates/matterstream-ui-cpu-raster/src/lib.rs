//! CPU software rasterizer for MatterStream SDF draw commands.
//!
//! Evaluates SdfDrawCmd list into an RGBA framebuffer on the CPU.
//! The GPU only needs to blit the resulting texture — a single
//! fullscreen quad with a texture sample. No storage buffers,
//! no complex shaders, works on any GPU including Mali-G31.
//!
//! Supports: Box, Slab, Circle, Line, bitmap Text, MSDF Text (via BitmapAtlas).

use matterstream_common::{
    SdfDrawCmd, Anim, GpuFont, RenderFrame,
    DRAW_TYPE_BOX, DRAW_TYPE_SLAB, DRAW_TYPE_CIRCLE, DRAW_TYPE_LINE,
    DRAW_TYPE_TEXT, DRAW_TYPE_RIBBON_BEGIN, DRAW_TYPE_RIBBON_END,
    sd_rounded_box, sd_box, sd_circle, sd_segment,
};

// ── Bitmap atlas (pre-rasterized from MSDF) ─────────────────────────────

/// Pre-rasterized alpha atlas for CPU text rendering.
/// Same glyph layout and glyph_table as the MSDF atlas, but each pixel
/// is a single alpha byte instead of RGB distance channels.
pub struct BitmapAtlas {
    pub width: u32,
    pub height: u32,
    /// Row-major alpha pixels, length = width * height.
    pub alpha: Vec<u8>,
    /// Glyph table: pairs of vec4<u32> per entry, same format as GPU.
    /// g0 = [glyph_id, atlas_xy_packed, atlas_wh_packed, advance_x_bits]
    /// g1 = [proj_sx_bits, proj_sy_bits, proj_tx_bits, proj_ty_bits]
    pub glyph_table: Vec<u32>,
}

impl BitmapAtlas {
    /// Load a pre-baked bitmap atlas (mca1 format: magic + width + height + alpha bytes).
    /// Glyph table comes from the metrics file (same as MSDF).
    pub fn from_baked(data: &[u8], glyph_table: &[u32]) -> Option<Self> {
        if data.len() < 12 || &data[0..4] != b"mca1" { return None; }
        let width = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let height = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let alpha = data[12..].to_vec();
        if alpha.len() != (width * height) as usize { return None; }
        Some(Self { width, height, alpha, glyph_table: glyph_table.to_vec() })
    }

    /// Convert an MSDF RGBA atlas to a bitmap alpha atlas.
    ///
    /// Takes the MSDF atlas pixels (RGBA8, row-major) and evaluates
    /// `median(R,G,B)` → threshold at 0.5 → alpha byte.
    /// The result uses the same pixel coordinates, so glyph_table
    /// atlas_xy/atlas_wh references remain valid.
    pub fn from_msdf(rgba: &[u8], width: u32, height: u32, glyph_table: &[u32], px_range: f32) -> Self {
        let n = (width * height) as usize;
        let mut alpha = vec![0u8; n];
        // Use a wider range for softer edges — preserves thin strokes
        // at small font sizes where each screen pixel spans multiple atlas texels.
        let range = (px_range * 0.5).max(1.0);

        for i in 0..n {
            let base = i * 4;
            if base + 2 >= rgba.len() { break; }
            let r = rgba[base] as f32 / 255.0;
            let g = rgba[base + 1] as f32 / 255.0;
            let b = rgba[base + 2] as f32 / 255.0;
            // MSDF median
            let sd = f32::max(f32::min(r, g), f32::min(f32::max(r, g), b));
            // Smooth threshold — gentler curve keeps thin strokes visible
            let a = (range * (sd - 0.5) + 0.5).clamp(0.0, 1.0);
            alpha[i] = (a * 255.0) as u8;
        }

        Self { width, height, alpha, glyph_table: glyph_table.to_vec() }
    }

    /// Sample alpha with bilinear interpolation at fractional atlas coordinates.
    fn sample(&self, ax: f32, ay: f32) -> f32 {
        let x0 = ax.floor() as i32;
        let y0 = ay.floor() as i32;
        let fx = ax - x0 as f32;
        let fy = ay - y0 as f32;

        let s00 = self.texel(x0, y0);
        let s10 = self.texel(x0 + 1, y0);
        let s01 = self.texel(x0, y0 + 1);
        let s11 = self.texel(x0 + 1, y0 + 1);

        let top = s00 + (s10 - s00) * fx;
        let bot = s01 + (s11 - s01) * fx;
        top + (bot - top) * fy
    }

    fn texel(&self, x: i32, y: i32) -> f32 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 { return 0.0; }
        self.alpha[(y as u32 * self.width + x as u32) as usize] as f32 / 255.0
    }
}

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

/// Rasterize a RenderFrame with an optional bitmap atlas for MSDF text.
pub fn rasterize_frame_with_atlas(frame: &RenderFrame, dark_theme: bool, atlas: Option<&BitmapAtlas>) -> Framebuffer {
    rasterize_impl(frame, dark_theme, atlas)
}

/// Rasterize a RenderFrame into a framebuffer (no MSDF text support).
pub fn rasterize_frame(frame: &RenderFrame, dark_theme: bool) -> Framebuffer {
    rasterize_impl(frame, dark_theme, None)
}

fn rasterize_impl(frame: &RenderFrame, dark_theme: bool, atlas: Option<&BitmapAtlas>) -> Framebuffer {
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
            8 => {
                if let Some(atlas) = atlas {
                    rasterize_msdf_text_cmd(&mut fb, cmd, &frame.char_buffer, atlas, &ribbon_clip, &ribbon_scroll);
                } else {
                    rasterize_text_cmd(&mut fb, cmd, &frame.char_buffer, &frame.glyph_bitmap, &frame.font, &ribbon_clip, &ribbon_scroll);
                }
            }
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

fn rasterize_msdf_text_cmd(
    fb: &mut Framebuffer,
    cmd: &SdfDrawCmd,
    char_buffer: &[u32],
    atlas: &BitmapAtlas,
    ribbon_clip: &Option<([f32; 2], [f32; 2])>,
    ribbon_scroll: &[f32; 2],
) {
    let px_range = cmd.params[1].max(2.0);
    let x_margin_frac = cmd.params[2];
    let packed = f32::to_bits(cmd.params[3]);
    let char_offset = (packed >> 16) as usize;
    let char_count = (packed & 0xFFFF) as usize;

    let line_h = cmd.size[1];
    let line_x = cmd.pos[0] + x_margin_frac * line_h;
    let line_y = cmd.pos[1];

    let mut cursor_x: f32 = 0.0;

    for ci in 0..char_count {
        if char_offset + ci >= char_buffer.len() { break; }
        let entry = char_buffer[char_offset + ci];
        let gt_idx = (entry >> 16) as usize;
        let delta_biased = (entry & 0xFFFF) as f32;
        let delta_px = (delta_biased - 2048.0) / 16.0;

        // Read glyph table entry (2 × vec4<u32>)
        let g0_base = gt_idx * 8; // 2 vec4s = 8 u32s
        if g0_base + 7 >= atlas.glyph_table.len() { break; }
        let g0 = &atlas.glyph_table[g0_base..g0_base + 4];
        let g1 = &atlas.glyph_table[g0_base + 4..g0_base + 8];

        let atlas_gx = (g0[1] & 0xFFFF) as f32;
        let atlas_gy = (g0[1] >> 16) as f32;
        let atlas_gw = (g0[2] & 0xFFFF) as f32;
        let atlas_gh = (g0[2] >> 16) as f32;

        let x_margin = f32::from_bits(g1[2]);
        let px_per_em_atlas = f32::from_bits(g1[1]);

        // Scale: atlas pixels per screen pixel
        let scale = if atlas_gh > 0.0 { atlas_gh / line_h } else { 1.0 };
        let font_size = if scale > 0.0 { px_per_em_atlas / scale } else { line_h };

        let advance_x_norm = f32::from_bits(g0[3]);
        let advance_px = advance_x_norm * font_size + delta_px;

        let gx = line_x + cursor_x;
        let gy = line_y;

        // Bounding box on screen
        let screen_w = if scale > 0.0 { atlas_gw / scale } else { 0.0 };
        let screen_h = line_h;
        let x0 = (gx - x_margin / scale).max(0.0) as u32;
        let y0 = gy.max(0.0) as u32;
        let x1 = ((gx + screen_w).ceil() as u32).min(fb.width);
        let y1 = ((gy + screen_h).ceil() as u32).min(fb.height);

        for y in y0..y1 {
            for x in x0..x1 {
                let mut px = x as f32 + 0.5;
                let mut py = y as f32 + 0.5;

                if let Some((clip_min, clip_max)) = ribbon_clip {
                    if px < clip_min[0] || px >= clip_max[0] || py < clip_min[1] || py >= clip_max[1] {
                        continue;
                    }
                    px -= ribbon_scroll[0];
                    py -= ribbon_scroll[1];
                }

                // Screen pixel → atlas cell pixel
                let acx = (px - gx) * scale + x_margin;
                let acy = (py - gy) * scale;

                if acx >= 0.0 && acx < atlas_gw && acy >= 0.0 && acy < atlas_gh {
                    let alpha = atlas.sample(atlas_gx + acx + 0.5, atlas_gy + acy + 0.5) * cmd.color[3];
                    if alpha > 0.01 {
                        fb.blend_pixel(x, y, cmd.color[0], cmd.color[1], cmd.color[2], alpha);
                    }
                }
            }
        }

        cursor_x += advance_px;
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
