//! Multi-card view runner.
//!
//! Loads a card session JSON file, compiles each card TSX independently,
//! and renders all cards at their session-specified positions.
//!
//! Usage:
//!   cargo run --example run-cards --features compiler -- [--timeout <seconds>] <session.json>

use std::env;
use std::fs;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use matterstream::compile_to_asm;
use matterstream::arena::TripleArena;
use matterstream_vm::rpn::RpnVm;
use matterstream_vm::ui_vm::{render_ui_draws_with_font, UiDrawCmd};
use matterstream_packaging::fnta::builtin_font;
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{EventLoop, ControlFlow};
use winit::window::Window;

/// A compiled card ready to render.
struct CompiledCard {
    draws: Vec<UiDrawCmd>,
    string_table: Vec<String>,
    x: i32,
    y: i32,
}

/// Offset all draw commands by (dx, dy) and shift string indices by str_offset.
fn offset_draws(draws: &[UiDrawCmd], dx: i32, dy: i32, str_offset: u32) -> Vec<UiDrawCmd> {
    draws
        .iter()
        .map(|cmd| match cmd {
            UiDrawCmd::Box { x, y, w, h, color } => UiDrawCmd::Box {
                x: x + dx,
                y: y + dy,
                w: *w,
                h: *h,
                color: *color,
            },
            UiDrawCmd::Slab {
                x, y, w, h, radius, color,
            } => UiDrawCmd::Slab {
                x: x + dx,
                y: y + dy,
                w: *w,
                h: *h,
                radius: *radius,
                color: *color,
            },
            UiDrawCmd::Circle { x, y, r, color } => UiDrawCmd::Circle {
                x: x + dx,
                y: y + dy,
                r: *r,
                color: *color,
            },
            UiDrawCmd::Text { x, y, size, slot, color } => UiDrawCmd::Text {
                x: x + dx,
                y: y + dy,
                size: *size,
                slot: *slot,
                color: *color,
            },
            UiDrawCmd::TextStr {
                x, y, size, str_idx, color,
            } => UiDrawCmd::TextStr {
                x: x + dx,
                y: y + dy,
                size: *size,
                str_idx: str_idx + str_offset,
                color: *color,
            },
            UiDrawCmd::Line { x1, y1, x2, y2, color } => UiDrawCmd::Line {
                x1: x1 + dx,
                y1: y1 + dy,
                x2: x2 + dx,
                y2: y2 + dy,
                color: *color,
            },
        })
        .collect()
}

/// Minimal JSON parser for card session files.
/// Extracts card entries with id, file, x, y fields.
fn parse_session(json: &str) -> Vec<(String, String, i32, i32)> {
    let mut cards = Vec::new();
    // Find the "cards" array content
    let Some(arr_start) = json.find('[') else {
        return cards;
    };
    let Some(arr_end) = json.rfind(']') else {
        return cards;
    };
    let arr = &json[arr_start + 1..arr_end];

    // Split by object boundaries
    let mut depth = 0;
    let mut obj_start = None;
    for (i, ch) in arr.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    obj_start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = obj_start {
                        let obj = &arr[start..=i];
                        if let Some(card) = parse_card_obj(obj) {
                            cards.push(card);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    cards
}

fn parse_card_obj(obj: &str) -> Option<(String, String, i32, i32)> {
    let id = extract_str_field(obj, "id")?;
    let file = extract_str_field(obj, "file")?;
    let x = extract_num_field(obj, "x").unwrap_or(0);
    let y = extract_num_field(obj, "y").unwrap_or(0);
    Some((id, file, x, y))
}

fn extract_str_field(obj: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\"", field);
    let idx = obj.find(&pattern)?;
    let rest = &obj[idx + pattern.len()..];
    // Skip colon and whitespace
    let rest = rest.trim_start().strip_prefix(':')?;
    let rest = rest.trim_start();
    // Extract quoted string value
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn extract_num_field(obj: &str, field: &str) -> Option<i32> {
    let pattern = format!("\"{}\"", field);
    let idx = obj.find(&pattern)?;
    let rest = &obj[idx + pattern.len()..];
    let rest = rest.trim_start().strip_prefix(':')?;
    let rest = rest.trim_start();
    // Parse number (possibly negative)
    let end = rest.find(|c: char| !c.is_ascii_digit() && c != '-').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut timeout_s = None;
    let mut session_path = None;

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
                    session_path = Some(args[i].clone());
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

    let session_path = if let Some(path) = session_path {
        path
    } else {
        eprintln!(
            "Usage: cargo run --example run-cards --features compiler -- [--timeout <s>] <session.json>"
        );
        return;
    };

    let session_json = match fs::read_to_string(&session_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading session file '{}': {}", session_path, e);
            return;
        }
    };

    let session_dir = Path::new(&session_path)
        .parent()
        .unwrap_or(Path::new("."));

    let card_entries = parse_session(&session_json);
    if card_entries.is_empty() {
        eprintln!("No cards found in session file.");
        return;
    }

    // Compile each card
    let mut compiled_cards: Vec<CompiledCard> = Vec::new();

    for (id, file, x, y) in &card_entries {
        let card_path = session_dir.join(file);
        let code = match fs::read_to_string(&card_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading card '{}' at {:?}: {}", id, card_path, e);
                continue;
            }
        };

        let asm_output = match compile_to_asm(&code) {
            Ok(out) => out,
            Err(e) => {
                eprintln!("Error compiling card '{}': {}", id, e);
                continue;
            }
        };

        let mut arenas = TripleArena::new();
        let mut vm = RpnVm::new();
        vm.string_table = asm_output.string_table.clone();

        if let Err(e) = vm.execute(&asm_output.bytecode, &mut arenas) {
            eprintln!("VM error in card '{}': {:?}", id, e);
            continue;
        }

        println!(
            "  Card '{}': {} draws, {} strings",
            id,
            vm.ui_draws.len(),
            asm_output.string_table.len()
        );

        compiled_cards.push(CompiledCard {
            draws: vm.ui_draws,
            string_table: asm_output.string_table,
            x: *x,
            y: *y,
        });
    }

    println!(
        "=== Compiled {} cards ===",
        compiled_cards.len()
    );

    // Merge all draw commands with offsets into a single draw list + merged string table
    let mut all_draws: Vec<UiDrawCmd> = Vec::new();
    let mut merged_strings: Vec<String> = Vec::new();

    for card in &compiled_cards {
        let str_offset = merged_strings.len() as u32;
        merged_strings.extend(card.string_table.iter().cloned());
        let offset_cmds = offset_draws(&card.draws, card.x, card.y, str_offset);
        all_draws.extend(offset_cmds);
    }

    let font = builtin_font();

    // --- Window setup ---
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes().with_title("MatterStream — Card Session"),
            )
            .unwrap(),
    );

    let context = Context::new(window.clone()).unwrap();
    let mut surface = Surface::new(&context, window.clone()).unwrap();

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    let (width, height) = {
                        let size = window.inner_size();
                        (size.width.max(1), size.height.max(1))
                    };
                    surface
                        .resize(
                            NonZeroU32::new(width).unwrap(),
                            NonZeroU32::new(height).unwrap(),
                        )
                        .unwrap();

                    let mut buffer = surface.buffer_mut().unwrap();
                    buffer.fill(0x00111118); // Dark background

                    render_ui_draws_with_font(
                        &all_draws,
                        &mut buffer,
                        width,
                        height,
                        &merged_strings,
                        Some(&font),
                    );

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
