//! RPN VM + Arena visualization example.
//!
//! Computes Fibonacci numbers via RPN bytecode using arena memory,
//! then renders the results as a bar chart in a window.
//!
//! Usage:
//!   cargo run --example rpn-arena [-- --timeout <seconds>]

use std::env;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use matterstream::arena::TripleArena;
use matterstream::rpn::{RpnOp, RpnVm};
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

fn encode_push32(val: u32) -> Vec<u8> {
    let mut buf = vec![RpnOp::Push32 as u8];
    buf.extend_from_slice(&val.to_le_bytes());
    buf
}

fn encode_push64(val: u64) -> Vec<u8> {
    let mut buf = vec![RpnOp::Push64 as u8];
    buf.extend_from_slice(&val.to_le_bytes());
    buf
}

fn encode_jmp(target: u64) -> Vec<u8> {
    let mut buf = vec![RpnOp::Jmp as u8];
    buf.extend_from_slice(&target.to_le_bytes());
    buf
}

fn encode_jmpif(target: u64) -> Vec<u8> {
    let mut buf = vec![RpnOp::JmpIf as u8];
    buf.extend_from_slice(&target.to_le_bytes());
    buf
}

/// Build RPN bytecode that computes fib(n) using arena memory.
/// After execution, the result is on top of the VM stack.
fn build_fib_bytecode(
    arenas: &mut TripleArena,
    n: u32,
) -> (Vec<u8>, matterstream::Ova) {
    let ova_n = arenas.alloc_nursery(4).unwrap();
    let ova_a = arenas.alloc_nursery(4).unwrap();
    let ova_b = arenas.alloc_nursery(4).unwrap();

    let mut bc = Vec::new();

    // Store n
    bc.extend_from_slice(&encode_push64(n as u64));
    bc.extend_from_slice(&encode_push32(ova_n.0));
    bc.push(RpnOp::Store as u8);

    // Store a=0
    bc.extend_from_slice(&encode_push64(0));
    bc.extend_from_slice(&encode_push32(ova_a.0));
    bc.push(RpnOp::Store as u8);

    // Store b=1
    bc.extend_from_slice(&encode_push64(1));
    bc.extend_from_slice(&encode_push32(ova_b.0));
    bc.push(RpnOp::Store as u8);

    let loop_start = bc.len();

    // Load n, check > 0
    bc.extend_from_slice(&encode_push32(ova_n.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push64(0));
    bc.push(RpnOp::CmpGt as u8);

    let jmpif_pos = bc.len();
    bc.extend_from_slice(&encode_jmpif(0)); // placeholder

    let jmp_end_pos = bc.len();
    bc.extend_from_slice(&encode_jmp(0)); // placeholder

    let body_start = bc.len();

    // next = a + b
    bc.extend_from_slice(&encode_push32(ova_a.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push32(ova_b.0));
    bc.push(RpnOp::Load as u8);
    bc.push(RpnOp::Add as u8);

    // a = b
    bc.extend_from_slice(&encode_push32(ova_b.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push32(ova_a.0));
    bc.push(RpnOp::Store as u8);

    // b = next
    bc.extend_from_slice(&encode_push32(ova_b.0));
    bc.push(RpnOp::Store as u8);

    // n -= 1
    bc.extend_from_slice(&encode_push32(ova_n.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push64(1));
    bc.push(RpnOp::Sub as u8);
    bc.extend_from_slice(&encode_push32(ova_n.0));
    bc.push(RpnOp::Store as u8);

    bc.extend_from_slice(&encode_jmp(loop_start as u64));

    let loop_end = bc.len();

    // Load result
    bc.extend_from_slice(&encode_push32(ova_b.0));
    bc.push(RpnOp::Load as u8);
    bc.push(RpnOp::Halt as u8);

    // Fix up targets
    let body_bytes = (body_start as u64).to_le_bytes();
    bc[jmpif_pos + 1..jmpif_pos + 9].copy_from_slice(&body_bytes);
    let end_bytes = (loop_end as u64).to_le_bytes();
    bc[jmp_end_pos + 1..jmp_end_pos + 9].copy_from_slice(&end_bytes);

    (bc, ova_b)
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

    // Compute fib(1) through fib(15) using the RPN VM
    let mut fib_values: Vec<u64> = Vec::new();
    for n in 1..=15 {
        let mut arenas = TripleArena::new();
        let (bytecode, _ova_result) = build_fib_bytecode(&mut arenas, n);
        let mut vm = RpnVm::new();
        let trace = vm.execute_metered(&bytecode, &mut arenas).unwrap();
        let result = vm.stack.last().and_then(|v| v.as_u64()).unwrap_or(0);
        println!(
            "fib({:2}) = {:6}  [gas: {:5}, opcodes: {:4}, backward_jumps: {:3}]",
            n, result, trace.gas_consumed, trace.opcodes_executed, trace.backward_jumps
        );
        fib_values.push(result);
    }

    // Disassemble the last bytecode for display
    {
        let mut arenas = TripleArena::new();
        let (bytecode, _) = build_fib_bytecode(&mut arenas, 10);
        println!("\nDisassembly of fib(10):");
        println!("{}", RpnVm::disassemble(&bytecode).unwrap());
    }

    // --- Window rendering ---
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes().with_title("RPN Arena - Fibonacci Bar Chart"),
            )
            .unwrap(),
    );

    let context = Context::new(window.clone()).unwrap();
    let mut surface = Surface::new(&context, window.clone()).unwrap();

    let max_fib = *fib_values.iter().max().unwrap_or(&1) as f64;

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
                    buffer.fill(0xFF1a1a2e); // dark navy background

                    let bar_count = fib_values.len();
                    let margin = 20u32;
                    let chart_w = width.saturating_sub(margin * 2);
                    let chart_h = height.saturating_sub(margin * 3);
                    let bar_gap = 4u32;
                    let bar_w = if bar_count > 0 {
                        (chart_w.saturating_sub(bar_gap * bar_count as u32)) / bar_count as u32
                    } else {
                        0
                    };

                    // Color gradient: cyan -> magenta
                    let colors: Vec<u32> = (0..bar_count)
                        .map(|i| {
                            let t = i as f32 / (bar_count - 1).max(1) as f32;
                            let r = (0.0 + t * 230.0) as u32;
                            let g = (220.0 - t * 170.0) as u32;
                            let b = (220.0 + t * 35.0) as u32;
                            (0xFF << 24) | (r << 16) | (g << 8) | b
                        })
                        .collect();

                    for (i, &val) in fib_values.iter().enumerate() {
                        let bar_h =
                            ((val as f64 / max_fib) * chart_h as f64) as u32;
                        let x0 = margin + i as u32 * (bar_w + bar_gap);
                        let y0 = margin + chart_h - bar_h;

                        let color = colors[i];

                        for dy in 0..bar_h {
                            for dx in 0..bar_w {
                                let px = x0 + dx;
                                let py = y0 + dy;
                                if px < width && py < height {
                                    buffer[(py * width + px) as usize] = color;
                                }
                            }
                        }
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
