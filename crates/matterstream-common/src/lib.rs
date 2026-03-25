//! Shared utilities for the MatterStream ecosystem. Zero external dependencies.

pub mod gfx_utils;
pub mod backend;

pub use gfx_utils::{rgba, rgba_unpack};
pub use backend::Rasterizer;
