//! MatterStream UI — draw command types, CPU-side rasterizer, hit testing, and action collection.
//!
//! `UiDrawCmd` and related types are the canonical UI output format.
//! The VM pushes these during execution; the rasterizer renders them to pixels.

pub mod types;
pub mod render;
pub mod actions;

pub use types::*;
pub use render::{
    render_ui_draws, render_ui_draws_with_font,
    draw_filled_rect, draw_rounded_rect, draw_filled_circle, draw_line,
    blend_pixel, rgba_unpack,
};
pub use actions::{ActionEvent, collect_actions, hit_test_action};
