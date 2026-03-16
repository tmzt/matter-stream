//! Multi-card view runner with draggable cards and text input.
//!
//! Loads a card session JSON file, compiles each card TSX independently,
//! and renders all cards at their session-specified positions.
//! Cards can be dragged with the mouse. Supports retina/HiDPI displays.
//! The prompt bar supports keyboard text entry and fires actions on Enter.
//!
//! Usage:
//!   cargo run --example run-cards --features compiler -- [--timeout <seconds>] <session.json>

use std::env;
use std::fs;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use matterstream::compile_to_asm;
use matterstream::arena::TripleArena;
use matterstream_vm::rpn::RpnVm;
use matterstream_vm::ui_vm::{render_ui_draws_with_font, UiDrawCmd};
use matterstream_packaging::fnta::builtin_font;
use softbuffer::{Context, Surface};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{EventLoop, ControlFlow};
use winit::keyboard::{Key, NamedKey};
use winit::window::Window;

/// A compiled card ready to render.
struct CompiledCard {
    id: String,
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
            UiDrawCmd::Action { x, y, w, h, str_idx } => UiDrawCmd::Action {
                x: s(x + dx),
                y: s(y + dy),
                w: su(*w),
                h: su(*h),
                str_idx: str_idx + str_offset,
            },
        })
        .collect()
}

