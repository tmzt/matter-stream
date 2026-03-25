//! UiPipeline trait — monomorphized render pipeline interface.
//!
//! Both GPU (wgpu) and CPU (softbuffer SDF eval) implement this.

use crate::sdf::SdfDrawCmd;

/// Render pipeline for SDF draw commands.
/// Implementations are monomorphized via generics — no vtable dispatch.
pub trait UiPipeline {
    /// Render a draw list to the output surface.
    fn render(&mut self, draws: &[SdfDrawCmd], string_table: &[String]);

    /// Resize the output surface.
    fn resize(&mut self, width: u32, height: u32);
}
