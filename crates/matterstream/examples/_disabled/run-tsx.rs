use std::env;
use std::fs;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use matterstream::compile_to_asm;
use matterstream::arena::TripleArena;
use matterstream_vm::rpn::RpnVm;
use matterstream_vm::ui_vm::render_ui_draws_with_font;
use matterstream_packaging::fnta::builtin_font;
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{EventLoop, ControlFlow};
use winit::window::Window;

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
        eprintln!("Usage: cargo run --example run-tsx -- [--timeout <seconds>] <file.tsx>");
        return;
    };

    let code = match fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", file_path, e);
            return;
        }
    };

    let asm_output = match compile_to_asm(&code) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("Error compiling tsx file: {}", e);
            return;
        }
    };

    // Execute bytecode through RPN VM
    let mut arenas = TripleArena::new();
    let mut vm = RpnVm::new();
    vm.string_table = asm_output.string_table.clone();

    if let Err(e) = vm.execute(&asm_output.bytecode, &mut arenas) {
        eprintln!("VM execution error: {:?}", e);
        return;
    }

    let draws = vm.ui_draws.clone();
    let string_table = asm_output.string_table;
    let font = builtin_font();

    // --- Winit and Softbuffer setup ---
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(event_loop.create_window(Window::default_attributes().with_title("MatterStream Output")).unwrap());

    let context = Context::new(window.clone()).unwrap();
    let mut surface = Surface::new(&context, window.clone()).unwrap();

    event_loop.run(move |event, elwt| {
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
                buffer.fill(0xFF181818); // Dark gray background

                render_ui_draws_with_font(
                    &draws,
                    &mut buffer,
                    width,
                    height,
                    &string_table,
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
    }).unwrap();
}
