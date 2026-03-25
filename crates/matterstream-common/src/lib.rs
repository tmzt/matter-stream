//! Shared utilities for the MatterStream ecosystem. Zero external dependencies.

pub mod gfx_utils;
pub mod backend;
pub mod sdf;
pub mod pipeline;

pub use gfx_utils::{rgba, rgba_unpack};
pub use backend::Rasterizer;
pub use sdf::{SdfDrawCmd, sdf_eval, sd_box, sd_rounded_box, sd_circle, sd_segment, color_u32_to_f32};
pub use sdf::{DRAW_TYPE_BOX, DRAW_TYPE_SLAB, DRAW_TYPE_CIRCLE, DRAW_TYPE_LINE, DRAW_TYPE_TEXT, MAX_DRAW_CMDS};
pub use pipeline::UiPipeline;
