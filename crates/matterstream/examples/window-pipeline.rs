//! Example: Full pipeline rendered in a window.
//!
//! Parse → Compile → Process (with PackageRegistry) → Execute → Render.
//! Shows a mock login form layout using the @mtsm/ui/core package.
//!
//! Run with: cargo run -p matterstream --example window-pipeline

use std::num::NonZeroU32;
use std::sync::Arc;

use matterstream::{
    Compiler, MatterStream, OpsHeader, Parser, Processor,
    PackageRegistry, CoreUiPackage,
    RsiPointer, BankId,
};
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

fn main() {
    let source = r##"
import { Slab } from '@mtsm/ui/core';

<>
  <Slab x={0.0} y={0.6} color="#ECEFF1FF" />
  <Slab x={0.0} y={0.2} color="#37474FFF" />
  <Slab x={0.0} y={-0.2} color="#37474FFF" />
  <Slab x={0.0} y={-0.6} color="#1565C0FF" />
</>
"##;

    // --- Pipeline ---
    println!("[1/4] Parsing...");
    let _parsed = Parser::parse(source).expect("parse failed");
    println!("[2/4] Compiling...");
    let compiled = Compiler::compile(source).expect("compile failed");
    println!("[3/4] Processing...");
    let mut registry = PackageRegistry::new();
    registry.register_package(CoreUiPackage);
    let processor = Processor::new();
    let output = processor.process(compiled, &registry).expect("process failed");
    println!("[4/4] Rendering {} ops in window...", output.ops.ops.len());

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("MatterStream — Full Pipeline")
                    .with_inner_size(winit::dpi::LogicalSize::new(400, 500)),
            )
            .unwrap(),
    );

    let context = Context::new(window.clone()).unwrap();
    let mut surface = Surface::new(&context, window.clone()).unwrap();

    let mut stream = MatterStream::new();
    let header = OpsHeader::new(vec![RsiPointer::new(1, BankId::Vec3 as u8, 0)], false);
    let ops = output.ops.ops;

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
                    buffer.fill(0xFF263238); // Blue-grey background

                    // Draw each slab as a wide horizontal bar
                    let bar_w = (width as f32 * 0.6) as i32;
                    let bar_h = (height as f32 * 0.1) as i32;

                    for draw in &stream.draws {
                        let cx = ((draw.position[0] + 1.0) / 2.0 * width as f32) as i32;
                        let cy = ((-draw.position[1] + 1.0) / 2.0 * height as f32) as i32;

                        let r = (draw.color[0] * 255.0) as u32;
                        let g = (draw.color[1] * 255.0) as u32;
                        let b = (draw.color[2] * 255.0) as u32;
                        let color = (0xFF << 24) | (r << 16) | (g << 8) | b;

                        let x0 = cx - bar_w / 2;
                        let y0 = cy - bar_h / 2;
                        for py in y0..(y0 + bar_h) {
                            for px in x0..(x0 + bar_w) {
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
