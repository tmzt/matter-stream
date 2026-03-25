//! Pluggable render backend trait — monomorphized, no vtable dispatch.

/// Render backend for pixel-level drawing operations.
/// Implementations are monomorphized at compile time via generics.
pub trait RenderBackend {
    /// Alpha-composite `src_rgba` (0xRRGGBBAA) over `dst` (backend-specific format).
    fn blend_pixel(dst: u32, src_rgba: u32) -> u32;

    /// Draw a filled axis-aligned rectangle.
    fn draw_rect(buf: &mut [u32], width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32);

    /// Draw a filled rounded rectangle.
    #[allow(clippy::too_many_arguments)]
    fn draw_rounded_rect(buf: &mut [u32], width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, radius: u32, color: u32);

    /// Draw a filled circle.
    fn draw_circle(buf: &mut [u32], width: u32, height: u32, cx: i32, cy: i32, r: u32, color: u32);

    /// Draw a line segment.
    #[allow(clippy::too_many_arguments)]
    fn draw_line(buf: &mut [u32], width: u32, height: u32, x1: i32, y1: i32, x2: i32, y2: i32, color: u32);
}
