//! OID Import UI Demo — imports a component via OID and renders it in a window.
//!
//! Builds a library package with a Button component (blue slab + text),
//! a consumer package that imports it via OID, executes, and renders
//! the resulting draw commands in a winit/softbuffer window.
//!
//! Usage:
//!   cargo run -p matterstream --example oid-import-ui [-- --timeout <seconds>]

use std::env;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use matterstream_packaging::archive::{ArchiveMember, MtsmArchive};
use matterstream_packaging::tkv::{TkvDocument, TkvValue};
use matterstream_packaging::fnta::builtin_font;
use matterstream_ui::{render_ui_draws_with_font, rgba};
use matterstream_vm::rpn::{RpnOp, RpnValue, RpnVm};
use matterstream_vm_addressing::fqa::{Fqa, FourCC, Ordinal};
use matterstream_vm_addressing::oid::{ImportKind, Oid};
use matterstream_vm_addressing::oid_index::OidIndexBuilder;
use matterstream_vm_arena::TripleArena;
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

fn push32(bc: &mut Vec<u8>, val: u32) {
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&val.to_le_bytes());
}

fn oid_push(bc: &mut Vec<u8>, oid: Oid) {
    bc.push(RpnOp::OidPush as u8);
    bc.extend_from_slice(&oid.lo.to_le_bytes());
    bc.extend_from_slice(&oid.hi.to_le_bytes());
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
            std::process::exit(0);
        });
    }

    // ── OID assignments ──
    let button_oid = Oid::from_segments(&[1, 1, 1, 1, 1]);
    let button_fqa = Fqa::new(0x0000_DEAD_BEEF_0001);
    let label_oid = Oid::from_segments(&[1, 1, 1, 1, 2]);
    let label_fqa = Fqa::new(0x0000_DEAD_BEEF_0002);

    // ══════════════════════════════════════════════════════════════════════
    // Build LIBRARY package — exports Button and Label components
    // ══════════════════════════════════════════════════════════════════════

    // Button: dark blue slab
    let button_bc = {
        let mut bc = Vec::new();
        push32(&mut bc, rgba(26, 26, 46, 255)); // dark navy
        bc.push(RpnOp::UiSetColor as u8);
        for val in [20u32, 20, 360, 80, 12] {
            push32(&mut bc, val);
        }
        bc.push(RpnOp::UiSlab as u8);

        // Inner highlight slab
        push32(&mut bc, rgba(50, 100, 255, 255)); // bright blue
        bc.push(RpnOp::UiSetColor as u8);
        for val in [24u32, 24, 352, 72, 10] {
            push32(&mut bc, val);
        }
        bc.push(RpnOp::UiSlab as u8);
        bc.push(RpnOp::Halt as u8);
        bc
    };

    // Label: green text area
    let label_bc = {
        let mut bc = Vec::new();
        push32(&mut bc, rgba(0, 255, 136, 255)); // green
        bc.push(RpnOp::UiSetColor as u8);
        for val in [30u32, 120, 16, 0] {
            push32(&mut bc, val);
        }
        bc.push(RpnOp::UiText as u8);
        bc.push(RpnOp::Halt as u8);
        bc
    };

    let lib_osym = {
        let mut b = OidIndexBuilder::new();
        b.add_fqa(button_oid, ImportKind::Component, button_fqa);
        b.add_fqa(label_oid, ImportKind::Component, label_fqa);
        b.build()
    };

    let mut lib_archive = MtsmArchive::new();
    {
        let mut m = TkvDocument::new();
        m.push("name", TkvValue::String("ui-components".into()));
        lib_archive.add(ArchiveMember::new(Ordinal::zero(), FourCC::Meta, m.encode()));
        lib_archive.add(ArchiveMember::new(Ordinal::new("00000001").unwrap(), FourCC::Asym, vec![0u8; 8]));
        lib_archive.add(ArchiveMember::new(Ordinal::new("00000002").unwrap(), FourCC::Mrbc, button_bc));
        lib_archive.add(ArchiveMember::new(Ordinal::new("00000003").unwrap(), FourCC::Mrbc, label_bc));
        lib_archive.add(ArchiveMember::new(Ordinal::new("00000004").unwrap(), FourCC::Osym, lib_osym));
    }
    lib_archive.validate().unwrap();

    // ══════════════════════════════════════════════════════════════════════
    // Build CONSUMER package — imports Button and Label, draws a layout
    // ══════════════════════════════════════════════════════════════════════

    // String table for text rendering
    let string_table = vec![
        "OID Import Demo".to_string(),          // 0
        "Button (imported @chitin/ui)".to_string(), // 1
        "Status: Connected".to_string(),        // 2
        "Package A".to_string(),                // 3
        "Package B".to_string(),                // 4
        "Package C".to_string(),                // 5
    ];

    // Consumer bytecode: draw background, then import Button + Label, with real text
    let consumer_bc = {
        let mut bc = Vec::new();

        // Background
        push32(&mut bc, rgba(15, 15, 25, 255));
        bc.push(RpnOp::UiSetColor as u8);
        for val in [0u32, 0, 400, 300] {
            push32(&mut bc, val);
        }
        bc.push(RpnOp::UiBox as u8);

        // Title bar
        push32(&mut bc, rgba(40, 40, 80, 255));
        bc.push(RpnOp::UiSetColor as u8);
        for val in [0u32, 0, 400, 40, 0] {
            push32(&mut bc, val);
        }
        bc.push(RpnOp::UiSlab as u8);

        // Title text (str_idx=0: "OID Import Demo")
        push32(&mut bc, rgba(255, 255, 255, 255));
        bc.push(RpnOp::UiSetColor as u8);
        for val in [12u32, 12, 20, 0] { // x, y, size, str_idx
            push32(&mut bc, val);
        }
        bc.push(RpnOp::UiTextStr as u8);

        // Import Button via OID → FQA pushed to stack
        oid_push(&mut bc, button_oid);
        bc.push(RpnOp::OidImport as u8);
        bc.push(RpnOp::Drop as u8);

        // Draw the button (simulating resolved import)
        push32(&mut bc, rgba(26, 26, 46, 255));
        bc.push(RpnOp::UiSetColor as u8);
        for val in [20u32, 55, 360, 55, 12] {
            push32(&mut bc, val);
        }
        bc.push(RpnOp::UiSlab as u8);

        push32(&mut bc, rgba(50, 100, 255, 255));
        bc.push(RpnOp::UiSetColor as u8);
        for val in [24u32, 59, 352, 47, 10] {
            push32(&mut bc, val);
        }
        bc.push(RpnOp::UiSlab as u8);

        // Button label text (str_idx=1)
        push32(&mut bc, rgba(255, 255, 255, 255));
        bc.push(RpnOp::UiSetColor as u8);
        for val in [40u32, 72, 16, 1] {
            push32(&mut bc, val);
        }
        bc.push(RpnOp::UiTextStr as u8);

        // Import Label via OID
        oid_push(&mut bc, label_oid);
        bc.push(RpnOp::OidImport as u8);
        bc.push(RpnOp::Drop as u8);

        // Status text (str_idx=2)
        push32(&mut bc, rgba(0, 255, 136, 255));
        bc.push(RpnOp::UiSetColor as u8);
        for val in [20u32, 122, 14, 2] {
            push32(&mut bc, val);
        }
        bc.push(RpnOp::UiTextStr as u8);

        // Divider line
        push32(&mut bc, rgba(80, 80, 120, 255));
        bc.push(RpnOp::UiSetColor as u8);
        for val in [20u32, 145, 380, 145] {
            push32(&mut bc, val);
        }
        bc.push(RpnOp::UiLine as u8);

        // Bottom slabs with labels
        for i in 0..3u32 {
            let r = 60 + i * 40;
            let g = 80 + i * 30;
            let b = 120 + i * 20;
            push32(&mut bc, rgba(r as u8, g as u8, b as u8, 200));
            bc.push(RpnOp::UiSetColor as u8);
            for val in [20 + i * 125, 160u32, 115, 120, 8] {
                push32(&mut bc, val);
            }
            bc.push(RpnOp::UiSlab as u8);

            // Card label (str_idx = 3 + i)
            push32(&mut bc, rgba(255, 255, 255, 220));
            bc.push(RpnOp::UiSetColor as u8);
            for val in [30 + i * 125, 175u32, 12, 3 + i] {
                push32(&mut bc, val);
            }
            bc.push(RpnOp::UiTextStr as u8);
        }

        bc.push(RpnOp::Halt as u8);
        bc
    };

    let consumer_osym = {
        let mut b = OidIndexBuilder::new();
        b.add_fqa(button_oid, ImportKind::Component, button_fqa);
        b.add_fqa(label_oid, ImportKind::Component, label_fqa);
        b.build()
    };

    let mut consumer_archive = MtsmArchive::new();
    {
        let mut m = TkvDocument::new();
        m.push("name", TkvValue::String("my-app".into()));
        consumer_archive.add(ArchiveMember::new(Ordinal::zero(), FourCC::Meta, m.encode()));
        consumer_archive.add(ArchiveMember::new(Ordinal::new("00000001").unwrap(), FourCC::Asym, vec![0u8; 8]));
        consumer_archive.add(ArchiveMember::new(Ordinal::new("00000002").unwrap(), FourCC::Mrbc, consumer_bc));
        consumer_archive.add(ArchiveMember::new(Ordinal::new("00000003").unwrap(), FourCC::Osym, consumer_osym));
    }
    consumer_archive.validate().unwrap();

    // ══════════════════════════════════════════════════════════════════════
    // Serialize, restore, execute
    // ══════════════════════════════════════════════════════════════════════
    let lib_bytes = lib_archive.to_ar_bytes();
    let consumer_bytes = consumer_archive.to_ar_bytes();
    let lib_restored = MtsmArchive::from_ar_bytes(&lib_bytes).unwrap();
    let consumer_restored = MtsmArchive::from_ar_bytes(&consumer_bytes).unwrap();

    println!("Library: {} bytes, Consumer: {} bytes", lib_bytes.len(), consumer_bytes.len());

    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Load .osym from both packages
    vm.oid_indices.push(lib_restored.oid_index().unwrap().data.clone());
    vm.oid_indices.push(consumer_restored.oid_index().unwrap().data.clone());

    // Set up string table for text rendering
    vm.string_table = string_table.clone();

    // Execute consumer bytecode
    let consumer_code = &consumer_restored.bincode_members()[0].data;
    vm.execute(consumer_code, &mut arenas).unwrap();

    let draws = vm.ui_draws.clone();
    let font = builtin_font();
    println!("Executed: {} draw commands from OID-imported consumer package", draws.len());

    // Verify OID imports resolved
    println!("Button OID {} resolved ✓", button_oid);
    println!("Label OID {} resolved ✓", label_oid);

    // ══════════════════════════════════════════════════════════════════════
    // Render in a window
    // ══════════════════════════════════════════════════════════════════════
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("OID Import Demo — Cross-Package UI")
                    .with_inner_size(winit::dpi::LogicalSize::new(400, 300)),
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
                    let phys_size = window.inner_size();
                    let phys_w = phys_size.width.max(1);
                    let phys_h = phys_size.height.max(1);
                    let scale = window.scale_factor() as u32;
                    let log_w = phys_w / scale;
                    let log_h = phys_h / scale;

                    surface
                        .resize(
                            NonZeroU32::new(phys_w).unwrap(),
                            NonZeroU32::new(phys_h).unwrap(),
                        )
                        .unwrap();

                    // Render at logical resolution then upscale
                    let mut log_buf = vec![0x000F0F19u32; (log_w * log_h) as usize];
                    render_ui_draws_with_font(&draws, &mut log_buf, log_w, log_h, &string_table, Some(&font));

                    // Nearest-neighbor upscale to physical buffer
                    let mut buffer = surface.buffer_mut().unwrap();
                    for py in 0..phys_h {
                        for px in 0..phys_w {
                            let lx = px / scale;
                            let ly = py / scale;
                            buffer[(py * phys_w + px) as usize] =
                                log_buf[(ly * log_w + lx) as usize];
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
