//! OID Import Demo — multi-package example with real OID-based imports.
//!
//! Demonstrates the full inter-package import pipeline:
//!   1. Build a "library" package with a Button component (draws a slab)
//!   2. Build a "consumer" package that imports the Button via OID
//!   3. Package both as MTSM archives with .osym symbol tables
//!   4. Serialize to AR bytes and parse back
//!   5. Load both .osym indices into the VM
//!   6. Execute the consumer's bytecode — OidCall resolves the Button's OID
//!   7. Verify the Button's FQA appears on the stack
//!   8. Security test: sandboxed OID cannot invoke native hooks

use matterstream_packaging::archive::{ArchiveMember, MtsmArchive};
use matterstream_packaging::tkv::{TkvDocument, TkvValue};
use matterstream_vm::rpn::{MtuiOp, NativeHookFn, RpnError, RpnOp, RpnValue, RpnVm, UserCallOp};
use matterstream_vm_addressing::fqa::{Fqa, FourCC, Ordinal};
use matterstream_vm_addressing::oid::{ImportKind, Oid};
use matterstream_vm_addressing::oid_index::OidIndexBuilder;
use matterstream_vm_arena::TripleArena;

/// Helper: encode a Push32 instruction.
fn push32(bc: &mut Vec<u8>, val: u32) {
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&val.to_le_bytes());
}

/// Helper: encode a Push128 instruction for an OID.
fn oid_push(bc: &mut Vec<u8>, oid: Oid) {
    bc.push(RpnOp::Push128 as u8);
    bc.extend_from_slice(&oid.lo.to_le_bytes());
    bc.extend_from_slice(&oid.hi.to_le_bytes());
}

