//! Example: Load a .tsx file, compile it, and render in a native window.
//!
//! Run with:
//!   cargo run -p matterstream --example run-tsx -- crates/matterstream/examples/example.tsx
//!   cargo run -p matterstream --example run-tsx -- --timeout 5 crates/matterstream/examples/login_form.tsx

use std::env;
use std::fs;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use matterstream::{Compiler, Draw, MatterStream, OpsHeader, Primitive, Parser, RsiPointer, BankId};
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

// ---------------------------------------------------------------------------
// Minimal 5x7 bitmap font for ASCII 32..127
// Each glyph is 5 columns x 7 rows, packed as [u8; 5] (one byte per column,
// bit 0 = top row). Only printable ASCII is covered.
// ---------------------------------------------------------------------------

const GLYPH_W: i32 = 5;
const GLYPH_H: i32 = 7;
const GLYPH_SPACING: i32 = 1;

fn glyph_bitmap(ch: char) -> [u8; 5] {
    match ch {
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00],
        '!' => [0x00, 0x00, 0x5F, 0x00, 0x00],
        '"' => [0x00, 0x07, 0x00, 0x07, 0x00],
        '#' => [0x14, 0x7F, 0x14, 0x7F, 0x14],
        '$' => [0x24, 0x2A, 0x7F, 0x2A, 0x12],
        '%' => [0x23, 0x13, 0x08, 0x64, 0x62],
        '&' => [0x36, 0x49, 0x55, 0x22, 0x50],
        '\'' => [0x00, 0x05, 0x03, 0x00, 0x00],
        '(' => [0x00, 0x1C, 0x22, 0x41, 0x00],
        ')' => [0x00, 0x41, 0x22, 0x1C, 0x00],
        '*' => [0x14, 0x08, 0x3E, 0x08, 0x14],
        '+' => [0x08, 0x08, 0x3E, 0x08, 0x08],
        ',' => [0x00, 0x50, 0x30, 0x00, 0x00],
        '-' => [0x08, 0x08, 0x08, 0x08, 0x08],
        '.' => [0x00, 0x60, 0x60, 0x00, 0x00],
        '/' => [0x20, 0x10, 0x08, 0x04, 0x02],
        '0' => [0x3E, 0x51, 0x49, 0x45, 0x3E],
        '1' => [0x00, 0x42, 0x7F, 0x40, 0x00],
        '2' => [0x42, 0x61, 0x51, 0x49, 0x46],
        '3' => [0x21, 0x41, 0x45, 0x4B, 0x31],
        '4' => [0x18, 0x14, 0x12, 0x7F, 0x10],
        '5' => [0x27, 0x45, 0x45, 0x45, 0x39],
        '6' => [0x3C, 0x4A, 0x49, 0x49, 0x30],
        '7' => [0x01, 0x71, 0x09, 0x05, 0x03],
        '8' => [0x36, 0x49, 0x49, 0x49, 0x36],
        '9' => [0x06, 0x49, 0x49, 0x29, 0x1E],
        ':' => [0x00, 0x36, 0x36, 0x00, 0x00],
        ';' => [0x00, 0x56, 0x36, 0x00, 0x00],
        '<' => [0x08, 0x14, 0x22, 0x41, 0x00],
        '=' => [0x14, 0x14, 0x14, 0x14, 0x14],
        '>' => [0x00, 0x41, 0x22, 0x14, 0x08],
        '?' => [0x02, 0x01, 0x51, 0x09, 0x06],
        '@' => [0x32, 0x49, 0x79, 0x41, 0x3E],
        'A' => [0x7E, 0x11, 0x11, 0x11, 0x7E],
        'B' => [0x7F, 0x49, 0x49, 0x49, 0x36],
        'C' => [0x3E, 0x41, 0x41, 0x41, 0x22],
        'D' => [0x7F, 0x41, 0x41, 0x22, 0x1C],
        'E' => [0x7F, 0x49, 0x49, 0x49, 0x41],
        'F' => [0x7F, 0x09, 0x09, 0x09, 0x01],
        'G' => [0x3E, 0x41, 0x49, 0x49, 0x7A],
        'H' => [0x7F, 0x08, 0x08, 0x08, 0x7F],
        'I' => [0x00, 0x41, 0x7F, 0x41, 0x00],
        'J' => [0x20, 0x40, 0x41, 0x3F, 0x01],
        'K' => [0x7F, 0x08, 0x14, 0x22, 0x41],
        'L' => [0x7F, 0x40, 0x40, 0x40, 0x40],
        'M' => [0x7F, 0x02, 0x0C, 0x02, 0x7F],
        'N' => [0x7F, 0x04, 0x08, 0x10, 0x7F],
        'O' => [0x3E, 0x41, 0x41, 0x41, 0x3E],
        'P' => [0x7F, 0x09, 0x09, 0x09, 0x06],
        'Q' => [0x3E, 0x41, 0x51, 0x21, 0x5E],
        'R' => [0x7F, 0x09, 0x19, 0x29, 0x46],
        'S' => [0x46, 0x49, 0x49, 0x49, 0x31],
        'T' => [0x01, 0x01, 0x7F, 0x01, 0x01],
        'U' => [0x3F, 0x40, 0x40, 0x40, 0x3F],
        'V' => [0x1F, 0x20, 0x40, 0x20, 0x1F],
        'W' => [0x3F, 0x40, 0x38, 0x40, 0x3F],
        'X' => [0x63, 0x14, 0x08, 0x14, 0x63],
        'Y' => [0x07, 0x08, 0x70, 0x08, 0x07],
        'Z' => [0x61, 0x51, 0x49, 0x45, 0x43],
        'a' => [0x20, 0x54, 0x54, 0x54, 0x78],
        'b' => [0x7F, 0x48, 0x44, 0x44, 0x38],
        'c' => [0x38, 0x44, 0x44, 0x44, 0x20],
        'd' => [0x38, 0x44, 0x44, 0x48, 0x7F],
        'e' => [0x38, 0x54, 0x54, 0x54, 0x18],
        'f' => [0x08, 0x7E, 0x09, 0x01, 0x02],
        'g' => [0x0C, 0x52, 0x52, 0x52, 0x3E],
        'h' => [0x7F, 0x08, 0x04, 0x04, 0x78],
        'i' => [0x00, 0x44, 0x7D, 0x40, 0x00],
        'j' => [0x20, 0x40, 0x44, 0x3D, 0x00],
        'k' => [0x7F, 0x10, 0x28, 0x44, 0x00],
        'l' => [0x00, 0x41, 0x7F, 0x40, 0x00],
        'm' => [0x7C, 0x04, 0x18, 0x04, 0x78],
        'n' => [0x7C, 0x08, 0x04, 0x04, 0x78],
        'o' => [0x38, 0x44, 0x44, 0x44, 0x38],
        'p' => [0x7C, 0x14, 0x14, 0x14, 0x08],
        'q' => [0x08, 0x14, 0x14, 0x18, 0x7C],
        'r' => [0x7C, 0x08, 0x04, 0x04, 0x08],
        's' => [0x48, 0x54, 0x54, 0x54, 0x20],
        't' => [0x04, 0x3F, 0x44, 0x40, 0x20],
        'u' => [0x3C, 0x40, 0x40, 0x20, 0x7C],
        'v' => [0x1C, 0x20, 0x40, 0x20, 0x1C],
        'w' => [0x3C, 0x40, 0x30, 0x40, 0x3C],
        'x' => [0x44, 0x28, 0x10, 0x28, 0x44],
        'y' => [0x0C, 0x50, 0x50, 0x50, 0x3C],
        'z' => [0x44, 0x64, 0x54, 0x4C, 0x44],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00],
    }
}

