use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use softbuffer::Buffer;
use matterstream_core::Draw;

pub struct Renderer;

impl Renderer {
    pub fn render<D: HasDisplayHandle, W: HasWindowHandle>(
        buffer: &mut Buffer<'_, D, W>,
        draws: &[Draw],
        width: u32,
        height: u32,
    ) {
        buffer.fill(0xFF181818);

        for draw in draws {
            let x = ((draw.position[0] + 1.0) / 2.0 * width as f32) as i32;
            let y = ((-draw.position[1] + 1.0) / 2.0 * height as f32) as i32;

            let r = (draw.color[0] * 255.0) as u32;
            let g = (draw.color[1] * 255.0) as u32;
            let b = (draw.color[2] * 255.0) as u32;
            let a = (draw.color[3] * 255.0) as u32;
            let color_u32 = (a << 24) | (r << 16) | (g << 8) | b;

            for i in 0..10 {
                for j in 0..10 {
                    let px = x + i;
                    let py = y + j;
                    if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                        let index = (py * width as i32 + px) as usize;
                        if index < buffer.len() {
                            buffer[index] = color_u32;
                        }
                    }
                }
            }
        }
    }
}
