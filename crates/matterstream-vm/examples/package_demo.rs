//! Packaging demo — assemble a small UI program, package it as a TKV + archive
//! member, then load and execute from the archive.
//!
//! Demonstrates the full pipeline:
//!   1. Build bytecode with the assembler
//!   2. Wrap in an MTSM archive (manifest + asym + .mrbc)
//!   3. Serialize to AR bytes and parse back
//!   4. Extract bytecode from the archive
//!   5. Execute on the RPN VM and verify draw output

use matterstream_packaging::archive::{ArchiveMember, MtsmArchive};
use matterstream_packaging::tkv::{TkvDocument, TkvValue};
use matterstream_vm::rpn::{RpnOp, RpnVm};
use matterstream_vm::ui_vm::{rgba, UiDrawCmd};
use matterstream_vm_addressing::fqa::{FourCC, Ordinal};
use matterstream_vm_arena::TripleArena;

fn main() {
    println!("=== MTSM Packaging Demo ===\n");

    // ── Step 1: Build bytecode ──
    // A small UI program that draws a blue rounded rectangle:
    //   Push32(color); UiSetColor; Push32(0); Push32(0); Push32(200); Push32(100); Push32(12); UiSlab; Halt
    let mut bytecode = Vec::new();

    // Set color to blue
    let blue = rgba(50, 100, 255, 255);
    bytecode.push(RpnOp::Push32 as u8);
    bytecode.extend_from_slice(&blue.to_le_bytes());
    bytecode.push(RpnOp::UiSetColor as u8);

    // Draw a slab at (0, 0) size 200×100 radius 12
    for val in [0u32, 0, 200, 100, 12] {
        bytecode.push(RpnOp::Push32 as u8);
        bytecode.extend_from_slice(&val.to_le_bytes());
    }
    bytecode.push(RpnOp::UiSlab as u8);

    // Halt
    bytecode.push(RpnOp::Halt as u8);

    println!("1. Assembled {} bytes of bytecode", bytecode.len());

    // ── Step 2: Build the MTSM archive ──
    let mut archive = MtsmArchive::new();

    // Manifest (TKV metadata)
    let mut manifest = TkvDocument::new();
    manifest.push("name", TkvValue::String("demo-ui-card".into()));
    manifest.push("version", TkvValue::Integer(1));
    manifest.push("description", TkvValue::String("A blue rounded rectangle".into()));
    archive.add(ArchiveMember::new(
        Ordinal::zero(),
        FourCC::Meta,
        manifest.encode(),
    ));

    // Asymmetric table (minimal)
    archive.add(ArchiveMember::new(
        Ordinal::new("00000001").unwrap(),
        FourCC::Asym,
        vec![0u8; 8],
    ));

    // Bytecode member
    archive.add(ArchiveMember::new(
        Ordinal::new("00000002").unwrap(),
        FourCC::Mrbc,
        bytecode.clone(),
    ));

    // Validate the archive structure
    archive.validate().expect("Archive should be valid");
    println!("2. Built MTSM archive with {} members", archive.members.len());

    // ── Step 3: Serialize to AR bytes and parse back ──
    let ar_bytes = archive.to_ar_bytes();
    println!("3. Serialized to {} bytes of AR format", ar_bytes.len());

    let restored = MtsmArchive::from_ar_bytes(&ar_bytes).expect("Should parse AR bytes");
    restored.validate().expect("Restored archive should be valid");
    println!("   Restored archive: {} members", restored.members.len());

    // ── Step 4: Extract bytecode from the archive ──
    let manifest = restored.manifest().expect("Should have manifest");
    let name = manifest
        .entries
        .iter()
        .find(|e| e.key == "name")
        .map(|e| match &e.value {
            TkvValue::String(s) => s.as_str(),
            _ => "unknown",
        })
        .unwrap_or("unknown");
    println!("4. Package name: {}", name);

    let mrbc_members = restored.bincode_members();
    assert_eq!(mrbc_members.len(), 1, "Should have exactly 1 .mrbc member");
    let loaded_bytecode = &mrbc_members[0].data;
    assert_eq!(loaded_bytecode, &bytecode, "Bytecode should match");
    println!("   Extracted {} bytes of bytecode", loaded_bytecode.len());

    // ── Step 5: Execute on the RPN VM ──
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let result = vm.execute(loaded_bytecode, &mut arenas);
    assert!(result.is_ok(), "VM execution should succeed");

    let draws = &vm.ui_draws;
    assert!(!draws.is_empty(), "Should have draw commands");
    println!("5. VM produced {} draw command(s)", draws.len());

    // Verify the slab draw command
    let draw = &draws[0];
    match draw {
        UiDrawCmd::Slab { x, y, w, h, radius, color } => {
            println!(
                "   Draw: Slab at ({},{}) size {}x{} radius {} color=0x{:08X}",
                x, y, w, h, radius, color
            );
            assert_eq!(*x, 0);
            assert_eq!(*y, 0);
            assert_eq!(*w, 200);
            assert_eq!(*h, 100);
            assert_eq!(*radius, 12);
            assert_eq!(*color, blue);
        }
        other => panic!("Expected Slab draw command, got {:?}", other),
    }

    println!("\n=== Demo complete: bytecode assembled → packaged → loaded → executed ===");
}
