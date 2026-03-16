//! Multi-card view runner with draggable cards.
//!
//! Loads a card session JSON file, compiles each card TSX independently,
//! and renders all cards at their session-specified positions.
//! Cards can be dragged with the mouse. Supports retina/HiDPI displays.
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
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{EventLoop, ControlFlow};
use winit::window::Window;

/// A compiled card ready to render.
struct CompiledCard {
    draws: Vec<UiDrawCmd>,
    string_table: Vec<String>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

/// Offset all draw commands by (dx, dy), scale by factor, and shift string indices.
fn offset_scale_draws(
    draws: &[UiDrawCmd],
    dx: i32,
    dy: i32,
    scale: f64,
    str_offset: u32,
) -> Vec<UiDrawCmd> {
    let s = |v: i32| -> i32 { (v as f64 * scale) as i32 };
    let su = |v: u32| -> u32 { (v as f64 * scale) as u32 };
    draws
        .iter()
        .map(|cmd| match cmd {
            UiDrawCmd::Box { x, y, w, h, color } => UiDrawCmd::Box {
                x: s(x + dx),
                y: s(y + dy),
                w: su(*w),
                h: su(*h),
                color: *color,
            },
            UiDrawCmd::Slab { x, y, w, h, radius, color } => UiDrawCmd::Slab {
                x: s(x + dx),
                y: s(y + dy),
                w: su(*w),
                h: su(*h),
                radius: su(*radius),
                color: *color,
            },
            UiDrawCmd::Circle { x, y, r, color } => UiDrawCmd::Circle {
                x: s(x + dx),
                y: s(y + dy),
                r: su(*r),
                color: *color,
            },
            UiDrawCmd::Text { x, y, size, slot, color } => UiDrawCmd::Text {
                x: s(x + dx),
                y: s(y + dy),
                size: su(*size),
                slot: *slot,
                color: *color,
            },
            UiDrawCmd::TextStr { x, y, size, str_idx, color } => UiDrawCmd::TextStr {
                x: s(x + dx),
                y: s(y + dy),
                size: su(*size),
                str_idx: str_idx + str_offset,
                color: *color,
            },
            UiDrawCmd::Line { x1, y1, x2, y2, color } => UiDrawCmd::Line {
                x1: s(x1 + dx),
                y1: s(y1 + dy),
                x2: s(x2 + dx),
                y2: s(y2 + dy),
                color: *color,
            },
        })
        .collect()
}

/// Build merged draw list + string table from all cards at current positions.
fn build_draw_list(
    cards: &[CompiledCard],
    scale: f64,
) -> (Vec<UiDrawCmd>, Vec<String>) {
    let mut all_draws = Vec::new();
    let mut merged_strings = Vec::new();
    for card in cards {
        let str_offset = merged_strings.len() as u32;
        merged_strings.extend(card.string_table.iter().cloned());
        all_draws.extend(offset_scale_draws(
            &card.draws,
            card.x,
            card.y,
            scale,
            str_offset,
        ));
    }
    (all_draws, merged_strings)
}

/// Minimal JSON parser for card session files.
fn parse_session(json: &str) -> Vec<(String, String, i32, i32)> {
    let mut cards = Vec::new();
    let Some(arr_start) = json.find('[') else { return cards };
    let Some(arr_end) = json.rfind(']') else { return cards };
    let arr = &json[arr_start + 1..arr_end];

    let mut depth = 0;
    let mut obj_start = None;
    for (i, ch) in arr.char_indices() {
        match ch {
            '{' => {
                if depth == 0 { obj_start = Some(i); }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = obj_start {
                        if let Some(card) = parse_card_obj(&arr[start..=i]) {
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
    let rest = rest.trim_start().strip_prefix(':')?;
    let rest = rest.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn extract_num_field(obj: &str, field: &str) -> Option<i32> {
    let pattern = format!("\"{}\"", field);
    let idx = obj.find(&pattern)?;
    let rest = &obj[idx + pattern.len()..];
    let rest = rest.trim_start().strip_prefix(':')?;
    let rest = rest.trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit() && c != '-').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Compute bounding box (w, h) from a card's draw commands.
fn card_bounds(draws: &[UiDrawCmd]) -> (i32, i32) {
    let mut max_x: i32 = 0;
    let mut max_y: i32 = 0;
    for cmd in draws {
        let (right, bottom) = match cmd {
            UiDrawCmd::Box { x, y, w, h, .. }
            | UiDrawCmd::Slab { x, y, w, h, .. } => (x + *w as i32, y + *h as i32),
            UiDrawCmd::Circle { x, y, r, .. } => (x + *r as i32, y + *r as i32),
            UiDrawCmd::Text { x, y, size, .. }
            | UiDrawCmd::TextStr { x, y, size, .. } => (x + *size as i32 * 4, y + *size as i32),
            UiDrawCmd::Line { x1, y1, x2, y2, .. } => {
                ((*x1).max(*x2), (*y1).max(*y2))
            }
        };
        max_x = max_x.max(right);
        max_y = max_y.max(bottom);
    }
    (max_x, max_y)
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

        let (bw, bh) = card_bounds(&vm.ui_draws);

        println!(
            "  Card '{}': {} draws, {}x{} bounds",
            id,
            vm.ui_draws.len(),
            bw,
            bh,
        );

        compiled_cards.push(CompiledCard {
            draws: vm.ui_draws,
            string_table: asm_output.string_table,
            x: *x,
            y: *y,
            w: bw,
            h: bh,
        });
    }

    println!("=== Compiled {} cards ===", compiled_cards.len());

    let font = builtin_font();

    // Drag state
    let mut dragging: Option<usize> = None; // index of card being dragged
    let mut drag_offset_x: f64 = 0.0;
    let mut drag_offset_y: f64 = 0.0;
    let mut cursor_x: f64 = 0.0;
    let mut cursor_y: f64 = 0.0;
    // --- Window setup ---
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("MatterStream — Card Session")
                    .with_inner_size(LogicalSize::new(1280.0, 800.0)),
            )
            .unwrap(),
    );

    let mut scale_factor: f64 = window.scale_factor();
    println!("  Scale factor: {:.1}x", scale_factor);

    let context = Context::new(window.clone()).unwrap();
    let mut surface = Surface::new(&context, window.clone()).unwrap();

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                Event::WindowEvent { event: ref win_event, .. } => match win_event {
                    WindowEvent::RedrawRequested => {
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

                        let (all_draws, merged_strings) =
                            build_draw_list(&compiled_cards, scale_factor);

                        let mut buffer = surface.buffer_mut().unwrap();
                        buffer.fill(0x00111118);

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

                    WindowEvent::ScaleFactorChanged { scale_factor: new_scale, .. } => {
                        scale_factor = *new_scale;
                    }

                    WindowEvent::CursorMoved { position, .. } => {
                        // Convert physical to logical coordinates
                        cursor_x = position.x / scale_factor;
                        cursor_y = position.y / scale_factor;

                        if let Some(idx) = dragging {
                            compiled_cards[idx].x = (cursor_x - drag_offset_x) as i32;
                            compiled_cards[idx].y = (cursor_y - drag_offset_y) as i32;
                            window.request_redraw();
                        }
                    }

                    WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                        match state {
                            ElementState::Pressed => {
                                // Hit test cards in reverse order (last = top)
                                let mut hit = None;
                                for (i, card) in compiled_cards.iter().enumerate().rev() {
                                    let cx = cursor_x as i32;
                                    let cy = cursor_y as i32;
                                    if cx >= card.x
                                        && cx < card.x + card.w
                                        && cy >= card.y
                                        && cy < card.y + card.h
                                    {
                                        hit = Some(i);
                                        break;
                                    }
                                }
                                if let Some(idx) = hit {
                                    drag_offset_x = cursor_x - compiled_cards[idx].x as f64;
                                    drag_offset_y = cursor_y - compiled_cards[idx].y as f64;
                                    dragging = Some(idx);
                                    // Move dragged card to end (top of z-order)
                                    let card = compiled_cards.remove(idx);
                                    compiled_cards.push(card);
                                    dragging = Some(compiled_cards.len() - 1);
                                    window.request_redraw();
                                }
                            }
                            ElementState::Released => {
                                dragging = None;
                            }
                        }
                    }

                    WindowEvent::CloseRequested => {
                        elwt.exit();
                    }

                    _ => (),
                },
                _ => (),
            }
        })
        .unwrap();
}
