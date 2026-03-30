//! Example: Compile inline TSX and render colored slabs in a native window.
//!
//! Run with: cargo run -p matterstream --example window-slabs

use std::num::NonZeroU32;
use std::sync::Arc;

use matterstream::{Compiler, MatterStream, OpsHeader, RsiPointer, BankId};
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

fn main() {
    let source = r##"
<>
  <Slab x={-0.6} y={0.4} color="#E74C3CFF" />
  <Slab x={-0.2} y={0.4} color="#E67E22FF" />
  <Slab x={0.2} y={0.4} color="#F1C40FFF" />
  <Slab x={0.6} y={0.4} color="#2ECC71FF" />
  <Slab x={-0.6} y={-0.1} color="#3498DBFF" />
  <Slab x={-0.2} y={-0.1} color="#9B59B6FF" />
  <Slab x={0.2} y={-0.1} color="#1ABC9CFF" />
  <Slab x={0.6} y={-0.1} color="#E91E63FF" />
</>
"##;

    let compiled = Compiler::compile(source).expect("compile failed");
    println!("Compiled {} ops from inline TSX", compiled.ops.len());

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("MatterStream — Window Slabs")
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
                        let _ = stream.execute(&header, &compiled.ops).await;
                    });

                    let (width, height) = {
                        let size = window.inner_size();
                        (size.width, size.height)
                    };
                    if width == 0 || height == 0 { return; }

                    surface.resize(NonZeroU32::new(width).unwrap(), NonZeroU32::new(height).unwrap()).unwrap();
                    let mut buffer = surface.buffer_mut().unwrap();
                    buffer.fill(0xFF1E1E2E); // Dark background

                    let slab_w = (width as f32 * 0.08) as i32;
                    let slab_h = (height as f32 * 0.12) as i32;

                    for draw in &stream.draws {
                        // Map NDC [-1,1] to pixel coordinates
                        let cx = ((draw.position[0] + 1.0) / 2.0 * width as f32) as i32;
                        let cy = ((-draw.position[1] + 1.0) / 2.0 * height as f32) as i32;

                        let r = (draw.color[0] * 255.0) as u32;
                        let g = (draw.color[1] * 255.0) as u32;
                        let b = (draw.color[2] * 255.0) as u32;
                        let color = (0xFF << 24) | (r << 16) | (g << 8) | b;

                        let x0 = cx - slab_w / 2;
                        let y0 = cy - slab_h / 2;
                        for py in y0..(y0 + slab_h) {
                            for px in x0..(x0 + slab_w) {
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