fn main() {
    println!("=== OID Import Demo ===\n");

    // ── OID assignments ──
    // Library exports a Button component at 1.1.1.1.1
    let button_oid = Oid::from_segments(&[1, 1, 1, 1, 1]);
    let button_fqa = Fqa::new(0x0000_DEAD_BEEF_0001);

    // System native hook at 1.1.1.3.1
    let native_hook_oid = Oid::from_segments(&[1, 1, 1, 3, 1]);

    println!("OIDs:");
    println!("  Button component: {} -> FQA({:#018x})", button_oid, button_fqa.value());
    println!("  Native hook:      {} (system)", native_hook_oid);
    println!();

    // ══════════════════════════════════════════════════════════════════════
    // Step 1: Build the LIBRARY package
    // ══════════════════════════════════════════════════════════════════════
    let lib_bytecode = {
        let mut bc = Vec::new();
        let blue = matterstream_common::rgba(50, 100, 255, 255);
        push32(&mut bc, blue);
        bc.push(MtuiOp::SetColor.byte());
        for val in [0u32, 0, 200, 100, 12] {
            push32(&mut bc, val);
        }
        bc.push(MtuiOp::Slab.byte());
        bc.push(RpnOp::Halt as u8);
        bc
    };

    let lib_osym = {
        let mut builder = OidIndexBuilder::new();
        builder.add_fqa(button_oid, ImportKind::Component, button_fqa);
        builder.build()
    };

    let mut lib_archive = MtsmArchive::new();
    {
        let mut manifest = TkvDocument::new();
        manifest.push("name", TkvValue::String("ui-lib".into()));
        manifest.push("version", TkvValue::Integer(1));
        lib_archive.add(ArchiveMember::new(Ordinal::zero(), FourCC::Meta, manifest.encode()));
        lib_archive.add(ArchiveMember::new(Ordinal::new("00000001").unwrap(), FourCC::Asym, vec![0u8; 8]));
        lib_archive.add(ArchiveMember::new(Ordinal::new("00000002").unwrap(), FourCC::Mrbc, lib_bytecode));
        lib_archive.add(ArchiveMember::new(Ordinal::new("00000003").unwrap(), FourCC::Osym, lib_osym));
    }
    lib_archive.validate().expect("Library archive should be valid");
    println!("1. Built library package: {} members", lib_archive.members.len());

    // ══════════════════════════════════════════════════════════════════════
    // Step 2: Build the CONSUMER package
    // ══════════════════════════════════════════════════════════════════════
    let consumer_bytecode = {
        let mut bc = Vec::new();
        // Import the Button component via OID → pushes FQA onto stack
        oid_push(&mut bc, button_oid);
        bc.push(RpnOp::UserCall as u8);
        bc.extend_from_slice(&(UserCallOp::OidCall as u64).to_le_bytes());
        bc.extend_from_slice(&0u64.to_le_bytes());
        bc.push(RpnOp::Halt as u8);
        bc
    };

    let consumer_osym = {
        let mut builder = OidIndexBuilder::new();
        // Consumer declares it imports button_oid (resolves to button_fqa)
        builder.add_fqa(button_oid, ImportKind::Component, button_fqa);
        // System native hook
        builder.add_native_hook(native_hook_oid, 0);
        builder.build()
    };

    let mut consumer_archive = MtsmArchive::new();
    {
        let mut manifest = TkvDocument::new();
        manifest.push("name", TkvValue::String("my-app".into()));
        manifest.push("version", TkvValue::Integer(1));
        consumer_archive.add(ArchiveMember::new(Ordinal::zero(), FourCC::Meta, manifest.encode()));
        consumer_archive.add(ArchiveMember::new(Ordinal::new("00000001").unwrap(), FourCC::Asym, vec![0u8; 8]));
        consumer_archive.add(ArchiveMember::new(Ordinal::new("00000002").unwrap(), FourCC::Mrbc, consumer_bytecode));
        consumer_archive.add(ArchiveMember::new(Ordinal::new("00000003").unwrap(), FourCC::Osym, consumer_osym));
    }
    consumer_archive.validate().expect("Consumer archive should be valid");
    println!("2. Built consumer package: {} members", consumer_archive.members.len());

    // ══════════════════════════════════════════════════════════════════════
    // Step 3: Serialize both archives to AR bytes and parse back
    // ══════════════════════════════════════════════════════════════════════
    let lib_bytes = lib_archive.to_ar_bytes();
    let consumer_bytes = consumer_archive.to_ar_bytes();
    println!("3. Serialized: library={} bytes, consumer={} bytes", lib_bytes.len(), consumer_bytes.len());

    let lib_restored = MtsmArchive::from_ar_bytes(&lib_bytes).expect("Should parse library");
    let consumer_restored = MtsmArchive::from_ar_bytes(&consumer_bytes).expect("Should parse consumer");
    lib_restored.validate().expect("Library should validate");
    consumer_restored.validate().expect("Consumer should validate");
    println!("   Both archives restored and validated");

    // ══════════════════════════════════════════════════════════════════════
    // Step 4: Load .osym indices and verify OID lookups
    // ══════════════════════════════════════════════════════════════════════
    let lib_osym_data = lib_restored.oid_index().expect("Library should have .osym").data.clone();
    let consumer_osym_data = consumer_restored.oid_index().expect("Consumer should have .osym").data.clone();

    let lib_idx = lib_restored.oid_index_parsed().unwrap().unwrap();
    let consumer_idx = consumer_restored.oid_index_parsed().unwrap().unwrap();
    println!("4. Loaded OID indices: library={} entries, consumer={} entries", lib_idx.len(), consumer_idx.len());

    // Verify we can look up the Button
    let button_entry = lib_idx.lookup(button_oid).expect("Button should be in library index");
    assert_eq!(button_entry.fqa().value(), button_fqa.value());
    println!("   Button OID {} → FQA({:#018x}) ✓", button_oid, button_entry.fqa().value());

    // ══════════════════════════════════════════════════════════════════════
    // Step 5: Execute the consumer's bytecode with OID import
    // ══════════════════════════════════════════════════════════════════════
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Load both .osym indices into the VM
    vm.oid_indices.push(lib_osym_data);
    vm.oid_indices.push(consumer_osym_data);

    // Register a native hook for the system hook
    fn system_hook(vm: &mut RpnVm, _arenas: &mut TripleArena) -> Result<(), RpnError> {
        vm.push_value(RpnValue::U64(0xCAFE))?;
        Ok(())
    }
    vm.native_hooks.push(system_hook as NativeHookFn);

    // Execute consumer bytecode
    let consumer_mrbc = consumer_restored.bincode_members();
    let consumer_code = &consumer_mrbc[0].data;
    vm.execute(consumer_code, &mut arenas).expect("Consumer should execute");

    // The OidCall for the Button (non-native) pushes the FQA
    assert_eq!(vm.stack.len(), 1);
    match &vm.stack[0] {
        RpnValue::Fqa(fqa) => {
            assert_eq!(fqa.value(), button_fqa.value());
            println!("5. Consumer executed: OidCall resolved Button → FQA({:#018x}) ✓", fqa.value());
        }
        other => panic!("Expected Fqa on stack, got {:?}", other),
    }

    // ══════════════════════════════════════════════════════════════════════
    // Step 6: Test native hook dispatch (system OID)
    // ══════════════════════════════════════════════════════════════════════
    let mut hook_bc = Vec::new();
    oid_push(&mut hook_bc, native_hook_oid);
    hook_bc.push(RpnOp::UserCall as u8);
    hook_bc.extend_from_slice(&(UserCallOp::OidCall as u64).to_le_bytes());
    hook_bc.extend_from_slice(&0u64.to_le_bytes());
    hook_bc.push(RpnOp::Halt as u8);

    vm.execute(&hook_bc, &mut arenas).expect("System hook should execute");
    match &vm.stack[0] {
        RpnValue::U64(val) => {
            assert_eq!(*val, 0xCAFE);
            println!("6. Native hook dispatch: system OID → VM-escape → pushed 0x{:X} ✓", val);
        }
        other => panic!("Expected U64 from hook, got {:?}", other),
    }

    // ══════════════════════════════════════════════════════════════════════
    // Step 7: Security test — sandboxed OID cannot invoke native hooks
    // ══════════════════════════════════════════════════════════════════════
    // Register a native hook under a sandboxed OID (public CHT)
    let sandboxed_hook_oid = Oid::from_segments(&[1, 1, 1, 1, 3]);
    let mut bad_osym_builder = OidIndexBuilder::new();
    bad_osym_builder.add_native_hook(sandboxed_hook_oid, 0);
    vm.oid_indices.push(bad_osym_builder.build());

    let mut bad_bc = Vec::new();
    oid_push(&mut bad_bc, sandboxed_hook_oid);
    bad_bc.push(RpnOp::UserCall as u8);
    bad_bc.extend_from_slice(&(UserCallOp::OidCall as u64).to_le_bytes());
    bad_bc.extend_from_slice(&0u64.to_le_bytes());
    bad_bc.push(RpnOp::Halt as u8);

    let err = vm.execute(&bad_bc, &mut arenas).unwrap_err();
    match err {
        RpnError::OidSecurityViolation { .. } => {
            println!("7. Security enforcement: sandboxed OID correctly rejected for VM-escape ✓");
        }
        other => panic!("Expected OidSecurityViolation, got {:?}", other),
    }

    println!("\n=== Demo complete: library packaged → consumer imports via OID → resolved → security enforced ===");
}
