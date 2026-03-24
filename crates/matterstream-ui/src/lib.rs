//! MatterStream UI — CPU-side rasterizer, hit testing, and action collection.
//!
//! This crate provides rendering functions for `UiDrawCmd` output from the VM,
//! plus action/interaction handling. The VM types (`UiDrawCmd`, `UiDrawState`)
//! live in `matterstream-vm`; this crate adds the rendering layer on top.

pub mod render;
pub mod actions;

pub use render::{
    render_ui_draws, render_ui_draws_with_font,
    draw_filled_rect, draw_rounded_rect, draw_filled_circle, draw_line,
    blend_pixel, rgba_unpack,
};
pub use actions::{ActionEvent, collect_actions, hit_test_action};
