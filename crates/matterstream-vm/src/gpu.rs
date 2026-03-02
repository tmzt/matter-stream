//! GPU pipeline types and DrawCmd for the compute → vertex → fragment pipeline.
//!
//! This module defines the data structures shared between CPU and GPU.
//! The actual GPU pipeline (wgpu device, shaders, bind groups) is constructed
//! in examples that depend on wgpu, since wgpu is a dev-dependency.

/// A single draw command produced by the compute shader (or CPU-side builder).
/// Each DrawCmd maps to one instanced quad in the vertex shader.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DrawCmd {
    /// Position (x, y).
    pub pos: [f32; 2],
    /// Size (width, height).
    pub size: [f32; 2],
    /// Color (RGBA, 0.0–1.0).
    pub color: [f32; 4],
    /// Type-specific parameters: [ty, radius, softness, slot].
    /// ty: 0=Box, 1=Slab, 2=Circle, 3=Line, 4=Text
    pub params: [f32; 4],
}

// Safety: DrawCmd is repr(C) and contains only f32 fields
unsafe impl bytemuck::Pod for DrawCmd {}
unsafe impl bytemuck::Zeroable for DrawCmd {}

impl DrawCmd {
    pub fn box_cmd(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Self {
        Self {
            pos: [x, y],
            size: [w, h],
            color,
            params: [0.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn slab_cmd(x: f32, y: f32, w: f32, h: f32, radius: f32, color: [f32; 4]) -> Self {
        Self {
            pos: [x, y],
            size: [w, h],
            color,
            params: [1.0, radius, 0.0, 0.0],
        }
    }

    pub fn circle_cmd(cx: f32, cy: f32, diameter: f32, color: [f32; 4]) -> Self {
        Self {
            pos: [cx - diameter / 2.0, cy - diameter / 2.0],
            size: [diameter, diameter],
            color,
            params: [2.0, diameter / 2.0, 0.0, 0.0],
        }
    }

    pub fn line_cmd(x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 4]) -> Self {
        let cx = (x1 + x2) / 2.0;
        let cy = (y1 + y2) / 2.0;
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt();
        Self {
            pos: [cx - len / 2.0, cy - thickness / 2.0],
            size: [len, thickness],
            color,
            params: [3.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn text_cmd(x: f32, y: f32, w: f32, h: f32, slot: f32, color: [f32; 4]) -> Self {
        Self {
            pos: [x, y],
            size: [w, h],
            color,
            params: [4.0, 0.0, 0.0, slot],
        }
    }
}

/// Maximum draw commands per frame.
pub const MAX_DRAW_CMDS: usize = 4096;

/// GPU render bytecode opcodes (u32 words, for the compute shader interpreter).
pub mod render_ops {
    pub const OP_NOP: u32 = 0;
    pub const OP_SET_COLOR: u32 = 1;      // followed by RGBA as u32
    pub const OP_BOX: u32 = 2;            // pops x, y, w, h from stack
    pub const OP_SLAB: u32 = 3;           // pops x, y, w, h, radius from stack
    pub const OP_CIRCLE: u32 = 4;         // pops cx, cy, r from stack
    pub const OP_TEXT: u32 = 5;            // pops x, y, size, slot from stack
    pub const OP_LINE: u32 = 6;           // pops x1, y1, x2, y2 from stack
    pub const OP_PUSH_IMM: u32 = 7;       // followed by u32 immediate
    pub const OP_LOAD_SCALAR: u32 = 8;    // followed by index
    pub const OP_LOAD_INT: u32 = 9;       // followed by index
    pub const OP_LOAD_VEC4: u32 = 10;     // followed by index
    pub const OP_LOAD_ZP: u32 = 11;       // followed by byte offset
    pub const OP_JMP: u32 = 12;           // followed by target word offset
    pub const OP_JMP_IF: u32 = 13;        // followed by target, pops condition
    pub const OP_CMP_GT: u32 = 14;
    pub const OP_ADD: u32 = 15;
    pub const OP_HALT: u32 = 16;
    pub const OP_DUP: u32 = 17;
    pub const OP_DROP: u32 = 18;
    pub const OP_PUSH_STATE: u32 = 19;
    pub const OP_POP_STATE: u32 = 20;
    pub const OP_SET_OFFSET: u32 = 21;
    pub const OP_MOD: u32 = 22;
    pub const OP_DIV: u32 = 23;
    pub const OP_MUL: u32 = 24;
    pub const OP_SUB: u32 = 25;
    pub const OP_CMP_EQ: u32 = 26;
}

/// Build a CPU-side draw list from VM ui_draws (used as fallback when no GPU).
pub fn build_draw_list_from_ui_draws(
    draws: &[crate::ui_vm::UiDrawCmd],
) -> Vec<DrawCmd> {
    let mut cmds = Vec::with_capacity(draws.len());
    for draw in draws {
        match draw {
            crate::ui_vm::UiDrawCmd::Box { x, y, w, h, color } => {
                cmds.push(DrawCmd::box_cmd(
                    *x as f32, *y as f32, *w as f32, *h as f32,
                    unpack_color_f32(*color),
                ));
            }
            crate::ui_vm::UiDrawCmd::Slab { x, y, w, h, radius, color } => {
                cmds.push(DrawCmd::slab_cmd(
                    *x as f32, *y as f32, *w as f32, *h as f32, *radius as f32,
                    unpack_color_f32(*color),
                ));
            }
            crate::ui_vm::UiDrawCmd::Circle { x, y, r, color } => {
                cmds.push(DrawCmd::circle_cmd(
                    *x as f32, *y as f32, (*r * 2) as f32,
                    unpack_color_f32(*color),
                ));
            }
            crate::ui_vm::UiDrawCmd::Text { x, y, size, slot, color } => {
                cmds.push(DrawCmd::text_cmd(
                    *x as f32, *y as f32, (*size * 4) as f32, *size as f32, *slot as f32,
                    unpack_color_f32(*color),
                ));
            }
            crate::ui_vm::UiDrawCmd::TextStr { x, y, size, str_idx, color } => {
                cmds.push(DrawCmd::text_cmd(
                    *x as f32, *y as f32, (*size * 4) as f32, *size as f32, *str_idx as f32,
                    unpack_color_f32(*color),
                ));
            }
            crate::ui_vm::UiDrawCmd::Line { x1, y1, x2, y2, color } => {
                cmds.push(DrawCmd::line_cmd(
                    *x1 as f32, *y1 as f32, *x2 as f32, *y2 as f32, 1.0,
                    unpack_color_f32(*color),
                ));
            }
        }
    }
    cmds
}

fn unpack_color_f32(rgba: u32) -> [f32; 4] {
    [
        ((rgba >> 24) & 0xFF) as f32 / 255.0,
        ((rgba >> 16) & 0xFF) as f32 / 255.0,
        ((rgba >> 8) & 0xFF) as f32 / 255.0,
        (rgba & 0xFF) as f32 / 255.0,
    ]
}