/// Build merged draw list + string table from all cards at current positions.
/// For the prompt_bar card, replaces placeholder text with typed input.
fn build_draw_list(
    cards: &[CompiledCard],
    scale: f64,
    input_text: &str,
    input_focused: bool,
    pulse_alpha: u8,
) -> (Vec<UiDrawCmd>, Vec<String>) {
    let mut all_draws = Vec::new();
    let mut merged_strings = Vec::new();
    let s = |v: i32| -> i32 { (v as f64 * scale) as i32 };
    let su = |v: u32| -> u32 { (v as f64 * scale) as u32 };

    for card in cards {
        let str_offset = merged_strings.len() as u32;
        merged_strings.extend(card.string_table.iter().cloned());
        let offset_cmds = offset_scale_draws(
            &card.draws,
            card.x,
            card.y,
            scale,
            str_offset,
        );

        if card.id == "prompt_bar" {
            // Filter: skip the placeholder TextStr if input has text
            for cmd in &offset_cmds {
                if !input_text.is_empty() {
                    // Skip placeholder text (the "Ask anything..." TextStr)
                    if let UiDrawCmd::TextStr { str_idx, .. } = cmd {
                        if let Some(txt) = merged_strings.get(*str_idx as usize) {
                            if txt == "Ask anything..." {
                                continue;
                            }
                        }
                    }
                }
                // Apply pulse to voice indicator circles
                if let UiDrawCmd::Circle { x, y, r, color } = cmd {
                    // Inner circles get pulsing alpha
                    let base_alpha = (*color & 0xFF) as u8;
                    if base_alpha > 0 && *r < su(20) {
                        let pulsed = (base_alpha as u16 * pulse_alpha as u16 / 255) as u8;
                        let pulsed_color = (*color & 0xFFFFFF00) | pulsed as u32;
                        all_draws.push(UiDrawCmd::Circle {
                            x: *x,
                            y: *y,
                            r: *r,
                            color: pulsed_color,
                        });
                        continue;
                    }
                }
                all_draws.push(cmd.clone());
            }
            // Add typed text overlay
            if !input_text.is_empty() {
                let text_idx = merged_strings.len() as u32;
                merged_strings.push(input_text.to_string());
                all_draws.push(UiDrawCmd::TextStr {
                    x: s(card.x + 56),
                    y: s(card.y + 18),
                    size: su(16),
                    str_idx: text_idx,
                    color: 0xEEEEEEFF,
                });
            }
            // Add cursor blink
            if input_focused {
                let cursor_x_pos = card.x + 56 + input_text.len() as i32 * 10;
                all_draws.push(UiDrawCmd::Box {
                    x: s(cursor_x_pos),
                    y: s(card.y + 16),
                    w: su(2),
                    h: su(20),
                    color: if pulse_alpha > 128 { 0xAAAAAAFF } else { 0x00000000 },
                });
            }
        } else {
            all_draws.extend(offset_cmds);
        }
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
            UiDrawCmd::Action { x, y, w, h, .. } => (x + *w as i32, y + *h as i32),
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
            id: id.clone(),
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
    let mut dragging: Option<usize> = None;
    let mut drag_offset_x: f64 = 0.0;
    let mut drag_offset_y: f64 = 0.0;
    let mut cursor_x: f64 = 0.0;
    let mut cursor_y: f64 = 0.0;

    // Text input state
    let mut input_text = String::new();
    let mut input_focused = false;

    // Animation
    let start_time = Instant::now();

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
            // Animate at ~30fps when input is focused (for cursor blink + pulse)
            if input_focused {
                elwt.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(33),
                ));
                window.request_redraw();
            } else {
                elwt.set_control_flow(ControlFlow::Wait);
            }

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

                        // Compute pulse: sine wave 0..255 at ~1.5Hz
                        let elapsed = start_time.elapsed().as_secs_f64();
                        let pulse = ((elapsed * 1.5 * std::f64::consts::TAU).sin() * 0.5 + 0.5) * 255.0;
                        let pulse_alpha = pulse as u8;

                        let (all_draws, merged_strings) = build_draw_list(
                            &compiled_cards,
                            scale_factor,
                            &input_text,
                            input_focused,
                            pulse_alpha,
                        );

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
                                let cx = cursor_x as i32;
                                let cy = cursor_y as i32;

                                // Hit test cards in reverse order (last = top)
                                let mut hit = None;
                                for (i, card) in compiled_cards.iter().enumerate().rev() {
                                    if cx >= card.x
                                        && cx < card.x + card.w
                                        && cy >= card.y
                                        && cy < card.y + card.h
                                    {
                                        hit = Some(i);
                                        break;
                                    }
                                }

                                let mut clicked_prompt = false;
                                if let Some(idx) = hit {
                                    let card = &compiled_cards[idx];
                                    let local_x = cx - card.x;
                                    let local_y = cy - card.y;
                                    let mut fired_action = false;

                                    for cmd in &card.draws {
                                        if let UiDrawCmd::Action { x, y, w, h, str_idx } = cmd {
                                            if local_x >= *x
                                                && local_x < x + *w as i32
                                                && local_y >= *y
                                                && local_y < y + *h as i32
                                            {
                                                let action_name = card
                                                    .string_table
                                                    .get(*str_idx as usize)
                                                    .cloned()
                                                    .unwrap_or_default();

                                                if action_name == "prompt_input" {
                                                    input_focused = true;
                                                    clicked_prompt = true;
                                                } else if action_name == "prompt_send" {
                                                    if !input_text.is_empty() {
                                                        println!(
                                                            "[action] {{ \"action\": \"prompt_send\", \"text\": \"{}\" }}",
                                                            input_text.replace('\\', "\\\\").replace('"', "\\\"")
                                                        );
                                                        input_text.clear();
                                                    }
                                                    clicked_prompt = true;
                                                } else {
                                                    println!(
                                                        "[action] {{ \"action\": \"{}\", \"card\": \"{}\" }}",
                                                        action_name, card.id
                                                    );
                                                }
                                                fired_action = true;
                                                break;
                                            }
                                        }
                                    }

                                    if !fired_action && !clicked_prompt {
                                        drag_offset_x =
                                            cursor_x - compiled_cards[idx].x as f64;
                                        drag_offset_y =
                                            cursor_y - compiled_cards[idx].y as f64;
                                        dragging = Some(idx);
                                    }

                                    // Move clicked card to end (top of z-order)
                                    let card = compiled_cards.remove(idx);
                                    compiled_cards.push(card);
                                    if dragging == Some(idx) {
                                        dragging = Some(compiled_cards.len() - 1);
                                    }
                                    window.request_redraw();
                                }

                                // Clicking outside any card unfocuses input
                                if hit.is_none() {
                                    input_focused = false;
                                    window.request_redraw();
                                } else if !clicked_prompt {
                                    // Clicked a non-prompt card
                                    input_focused = false;
                                }
                            }
                            ElementState::Released => {
                                dragging = None;
                            }
                        }
                    }

                    WindowEvent::KeyboardInput { event, .. } => {
                        if input_focused && event.state == ElementState::Pressed {
                            match &event.logical_key {
                                Key::Named(NamedKey::Enter) => {
                                    if !input_text.is_empty() {
                                        println!(
                                            "[action] {{ \"action\": \"prompt_send\", \"text\": \"{}\" }}",
                                            input_text.replace('\\', "\\\\").replace('"', "\\\"")
                                        );
                                        input_text.clear();
                                        window.request_redraw();
                                    }
                                }
                                Key::Named(NamedKey::Backspace) => {
                                    input_text.pop();
                                    window.request_redraw();
                                }
                                Key::Named(NamedKey::Escape) => {
                                    input_focused = false;
                                    window.request_redraw();
                                }
                                Key::Character(ch) => {
                                    // Limit input length to fit the bar
                                    if input_text.len() < 68 {
                                        input_text.push_str(ch.as_str());
                                        window.request_redraw();
                                    }
                                }
                                _ => {}
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