/// Draw a text string into the buffer at pixel position (px, py), scaled by `scale`.
fn draw_text(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    text: &str,
    px: i32,
    py: i32,
    scale: i32,
    color: u32,
) {
    let char_w = (GLYPH_W + GLYPH_SPACING) * scale;
    let total_w = text.len() as i32 * char_w - GLYPH_SPACING * scale;
    // Center the text block on (px, py)
    let start_x = px - total_w / 2;
    let start_y = py - (GLYPH_H * scale) / 2;

    for (ci, ch) in text.chars().enumerate() {
        let bitmap = glyph_bitmap(ch);
        let ox = start_x + ci as i32 * char_w;
        for col in 0..GLYPH_W {
            let bits = bitmap[col as usize];
            for row in 0..GLYPH_H {
                if bits & (1 << row) != 0 {
                    // Draw a scale x scale block for each pixel
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let fx = ox + col * scale + sx;
                            let fy = start_y + row * scale + sy;
                            if fx >= 0 && fx < width as i32 && fy >= 0 && fy < height as i32 {
                                buffer[(fy * width as i32 + fx) as usize] = color;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a single draw call (slab rectangle and/or label text).
fn render_draw(draw: &Draw, width: u32, height: u32, buffer: &mut [u32]) {
    let cx = ((draw.position[0] + 1.0) / 2.0 * width as f32) as i32;
    let cy = ((-draw.position[1] + 1.0) / 2.0 * height as f32) as i32;

    let r = (draw.color[0] * 255.0) as u32;
    let g = (draw.color[1] * 255.0) as u32;
    let b = (draw.color[2] * 255.0) as u32;
    let a = (draw.color[3] * 255.0) as u32;
    let color_u32 = (a << 24) | (r << 16) | (g << 8) | b;

    // Draw slab rectangle (skip for pure Text primitives with no size)
    let is_text_only = draw.primitive == Primitive::Text
        && draw.size[0] == 0.0
        && draw.size[1] == 0.0;

    if !is_text_only {
        let (sw, sh) = if draw.size[0] > 0.0 || draw.size[1] > 0.0 {
            (
                (draw.size[0] * width as f32) as i32,
                (draw.size[1] * height as f32) as i32,
            )
        } else {
            let w = (width as f32 * 0.6) as i32;
            let h = (height as f32 * 0.1) as i32;
            (w, h)
        };

        let x0 = cx - sw / 2;
        let y0 = cy - sh / 2;
        for py in y0..(y0 + sh) {
            for px in x0..(x0 + sw) {
                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                    buffer[(py * width as i32 + px) as usize] = color_u32;
                }
            }
        }
    }

    // Draw label text if present
    if let Some(label) = &draw.label {
        let scale = ((height as f32 * 0.004).max(1.0)) as i32;
        // For text-only draws, use the draw color; for labeled slabs, use white
        let text_color = if is_text_only {
            color_u32
        } else {
            0xFFFFFFFF
        };
        draw_text(buffer, width, height, label, cx, cy, scale, text_color);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut timeout_s = None;
    let mut file_path = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--timeout" => {
                if i + 1 < args.len() {
                    timeout_s = args[i + 1].parse().ok();
                    i += 2;
                } else {
                    eprintln!("--timeout requires a value");
                    return;
                }
            }
            _ => {
                if args[i].starts_with('-') {
                    eprintln!("Unknown flag: {}", args[i]);
                } else {
                    file_path = Some(args[i].clone());
                }
                i += 1;
            }
        }
    }

    if let Some(seconds) = timeout_s {
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(seconds));
            println!("Timeout reached, exiting.");
            std::process::exit(0);
        });
    }

    let file_path = if let Some(path) = file_path {
        path
    } else {
        eprintln!(
            "Usage: cargo run -p matterstream --example run-tsx -- [--timeout <seconds>] <file.tsx>"
        );
        return;
    };

    let code = match fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading tsx file '{}': {}", file_path, e);
            return;
        }
    };

    let _parsed = match Parser::parse(&code) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error parsing tsx file: {}", e);
            return;
        }
    };

    let compiled = match Compiler::compile(&code) {
        Ok(ops) => ops,
        Err(e) => {
            eprintln!("Error compiling tsx file: {}", e);
            return;
        }
    };

    // --- Winit and Softbuffer setup ---
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(Window::default_attributes().with_title("MatterStream Output"))
            .unwrap(),
    );

    let context = Context::new(window.clone()).unwrap();
    let mut surface = Surface::new(&context, window.clone()).unwrap();

    // --- MatterStream setup ---
    let mut stream = MatterStream::new();
    let header = OpsHeader::new(vec![RsiPointer::new(1, BankId::Vec3 as u8, 0)], false);

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            window.request_redraw();

            match event {
                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    // --- MatterStream Execution ---
                    smol::block_on(async {
                        if let Err(errors) = stream.execute(&header, &compiled.ops).await {
                            eprintln!("MatterStream execution failed with errors:");
                            for error in errors {
                                eprintln!("- {:?}", error);
                            }
                        }
                    });

                    // --- Rendering with Softbuffer ---
                    let (width, height) = {
                        let size = window.inner_size();
                        (size.width, size.height)
                    };
                    if width == 0 || height == 0 { return; }

                    surface
                        .resize(
                            NonZeroU32::new(width).unwrap(),
                            NonZeroU32::new(height).unwrap(),
                        )
                        .unwrap();

                    let mut buffer = surface.buffer_mut().unwrap();
                    buffer.fill(0xFF181818);

                    for draw in &stream.draws {
                        render_draw(draw, width, height, &mut buffer);
                    }

                    buffer.present().unwrap();
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    elwt.exit();
                }
                _ => (),
            }
        })
        .unwrap();
}
