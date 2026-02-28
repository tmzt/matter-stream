//! Full VM_SPEC v0.1.0 pipeline demonstration.
//!
//! Builds an AR archive, validates with SCL, loads into arena memory,
//! executes RPN bytecode, and renders the results in a window.
//!
//! Pipeline: Archive -> SCL -> Arena -> RPN -> Render
//!
//! Usage:
//!   cargo run --example vm-pipeline [-- --timeout <seconds>]

use std::env;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use matterstream::archive::{ArchiveMember, MtsmArchive};
use matterstream::arena::TripleArena;
use matterstream::aslr::{AslrToken, AsymTable};
use matterstream::fqa::{FourCC, Ordinal};
use matterstream::rpn::{RpnOp, RpnVm};
use matterstream::scl::{Scl, SclConfig, SclVerdict};
use matterstream::tkv::{TkvDocument, TkvValue};
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

/// Build RPN bytecode that sums integers 1..=n using arena memory.
/// Result (on stack): n*(n+1)/2
fn build_sum_bytecode(arenas: &mut TripleArena, n: u32) -> Vec<u8> {
    let ova_counter = arenas.alloc_nursery(4).unwrap();
    let ova_sum = arenas.alloc_nursery(4).unwrap();

    let mut bc = Vec::new();

    // counter = n
    bc.extend_from_slice(&encode_push64(n as u64));
    bc.extend_from_slice(&encode_push32(ova_counter.0));
    bc.push(RpnOp::Store as u8);

    // sum = 0
    bc.extend_from_slice(&encode_push64(0));
    bc.extend_from_slice(&encode_push32(ova_sum.0));
    bc.push(RpnOp::Store as u8);

    let loop_start = bc.len();

    // if counter > 0
    bc.extend_from_slice(&encode_push32(ova_counter.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push64(0));
    bc.push(RpnOp::CmpGt as u8);

    let jmpif_pos = bc.len();
    bc.extend_from_slice(&encode_jmpif(0)); // -> body

    let jmp_end_pos = bc.len();
    bc.extend_from_slice(&encode_jmp(0)); // -> end

    let body_start = bc.len();

    // sum += counter
    bc.extend_from_slice(&encode_push32(ova_sum.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push32(ova_counter.0));
    bc.push(RpnOp::Load as u8);
    bc.push(RpnOp::Add as u8);
    bc.extend_from_slice(&encode_push32(ova_sum.0));
    bc.push(RpnOp::Store as u8);

    // counter -= 1
    bc.extend_from_slice(&encode_push32(ova_counter.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push64(1));
    bc.push(RpnOp::Sub as u8);
    bc.extend_from_slice(&encode_push32(ova_counter.0));
    bc.push(RpnOp::Store as u8);

    bc.extend_from_slice(&encode_jmp(loop_start as u64));

    let loop_end = bc.len();

    // Push result
    bc.extend_from_slice(&encode_push32(ova_sum.0));
    bc.push(RpnOp::Load as u8);
    bc.push(RpnOp::Halt as u8);

    // Fix targets
    let body_bytes = (body_start as u64).to_le_bytes();
    bc[jmpif_pos + 1..jmpif_pos + 9].copy_from_slice(&body_bytes);
    let end_bytes = (loop_end as u64).to_le_bytes();
    bc[jmp_end_pos + 1..jmp_end_pos + 9].copy_from_slice(&end_bytes);

    bc
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
    // Step 1: Build an AR archive with .meta, .asym, and .mrbc members
    // ================================================================
    println!("=== Step 1: Building AR Archive ===");

    let mut archive = MtsmArchive::new();

    // Build TKV manifest (.meta)
    let mut manifest = TkvDocument::new();
    manifest.push("name", TkvValue::String("pipeline-demo".into()));
    manifest.push("version", TkvValue::Integer(1));
    manifest.push("description", TkvValue::String("VM pipeline demo".into()));
    let meta_bytes = manifest.encode();

    archive.add(ArchiveMember::new(
        Ordinal::zero(),
        FourCC::Meta,
        meta_bytes,
    ));

    // Build ASYM table
    let mut asym = AsymTable::new();
    asym.insert(AslrToken(0x1000), matterstream::Ova::new(matterstream::ArenaId::Nursery, 0, 0, 0));
    let asym_bytes = asym.to_bytes();

    archive.add(ArchiveMember::new(
        Ordinal::new("00000001").unwrap(),
        FourCC::Asym,
        asym_bytes,
    ));

    // Build RPN bytecode members (sum programs for n=1..=20)
    let mut arenas = TripleArena::new();
    let mut bytecodes: Vec<(u32, Vec<u8>)> = Vec::new();
    for n in 1..=20u32 {
        let bc = build_sum_bytecode(&mut arenas, n);
        bytecodes.push((n, bc.clone()));
        let ordinal = Ordinal::from_u64(n as u64 + 1);
        archive.add(ArchiveMember::new(ordinal, FourCC::Mrbc, bc));
    }

    // Validate and serialize
    archive.validate().expect("archive validation failed");
    let ar_bytes = archive.to_ar_bytes();
    println!(
        "  Archive: {} members, {} bytes serialized",
        archive.members.len(),
        ar_bytes.len()
    );

    // Roundtrip: parse back
    let parsed = MtsmArchive::from_ar_bytes(&ar_bytes).expect("archive parse failed");
    let parsed_manifest = parsed.manifest().expect("manifest parse failed");
    println!("  Manifest name: {:?}", parsed_manifest.entries[0].value);
    println!(
        "  Bincode members: {}",
        parsed.bincode_members().len()
    );

    // ================================================================
    // Step 2: SCL entropy validation
    // ================================================================
    println!("\n=== Step 2: SCL Entropy Validation ===");

    let scl = Scl::new(SclConfig::default());
    let mut accepted = 0;
    let mut rejected = 0;
    for member in &parsed.members {
        let verdict = scl.validate(&member.data);
        match verdict {
            SclVerdict::Accept => accepted += 1,
            _ => {
                println!(
                    "  REJECTED: {}.{} -- {}",
                    member.ordinal.as_str(),
                    member.fourcc.as_str(),
                    verdict
                );
                rejected += 1;
            }
        }
    }
    println!("  Accepted: {}, Rejected: {}", accepted, rejected);

    // ================================================================
    // Step 3: Execute RPN bytecode in fresh arenas
    // ================================================================
    println!("\n=== Step 3: RPN Execution (sum 1..=n) ===");

    let mut results: Vec<(u32, u64, u64)> = Vec::new(); // (n, result, gas)
    for (n, bc) in &bytecodes {
        let mut exec_arenas = TripleArena::new();
        let mut vm = RpnVm::new();
        let trace = vm.execute_metered(bc, &mut exec_arenas).unwrap();
        let result = vm.stack.last().and_then(|v| v.as_u64()).unwrap_or(0);
        let expected = (*n as u64) * (*n as u64 + 1) / 2;
        assert_eq!(result, expected, "sum(1..={}) should be {}", n, expected);
        results.push((*n, result, trace.gas_consumed));
    }

    for (n, result, gas) in &results {
        println!(
            "  sum(1..={:2}) = {:4}  [gas: {:5}]",
            n, result, gas
        );
    }

    // ================================================================
    // Step 4: Render results in window
    // ================================================================
    println!("\n=== Step 4: Rendering ===");

    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("VM Pipeline - sum(1..=n) + Gas Metering"),
            )
            .unwrap(),
    );

    let context = Context::new(window.clone()).unwrap();
    let mut surface = Surface::new(&context, window.clone()).unwrap();

    let max_result = results.iter().map(|r| r.1).max().unwrap_or(1) as f64;
    let max_gas = results.iter().map(|r| r.2).max().unwrap_or(1) as f64;

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
                    buffer.fill(0xFF0d1117); // dark background

                    let margin = 30u32;
                    let bar_count = results.len();
                    let half_h = height.saturating_sub(margin * 3) / 2;
                    let chart_w = width.saturating_sub(margin * 2);
                    let bar_gap = 3u32;
                    let bar_w = if bar_count > 0 {
                        chart_w.saturating_sub(bar_gap * bar_count as u32) / bar_count as u32
                    } else {
                        0
                    };

                    // Top half: sum results (green gradient)
                    for (i, &(_, result, _)) in results.iter().enumerate() {
                        let bar_h = ((result as f64 / max_result) * half_h as f64) as u32;
                        let x0 = margin + i as u32 * (bar_w + bar_gap);
                        let y0 = margin + half_h - bar_h;

                        let t = i as f32 / (bar_count - 1).max(1) as f32;
                        let r = (30.0 + t * 20.0) as u32;
                        let g = (180.0 + t * 75.0) as u32;
                        let b = (60.0 + t * 40.0) as u32;
                        let color = (0xFF << 24) | (r << 16) | (g << 8) | b;

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

                    // Divider line
                    let divider_y = margin + half_h + margin / 2;
                    if divider_y < height {
                        for x in margin..margin + chart_w {
                            if x < width {
                                buffer[(divider_y * width + x) as usize] = 0xFF404040;
                            }
                        }
                    }

                    // Bottom half: gas consumption (orange/red gradient)
                    let bottom_top = divider_y + margin / 2;
                    for (i, &(_, _, gas)) in results.iter().enumerate() {
                        let bar_h = ((gas as f64 / max_gas) * half_h as f64) as u32;
                        let x0 = margin + i as u32 * (bar_w + bar_gap);
                        let y0 = bottom_top + half_h - bar_h;

                        let t = i as f32 / (bar_count - 1).max(1) as f32;
                        let r = (200.0 + t * 55.0) as u32;
                        let g = (120.0 - t * 80.0) as u32;
                        let b = (30.0) as u32;
                        let color = (0xFF << 24) | (r.min(255) << 16) | (g << 8) | b;

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
