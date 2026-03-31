//! SDF draw command type and evaluation functions.
//!
//! `SdfDrawCmd` is the unified GPU-uploadable draw command format.
//! The VM emits it in MTUI mode; both GPU and CPU renderers consume it.

/// Draw type constants for SdfDrawCmd.params[0].
pub const DRAW_TYPE_BOX: f32 = 0.0;
pub const DRAW_TYPE_SLAB: f32 = 1.0;
pub const DRAW_TYPE_CIRCLE: f32 = 2.0;
pub const DRAW_TYPE_LINE: f32 = 3.0;
pub const DRAW_TYPE_TEXT: f32 = 4.0;
pub const DRAW_TYPE_TEXTURE: f32 = 5.0;
pub const DRAW_TYPE_RIBBON_BEGIN: f32 = 6.0;
pub const DRAW_TYPE_RIBBON_END: f32 = 7.0;
pub const DRAW_TYPE_MSDF_TEXT: f32 = 8.0;

/// Maximum draw commands per frame.
pub const MAX_DRAW_CMDS: usize = 4096;

/// Maximum animation bank entries.
pub const MAX_ANIMS: usize = 32;

/// Maximum texture bank entries.
pub const MAX_TEXTURES: usize = 8;

/// GPU texture descriptor — stored in texture_bank, 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuTexture {
    pub width: u32,
    pub height: u32,
    pub layer: u32,      // index into texture_2d_array
    pub flags: u32,      // format, filtering mode
}

impl GpuTexture {
    pub const NONE: Self = Self { width: 0, height: 0, layer: 0, flags: 0 };
}

/// Animation descriptor — stored in AnimBank, referenced by SdfDrawCmd.params[2].
/// 16 bytes, GPU-uploadable.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anim {
    /// Oscillation frequency in Hz. 0 = no oscillation (effectively off).
    pub freq: f32,
    /// Duty cycle 0.0-1.0. 1.0 = always on, 0.5 = blink.
    pub duty: f32,
    /// Packed bank ref for enable flag (bank_type << 16 | slot). 0 = always enabled.
    pub enable_ref: u32,
    pub _pad: u32,
}

impl Anim {
    pub const NONE: Self = Self { freq: 0.0, duty: 1.0, enable_ref: 0, _pad: 0 };
}

/// A single SDF draw command. repr(C) for GPU buffer upload.
///
/// Wire format: 48 bytes (12 × f32).
/// ```text
/// pos:    [f32; 2]   x, y
/// size:   [f32; 2]   w, h
/// color:  [f32; 4]   r, g, b, a (0.0-1.0)
/// params: [f32; 4]   [type, radius, softness, slot]
/// ```
/// Wire format: 48 bytes (12 × f32).
/// ```text
/// pos:    [f32; 2]   x, y
/// size:   [f32; 2]   w, h
/// color:  [f32; 4]   r, g, b, a (0.0-1.0)
/// params: [f32; 4]   [type, radius, anim_idx, slot]
///   params[0] = draw type (DRAW_TYPE_*)
///   params[1] = radius (Slab, Circle)
///   params[2] = anim_bank index (0 = no animation, 1+ = AnimBank[idx-1])
///   params[3] = slot (Text: string ref)
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SdfDrawCmd {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub params: [f32; 4],
}

impl SdfDrawCmd {
    pub const ZERO: Self = Self {
        pos: [0.0; 2],
        size: [0.0; 2],
        color: [0.0; 4],
        params: [0.0; 4],
    };

    /// Draw type from params[0].
    pub fn draw_type(&self) -> f32 {
        self.params[0]
    }

    /// Radius from params[1] (Slab, Circle).
    pub fn radius(&self) -> f32 {
        self.params[1]
    }

    /// Slot/string index from params[3] (Text).
    pub fn slot(&self) -> f32 {
        self.params[3]
    }
}

// ── SDF evaluation functions (pure math, shared between CPU and shader) ──

/// Signed distance to a rounded box centered at origin.
/// `half_size` = (w/2, h/2), `radius` = corner radius.
/// Returns negative inside, positive outside.
pub fn sd_rounded_box(px: f32, py: f32, half_w: f32, half_h: f32, radius: f32) -> f32 {
    let qx = px.abs() - half_w + radius;
    let qy = py.abs() - half_h + radius;
    let outside = (qx.max(0.0) * qx.max(0.0) + qy.max(0.0) * qy.max(0.0)).sqrt();
    let inside = qx.max(qy).min(0.0);
    outside + inside - radius
}

/// Signed distance to a box (axis-aligned, no rounding).
pub fn sd_box(px: f32, py: f32, half_w: f32, half_h: f32) -> f32 {
    sd_rounded_box(px, py, half_w, half_h, 0.0)
}

/// Signed distance to a circle centered at origin.
pub fn sd_circle(px: f32, py: f32, radius: f32) -> f32 {
    (px * px + py * py).sqrt() - radius
}

/// Signed distance to a line segment from (ax,ay) to (bx,by), with thickness.
pub fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32, thickness: f32) -> f32 {
    let pax = px - ax;
    let pay = py - ay;
    let bax = bx - ax;
    let bay = by - ay;
    let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay)).clamp(0.0, 1.0);
    let dx = pax - bax * h;
    let dy = pay - bay * h;
    (dx * dx + dy * dy).sqrt() - thickness * 0.5
}

