//! MatterStream UI — draw command types, render dispatch, and action handling.
//!
//! Pixel-level rendering is pluggable via `RenderBackend` from `matterstream-common`.
//! The softbuffer implementation lives in `matterstream-ui-soft`.

pub mod types;
pub mod render;
pub mod actions;

pub use types::*;
pub use render::{render_ui_draws, render_ui_draws_with_font};
pub use actions::{ActionEvent, collect_actions, hit_test_action};
