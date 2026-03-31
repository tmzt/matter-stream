//! Shared utilities for the MatterStream ecosystem. Zero external dependencies.

pub mod gfx_utils;
pub mod backend;
pub mod sdf;
pub mod pipeline;
pub mod font;

pub use gfx_utils::{rgba, rgba_unpack};
pub use backend::Rasterizer;
pub use sdf::{SdfDrawCmd, Anim, sdf_eval, sdf_eval_animated, any_animation_active, sd_box, sd_rounded_box, sd_circle, sd_segment, color_u32_to_f32};
pub use sdf::{DRAW_TYPE_BOX, DRAW_TYPE_SLAB, DRAW_TYPE_CIRCLE, DRAW_TYPE_LINE, DRAW_TYPE_TEXT, DRAW_TYPE_TEXTURE, DRAW_TYPE_RIBBON_BEGIN, DRAW_TYPE_RIBBON_END, MAX_DRAW_CMDS, MAX_ANIMS, MAX_TEXTURES, GpuTexture};
pub use pipeline::{RenderFrame};
pub use font::{GpuFont, StringOffset, pack_bitmap, pack_strings, MAX_FONTS, truncate_str, wordwrap};
