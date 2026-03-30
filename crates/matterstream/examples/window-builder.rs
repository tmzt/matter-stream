//! Example: Build ops with StreamBuilder and render them in a window.
//!
//! Demonstrates the ISA directly — no TSX parsing involved.
//! Uses PushState/PopState to isolate color per slab.
//!
//! Run with: cargo run -p matterstream --example window-builder

use std::num::NonZeroU32;
use std::sync::Arc;

use matterstream::{
    MatterStream, OpsHeader, Primitive, RsiPointer, BankId, StreamBuilder, Op,
};
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

fn main() {
    // Build a scene programmatically: a 3x3 grid of colored slabs
    let colors: [[f32; 4]; 9] = [
        [0.91, 0.30, 0.24, 1.0], // red
        [0.90, 0.49, 0.13, 1.0], // orange
        [0.95, 0.77, 0.06, 1.0], // yellow
        [0.18, 0.80, 0.44, 1.0], // green
        [0.20, 0.60, 0.86, 1.0], // blue
        [0.61, 0.35, 0.71, 1.0], // purple
        [0.10, 0.74, 0.61, 1.0], // teal
        [0.91, 0.12, 0.39, 1.0], // pink
        [0.93, 0.93, 0.93, 1.0], // white
    ];

    let mut ops: Vec<Op> = Vec::new();
    for (i, color) in colors.iter().enumerate() {
        let col = (i % 3) as f32;
        let row = (i / 3) as f32;
        // Map grid (0..2, 0..2) to NDC (-0.6..0.6, 0.6..-0.6)
        let x = -0.6 + col * 0.6;
        let y = 0.6 - row * 0.6;

        let slab_ops = StreamBuilder::new()
            .push_state()
            .set_trans([x, y, 0.0])
            .draw(Primitive::Slab, 0)
            .pop_state()
            .build();

        // Set color before push_state so it's part of the pushed state
        ops.push(Op::SetColor(*color));
        ops.extend(slab_ops);
    }

    println!("Built {} ops for 3x3 slab grid", ops.len());

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("MatterStream — StreamBuilder Grid")
                    .with_inner_size(winit::dpi::LogicalSize::new(640, 480)),
            )
            .unwrap(),
    );

    let context = Context::new(window.clone()).unwrap();
    let mut surface = Surface::new(&context, window.clone()).unwrap();

    let mut stream = MatterStream::new();
    let header = OpsHeader::new(vec![RsiPointer::new(1, BankId::Vec3 as u8, 0)], false);

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);
            window.request_redraw();

            match event {
                Event::WindowEvent { event: WindowEvent::RedrawRequested, .. } => {
                    smol::block_on(async {
                        let _ = stream.execute(&header, &ops).await;
                    });

                    let (width, height) = {
                        let size = window.inner_size();
                        (size.width, size.height)
                    };
                    if width == 0 || height == 0 { return; }

                    surface.resize(NonZeroU32::new(width).unwrap(), NonZeroU32::new(height).unwrap()).unwrap();
                    let mut buffer = surface.buffer_mut().unwrap();
                    buffer.fill(0xFF1E1E2E);

                    let slab_size = (width.min(height) as f32 * 0.12) as i32;

                    for draw in &stream.draws {
                        let cx = ((draw.position[0] + 1.0) / 2.0 * width as f32) as i32;
                        let cy = ((-draw.position[1] + 1.0) / 2.0 * height as f32) as i32;

                        let r = (draw.color[0] * 255.0) as u32;
                        let g = (draw.color[1] * 255.0) as u32;
                        let b = (draw.color[2] * 255.0) as u32;
                        let color = (0xFF << 24) | (r << 16) | (g << 8) | b;

                        let x0 = cx - slab_size / 2;
                        let y0 = cy - slab_size / 2;
                        for py in y0..(y0 + slab_size) {
                            for px in x0..(x0 + slab_size) {
                                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                                    buffer[(py * width as i32 + px) as usize] = color;
                                }
                            }
                        }
                    }

                    buffer.present().unwrap();
                }
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                    elwt.exit();
                }
                _ => (),
            }
        })
        .unwrap();
}