/// Evaluate the SDF for a single draw command at pixel position (px, py).
/// Returns (distance, color) — negative distance means inside.
/// For animated evaluation, use `sdf_eval_animated`.
pub fn sdf_eval(cmd: &SdfDrawCmd, px: f32, py: f32) -> (f32, [f32; 4]) {
    let cx = cmd.pos[0] + cmd.size[0] * 0.5;
    let cy = cmd.pos[1] + cmd.size[1] * 0.5;
    let local_x = px - cx;
    let local_y = py - cy;
    let hw = cmd.size[0] * 0.5;
    let hh = cmd.size[1] * 0.5;

    let d = match cmd.draw_type() as u32 {
        0 => sd_box(local_x, local_y, hw, hh),                         // Box
        1 => sd_rounded_box(local_x, local_y, hw, hh, cmd.radius()),   // Slab
        2 => sd_circle(local_x, local_y, hw.min(hh)),                  // Circle
        3 => {
            // Line: pos = (x1,y1), size = (x2,y2)
            sd_segment(px, py, cmd.pos[0], cmd.pos[1], cmd.size[0], cmd.size[1], 1.0)
        }
        4 => sd_box(local_x, local_y, hw, hh),                         // Text (placeholder box)
        _ => f32::MAX,
    };

    (d, cmd.color)
}

/// Unpack a u32 RGBA (0xRRGGBBAA) to [f32; 4] (0.0-1.0).
/// Evaluate SDF with time-based animation from AnimBank.
/// `time_ms` = elapsed milliseconds from GlobalUniforms.
/// `anim_bank` = current animation descriptors.
/// `int_bank` = for reading enable flags.
/// Returns (distance, animated_color) with alpha modulated by animation.
pub fn sdf_eval_animated(
    cmd: &SdfDrawCmd,
    px: f32,
    py: f32,
    time_ms: f32,
    anim_bank: &[Anim],
    int_bank: &[i32],
) -> (f32, [f32; 4]) {
    let (d, mut color) = sdf_eval(cmd, px, py);

    // params[2] = anim_bank index. 0 = no animation, 1+ = AnimBank[idx-1]
    let anim_idx = cmd.params[2] as u32;
    if anim_idx > 0 {
        let idx = (anim_idx - 1) as usize;
        if idx < anim_bank.len() {
            let anim = &anim_bank[idx];

            // Read enable from packed int_bank ref
            let enabled = if anim.enable_ref != 0 {
                let slot = (anim.enable_ref & 0xFFFF) as usize;
                if slot < int_bank.len() { int_bank[slot] as f32 } else { 0.0 }
            } else {
                1.0 // no enable ref = always enabled
            };

            if anim.freq > 0.0 {
                let time_s = time_ms / 1000.0;
                let phase = (time_s * anim.freq * std::f32::consts::TAU).sin();
                let threshold = 1.0 - anim.duty * 2.0;
                let pulse = smoothstep(0.0, 0.1, phase - threshold);
                color[3] *= enabled * pulse;
            } else {
                // freq=0: static, just apply enable
                color[3] *= enabled;
            }
        }
    }

    (d, color)
}

/// Smoothstep interpolation (matches WGSL smoothstep).
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Check if any animation in the bank is active (freq > 0 or enable != 0).
/// Used to determine refresh rate — if no animations, can drop to low FPS.
pub fn any_animation_active(anim_bank: &[Anim], int_bank: &[i32]) -> bool {
    anim_bank.iter().any(|a| {
        if a.freq > 0.0 {
            if a.enable_ref != 0 {
                let slot = (a.enable_ref & 0xFFFF) as usize;
                slot < int_bank.len() && int_bank[slot] != 0
            } else {
                true
            }
        } else {
            false
        }
    })
}

pub fn color_u32_to_f32(rgba: u32) -> [f32; 4] {
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

    #[test]
    fn sd_box_inside() {
        assert!(sd_box(0.0, 0.0, 10.0, 10.0) < 0.0);
    }

    #[test]
    fn sd_box_outside() {
        assert!(sd_box(15.0, 0.0, 10.0, 10.0) > 0.0);
    }

    #[test]
    fn sd_rounded_box_corner() {
        // Point at corner, just outside the rounding
        let d = sd_rounded_box(9.0, 9.0, 10.0, 10.0, 2.0);
        assert!(d < 0.0); // inside the rounded box
    }

    #[test]
    fn sd_circle_center() {
        assert!(sd_circle(0.0, 0.0, 5.0) < 0.0);
    }

    #[test]
    fn sd_circle_outside() {
        assert!(sd_circle(10.0, 0.0, 5.0) > 0.0);
    }

    #[test]
    fn sdf_eval_box() {
        let cmd = SdfDrawCmd {
            pos: [10.0, 10.0],
            size: [20.0, 20.0],
            color: [1.0, 0.0, 0.0, 1.0],
            params: [DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
        };
        let (d, _) = sdf_eval(&cmd, 20.0, 20.0); // center
        assert!(d < 0.0);
        let (d, _) = sdf_eval(&cmd, 0.0, 0.0); // outside
        assert!(d > 0.0);
    }

    #[test]
    fn color_conversion() {
        let c = color_u32_to_f32(0xFF0000FF);
        assert!((c[0] - 1.0).abs() < 0.01);
        assert!(c[1].abs() < 0.01);
        assert!(c[2].abs() < 0.01);
        assert!((c[3] - 1.0).abs() < 0.01);
    }
}
