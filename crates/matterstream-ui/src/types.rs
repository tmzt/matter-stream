//! Core UI types — draw commands, draw state, color helpers, transform math.

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
    TextStr {
        x: i32,
        y: i32,
        size: u32,
        str_idx: u32,
        color: u32,
    },
    Line {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        color: u32,
    },
    Action {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        str_idx: u32,
    },
}

/// UI draw state: current color (offsets moved to transform stack).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiDrawState {
    pub color: u32,
}

impl Default for UiDrawState {
    fn default() -> Self {
        Self {
            color: 0xFFFFFFFF, // white, fully opaque
        }
    }
}

/// Identity 4x4 matrix (column-major, OpenGL convention).
pub const MAT4_IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 1.0, 0.0,
    0.0, 0.0, 0.0, 1.0,
];

/// Multiply two 4x4 matrices (column-major).
pub fn mat4_multiply(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0f32;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            out[col * 4 + row] = sum;
        }
    }
    out
}

/// Apply the top transform matrix to a point, returning transformed (x, y).
pub fn apply_transform(transform: &[f32; 16], x: i32, y: i32) -> (i32, i32) {
    let is_translation_only =
        transform[0] == 1.0 && transform[1] == 0.0 && transform[2] == 0.0 && transform[3] == 0.0
        && transform[4] == 0.0 && transform[5] == 1.0 && transform[6] == 0.0 && transform[7] == 0.0
        && transform[8] == 0.0 && transform[9] == 0.0 && transform[10] == 1.0 && transform[11] == 0.0
        && transform[15] == 1.0;

    if is_translation_only {
        let dx = transform[12] as i32;
        let dy = transform[13] as i32;
        (x + dx, y + dy)
    } else {
        let fx = x as f32;
        let fy = y as f32;
        let rx = transform[0] * fx + transform[4] * fy + transform[12];
        let ry = transform[1] * fx + transform[5] * fy + transform[13];
        (rx as i32, ry as i32)
    }
}

/// Pack RGBA components into a u32 (0xRRGGBBAA).
pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    (r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | a as u32
}
