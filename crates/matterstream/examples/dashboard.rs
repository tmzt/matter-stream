//! Example: React-like UI components — Dashboard.
//!
//! Demonstrates a more complex layout with header, cards, badges,
//! and per-draw sizing via Op::SetSize.
//!
//! Run with: cargo run -p matterstream --example dashboard

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
// Component functions
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

fn badge(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Vec<Op> {
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
        .set_size([w, 0.008])
        .set_trans([x, y, 0.0])
        .draw(Primitive::Slab, 0)
        .pop_state()
        .build()
}

// ---------------------------------------------------------------------------
// Render helper
// ---------------------------------------------------------------------------

fn render_draw(draw: &Draw, width: u32, height: u32, buffer: &mut [u32]) {
    let cx = ((draw.position[0] + 1.0) / 2.0 * width as f32) as i32;
    let cy = ((-draw.position[1] + 1.0) / 2.0 * height as f32) as i32;

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
// Main — Dashboard
// ---------------------------------------------------------------------------

fn main() {
    let mut ops: Vec<Op> = Vec::new();

    // Header bar with status badges
    ops.extend(header_bar(0.0, 0.85, 1.9, 0.15, [0.15, 0.22, 0.35, 1.0]));
    ops.extend(badge(-0.6, 0.85, 0.15, 0.06, [0.18, 0.80, 0.44, 1.0])); // green — "online"
    ops.extend(badge(-0.35, 0.85, 0.15, 0.06, [0.20, 0.60, 0.86, 1.0])); // blue — "synced"
    ops.extend(badge(-0.1, 0.85, 0.15, 0.06, [0.95, 0.77, 0.06, 1.0])); // yellow — "3 alerts"

    // Divider below header
    ops.extend(divider(0.0, 0.74, 1.9));

    // Left card — "Metrics"
    let left_children = {
        let mut c = Vec::new();
        c.extend(button(-0.5, 0.35, 0.3, 0.1, [0.20, 0.60, 0.86, 1.0])); // blue button
        c.extend(button(-0.5, 0.15, 0.3, 0.1, [0.18, 0.80, 0.44, 1.0])); // green button
        c
    };
    ops.extend(card(-0.5, 0.3, 0.85, 0.55, [0.12, 0.12, 0.18, 1.0], left_children));

    // Right card — "Actions"
    let right_children = {
        let mut c = Vec::new();
        c.extend(button(0.5, 0.35, 0.3, 0.1, [0.91, 0.30, 0.24, 1.0])); // red button
        c.extend(button(0.5, 0.15, 0.3, 0.1, [0.61, 0.35, 0.71, 1.0])); // purple button
        c
    };
    ops.extend(card(0.5, 0.3, 0.85, 0.55, [0.12, 0.12, 0.18, 1.0], right_children));

    // Bottom card — full width with badge row
    let bottom_children = {
        let mut c = Vec::new();
        let colors: [[f32; 4]; 5] = [
            [0.91, 0.30, 0.24, 1.0], // red
            [0.90, 0.49, 0.13, 1.0], // orange
            [0.95, 0.77, 0.06, 1.0], // yellow
            [0.18, 0.80, 0.44, 1.0], // green
            [0.20, 0.60, 0.86, 1.0], // blue
        ];
        for (i, color) in colors.iter().enumerate() {
            let bx = -0.6 + i as f32 * 0.3;
            c.extend(badge(bx, -0.45, 0.18, 0.08, *color));
        }
        c
    };
    ops.extend(card(0.0, -0.45, 1.8, 0.35, [0.10, 0.10, 0.14, 1.0], bottom_children));

    println!("Dashboard: {} ops", ops.len());

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("MatterStream — Dashboard (Components)")
                    .with_inner_size(winit::dpi::LogicalSize::new(800, 600)),
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
                    buffer.fill(0xFF0D0D12); // Very dark background

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
