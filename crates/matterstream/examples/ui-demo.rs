//! UI draw opcode demo — renders colored shapes via RPN VM + softbuffer.
//!
//! Constructs RPN bytecode using UI opcodes, executes via RpnVm,
//! then rasterizes the draw list into a softbuffer window.
//!
//! Usage:
//!   cargo run --example ui-demo [-- --timeout <seconds>]

use std::env;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use matterstream::arena::TripleArena;
use matterstream::rpn::{RpnOp, RpnVm};
use matterstream::ui_vm::{render_ui_draws, rgba};
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

fn encode_push32(val: u32) -> Vec<u8> {
    let mut buf = vec![RpnOp::Push32 as u8];
    buf.extend_from_slice(&val.to_le_bytes());
    buf
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut timeout_s = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--timeout" && i + 1 < args.len() {
            timeout_s = args[i + 1].parse().ok();
            i += 2;
        } else {
            i += 1;
        }
    }

    if let Some(seconds) = timeout_s {
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(seconds));
            println!("Timeout reached, exiting.");
            std::process::exit(0);
        });
    }

    // ================================================================
    // Build RPN bytecode with UI draw opcodes
    // ================================================================
    println!("=== Building UI bytecode ===");

    let mut bc = Vec::new();

    // Dark blue background box
    bc.extend_from_slice(&encode_push32(rgba(13, 17, 23, 255)));
    bc.push(RpnOp::UiSetColor as u8);
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(800));
    bc.extend_from_slice(&encode_push32(600));
    bc.push(RpnOp::UiBox as u8);

    // Red box
    bc.extend_from_slice(&encode_push32(rgba(220, 50, 50, 255)));
    bc.push(RpnOp::UiSetColor as u8);
    bc.extend_from_slice(&encode_push32(50));
    bc.extend_from_slice(&encode_push32(50));
    bc.extend_from_slice(&encode_push32(200));
    bc.extend_from_slice(&encode_push32(150));
    bc.push(RpnOp::UiBox as u8);

    // Green rounded rect (slab)
    bc.extend_from_slice(&encode_push32(rgba(50, 200, 80, 255)));
    bc.push(RpnOp::UiSetColor as u8);
    bc.extend_from_slice(&encode_push32(300));
    bc.extend_from_slice(&encode_push32(50));
    bc.extend_from_slice(&encode_push32(200));
    bc.extend_from_slice(&encode_push32(150));
    bc.extend_from_slice(&encode_push32(20));
    bc.push(RpnOp::UiSlab as u8);

    // Blue circle
    bc.extend_from_slice(&encode_push32(rgba(60, 100, 240, 255)));
    bc.push(RpnOp::UiSetColor as u8);
    bc.extend_from_slice(&encode_push32(150));
    bc.extend_from_slice(&encode_push32(350));
    bc.extend_from_slice(&encode_push32(80));
    bc.push(RpnOp::UiCircle as u8);

    // Semi-transparent yellow circle overlapping
    bc.extend_from_slice(&encode_push32(rgba(255, 220, 50, 160)));
    bc.push(RpnOp::UiSetColor as u8);
    bc.extend_from_slice(&encode_push32(200));
    bc.extend_from_slice(&encode_push32(320));
    bc.extend_from_slice(&encode_push32(60));
    bc.push(RpnOp::UiCircle as u8);

    // Push state, set offset, draw box, pop state
    bc.push(RpnOp::UiPushState as u8);
    bc.extend_from_slice(&encode_push32(rgba(200, 100, 255, 200)));
    bc.push(RpnOp::UiSetColor as u8);
    bc.extend_from_slice(&encode_push32(400));
    bc.extend_from_slice(&encode_push32(250));
    bc.push(RpnOp::UiSetOffset as u8);
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(150));
    bc.extend_from_slice(&encode_push32(100));
    bc.push(RpnOp::UiBox as u8);
    bc.push(RpnOp::UiPopState as u8);

    // White diagonal line
    bc.extend_from_slice(&encode_push32(rgba(255, 255, 255, 255)));
    bc.push(RpnOp::UiSetColor as u8);
    bc.extend_from_slice(&encode_push32(600));
    bc.extend_from_slice(&encode_push32(50));
    bc.extend_from_slice(&encode_push32(750));
    bc.extend_from_slice(&encode_push32(250));
    bc.push(RpnOp::UiLine as u8);

    // Text placeholder
    bc.extend_from_slice(&encode_push32(rgba(200, 200, 200, 255)));
    bc.push(RpnOp::UiSetColor as u8);
    bc.extend_from_slice(&encode_push32(550));
    bc.extend_from_slice(&encode_push32(400));
    bc.extend_from_slice(&encode_push32(14));
    bc.extend_from_slice(&encode_push32(0));
    bc.push(RpnOp::UiText as u8);

    bc.push(RpnOp::Halt as u8);

    // ================================================================
    // Execute RPN VM
    // ================================================================
    println!("=== Executing RPN VM ===");

    let mut arenas = TripleArena::new();
    let mut vm = RpnVm::new();
    let trace = vm.execute_metered(&bc, &mut arenas).unwrap();

    println!(
        "  Opcodes: {}, Gas: {}, Draw cmds: {}",
        trace.opcodes_executed,
        trace.gas_consumed,
        vm.ui_draws.len()
    );

    let draws = vm.ui_draws.clone();

    // ================================================================
    // Render in window
    // ================================================================
    println!("=== Rendering ===");

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("STACKVM UI Demo — RPN VM + softbuffer"),
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
                    buffer.fill(0x00000000);

                    render_ui_draws(&draws, &mut buffer, width, height);

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
