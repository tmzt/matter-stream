//! Example: React-like UI components — Login Form.
//!
//! Demonstrates composable component functions that emit Op sequences
//! with per-draw sizing via Op::SetSize.
//!
//! Run with: cargo run -p matterstream --example components

use std::num::NonZeroU32;
use std::sync::Arc;

use matterstream::{
    Draw, MatterStream, Op, OpsHeader, Primitive, RsiPointer, StreamBuilder, BankId,
};
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

// ---------------------------------------------------------------------------
// Component functions — each returns Vec<Op>, using PushState/PopState
// ---------------------------------------------------------------------------

fn button(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Vec<Op> {
    StreamBuilder::new()
        .push_state()
        .set_color(color)
        .set_size([w, h])
        .set_trans([x, y, 0.0])
        .draw(Primitive::Slab, 0)
        .pop_state()
        .build()
}

fn card(x: f32, y: f32, w: f32, h: f32, bg: [f32; 4], children: Vec<Op>) -> Vec<Op> {
    let mut ops = StreamBuilder::new()
        .push_state()
        .set_color(bg)
        .set_size([w, h])
        .set_trans([x, y, 0.0])
        .draw(Primitive::Slab, 0)
        .build();
    ops.extend(children);
    ops.push(Op::PopState);
    ops
}

fn input_field(x: f32, y: f32, w: f32, h: f32) -> Vec<Op> {
    StreamBuilder::new()
        .push_state()
        .set_color([0.15, 0.15, 0.18, 1.0])
        .set_size([w, h])
        .set_trans([x, y, 0.0])
        .draw(Primitive::Slab, 0)
        .pop_state()
        .build()
}

fn header_bar(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Vec<Op> {
    StreamBuilder::new()
        .push_state()
        .set_color(color)
        .set_size([w, h])
        .set_trans([x, y, 0.0])
        .draw(Primitive::Slab, 0)
        .pop_state()
        .build()
}

fn divider(x: f32, y: f32, w: f32) -> Vec<Op> {
    StreamBuilder::new()
        .push_state()
        .set_color([0.3, 0.3, 0.35, 1.0])
        .set_size([w, 0.01])
        .set_trans([x, y, 0.0])
        .draw(Primitive::Slab, 0)
        .pop_state()
        .build()
}

// ---------------------------------------------------------------------------
// Render helper — maps Draw results to pixel rectangles
// ---------------------------------------------------------------------------

fn render_draw(draw: &Draw, width: u32, height: u32, buffer: &mut [u32]) {
    let cx = ((draw.position[0] + 1.0) / 2.0 * width as f32) as i32;
    let cy = ((-draw.position[1] + 1.0) / 2.0 * height as f32) as i32;

    // Use draw.size if set, otherwise fall back to a small default
    let (sw, sh) = if draw.size[0] > 0.0 || draw.size[1] > 0.0 {
        (
            (draw.size[0] * width as f32) as i32,
            (draw.size[1] * height as f32) as i32,
        )
    } else {
        let s = (width.min(height) as f32 * 0.05) as i32;
        (s, s)
    };

    let r = (draw.color[0] * 255.0) as u32;
    let g = (draw.color[1] * 255.0) as u32;
    let b = (draw.color[2] * 255.0) as u32;
    let color = (0xFF << 24) | (r << 16) | (g << 8) | b;

    let x0 = cx - sw / 2;
    let y0 = cy - sh / 2;
    for py in y0..(y0 + sh) {
        for px in x0..(x0 + sw) {
            if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                buffer[(py * width as i32 + px) as usize] = color;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main — Login Form
// ---------------------------------------------------------------------------

fn main() {
    // Compose the login form scene
    let mut ops: Vec<Op> = Vec::new();

    // Header bar (blue, full width at top)
    ops.extend(header_bar(0.0, 0.75, 1.6, 0.2, [0.08, 0.40, 0.75, 1.0]));

    // Card container (dark background, centered)
    let children = {
        let mut c = Vec::new();
        // Email input
        c.extend(input_field(0.0, 0.25, 1.0, 0.12));
        // Password input
        c.extend(input_field(0.0, 0.0, 1.0, 0.12));
        // Divider
        c.extend(divider(0.0, -0.15, 1.0));
        // Submit button (green)
        c.extend(button(0.0, -0.35, 0.6, 0.14, [0.18, 0.80, 0.44, 1.0]));
        c
    };
    ops.extend(card(0.0, -0.05, 1.2, 0.9, [0.12, 0.12, 0.15, 1.0], children));

    println!("Login form: {} ops", ops.len());

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("MatterStream — Login Form (Components)")
                    .with_inner_size(winit::dpi::LogicalSize::new(400, 500)),
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
                    buffer.fill(0xFF1A1A2E); // Dark background

                    for draw in &stream.draws {
                        render_draw(draw, width, height, &mut buffer);
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
