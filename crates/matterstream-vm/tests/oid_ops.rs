//! Tests for OID import opcodes (Push128, UserCall+OidImport, UserCall+OidCall).

use matterstream_vm::rpn::{NativeHookFn, RpnError, RpnOp, RpnValue, RpnVm, UserCallOp, VmHandleNative};
use matterstream_vm_addressing::fqa::Fqa;
use matterstream_vm_addressing::oid::{ImportKind, Oid};
use matterstream_vm_addressing::oid_index::OidIndexBuilder;
use matterstream_vm_arena::TripleArena;

/// Build a .osym index with some test entries.
fn make_test_osym() -> Vec<u8> {
    let mut builder = OidIndexBuilder::new();
    builder.add_fqa(
        Oid::from_segments(&[1, 1, 1, 1, 1]),
        ImportKind::Component,
        Fqa::new(0xAAAA),
    );
    builder.add_fqa(
        Oid::from_segments(&[1, 1, 1, 1, 2]),
        ImportKind::Hook,
        Fqa::new(0xBBBB),
    );
    builder.add_native_hook(Oid::from_segments(&[1, 1, 1, 3, 1]), 0);
    builder.build()
}

/// Encode a Push128 instruction for the given OID.
fn encode_oid_push(oid: Oid) -> Vec<u8> {
    let mut bc = vec![RpnOp::Push128 as u8];
    bc.extend_from_slice(&oid.lo.to_le_bytes());
    bc.extend_from_slice(&oid.hi.to_le_bytes());
    bc
}

#[test]
fn oid_push_puts_u128_on_stack() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let oid = Oid::from_segments(&[1, 1, 1, 1, 1]);
    let mut bc = encode_oid_push(oid);
    bc.push(RpnOp::Halt as u8);

    vm.execute(&bc, &mut arenas).unwrap();

    assert_eq!(vm.stack.len(), 1);
    match &vm.stack[0] {
        RpnValue::Fqa(fqa) => {
            let v = fqa.value();
            assert_eq!(Oid::new((v >> 64) as u64, v as u64), oid);
        }
        other => panic!("expected Fqa, got {:?}", other),
    }
}

#[test]
fn oid_import_resolves_to_fqa() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.oid_indices.push(make_test_osym());

    let oid = Oid::from_segments(&[1, 1, 1, 1, 1]);
    let mut bc = encode_oid_push(oid);
    bc.push(RpnOp::UserCall as u8);
    bc.extend_from_slice(&(UserCallOp::OidImport as u64).to_le_bytes());
    bc.extend_from_slice(&0u64.to_le_bytes());
    bc.push(RpnOp::Halt as u8);

    vm.execute(&bc, &mut arenas).unwrap();

    assert_eq!(vm.stack.len(), 1);
    match &vm.stack[0] {
        RpnValue::Fqa(fqa) => assert_eq!(fqa.value(), 0xAAAA),
        other => panic!("expected Fqa, got {:?}", other),
    }
}

#[test]
fn oid_import_not_found_errors() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.oid_indices.push(make_test_osym());

    let oid = Oid::from_segments(&[3, 3, 3]); // not in index
    let mut bc = encode_oid_push(oid);
    bc.push(RpnOp::UserCall as u8);
    bc.extend_from_slice(&(UserCallOp::OidImport as u64).to_le_bytes());
    bc.extend_from_slice(&0u64.to_le_bytes());
    bc.push(RpnOp::Halt as u8);

    let err = vm.execute(&bc, &mut arenas).unwrap_err();
    assert!(matches!(err, RpnError::OidNotFound { .. }));
}

#[test]
fn oid_call_native_hook_dispatches() {
    static HOOK_CALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    fn test_hook(_vm: &mut VmHandleNative, _arenas: &mut TripleArena) -> Result<(), RpnError> {
        HOOK_CALLED.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.oid_indices.push(make_test_osym());
    vm.native_hooks.push(test_hook as NativeHookFn);

    // OID 1.1.1.3.1 is a NativeHook with dispatch_id=0, under system subtree
    let oid = Oid::from_segments(&[1, 1, 1, 3, 1]);
    let mut bc = encode_oid_push(oid);
    bc.push(RpnOp::UserCall as u8);
    bc.extend_from_slice(&(UserCallOp::OidCall as u64).to_le_bytes());
    bc.extend_from_slice(&0u64.to_le_bytes());
    bc.push(RpnOp::Halt as u8);

    HOOK_CALLED.store(false, std::sync::atomic::Ordering::SeqCst);
    vm.execute(&bc, &mut arenas).unwrap();
    assert!(HOOK_CALLED.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn oid_call_sandboxed_rejects_native_hook() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Register a native hook under the public (sandboxed) subtree
    let mut builder = OidIndexBuilder::new();
    builder.add_native_hook(Oid::from_segments(&[1, 1, 1, 1, 1]), 0);
    vm.oid_indices.push(builder.build());

    fn dummy(_vm: &mut VmHandleNative, _arenas: &mut TripleArena) -> Result<(), RpnError> {
        Ok(())
    }
    vm.native_hooks.push(dummy as NativeHookFn);

    // OID 1.1.1.1.1 is under public CHT — sandboxed
    let oid = Oid::from_segments(&[1, 1, 1, 1, 1]);
    let mut bc = encode_oid_push(oid);
    bc.push(RpnOp::UserCall as u8);
    bc.extend_from_slice(&(UserCallOp::OidCall as u64).to_le_bytes());
    bc.extend_from_slice(&0u64.to_le_bytes());
    bc.push(RpnOp::Halt as u8);

    let err = vm.execute(&bc, &mut arenas).unwrap_err();
    assert!(matches!(err, RpnError::OidSecurityViolation { .. }));
}

#[test]
fn component_ops_oid_import_pushes_fqa() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.oid_indices.push(make_test_osym());

    // OID 1.1.1.1.2 is a Hook with FQA=0xBBBB
    let oid = Oid::from_segments(&[1, 1, 1, 1, 2]);
    let mut bc = encode_oid_push(oid);
    bc.push(RpnOp::UserCall as u8);
    bc.extend_from_slice(&(UserCallOp::ComponentOps as u64).to_le_bytes());
    bc.extend_from_slice(&0x00u64.to_le_bytes()); // sub-op 0 = OID_IMPORT
    bc.push(RpnOp::Halt as u8);

    vm.execute(&bc, &mut arenas).unwrap();

    assert_eq!(vm.stack.len(), 1);
    match &vm.stack[0] {
        RpnValue::Fqa(fqa) => assert_eq!(fqa.value(), 0xBBBB),
        other => panic!("expected Fqa, got {:?}", other),
    }
}

#[test]
fn oid_import_searches_multiple_indices() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // First index has 1.1.1.1.1
    let mut b1 = OidIndexBuilder::new();
    b1.add_fqa(Oid::from_segments(&[1, 1, 1, 1, 1]), ImportKind::Component, Fqa::new(0x1111));
    vm.oid_indices.push(b1.build());

    // Second index has 1.1.2.1
    let mut b2 = OidIndexBuilder::new();
    b2.add_fqa(Oid::from_segments(&[1, 1, 2, 1]), ImportKind::Symbol, Fqa::new(0x2222));
    vm.oid_indices.push(b2.build());

    // Resolve from first index
    let oid1 = Oid::from_segments(&[1, 1, 1, 1, 1]);
    let mut bc = encode_oid_push(oid1);
    bc.push(RpnOp::UserCall as u8);
    bc.extend_from_slice(&(UserCallOp::OidImport as u64).to_le_bytes());
    bc.extend_from_slice(&0u64.to_le_bytes());

    // Resolve from second index
    let oid2 = Oid::from_segments(&[1, 1, 2, 1]);
    bc.extend_from_slice(&encode_oid_push(oid2));
    bc.push(RpnOp::UserCall as u8);
    bc.extend_from_slice(&(UserCallOp::OidImport as u64).to_le_bytes());
    bc.extend_from_slice(&0u64.to_le_bytes());

    bc.push(RpnOp::Halt as u8);

    vm.execute(&bc, &mut arenas).unwrap();

    assert_eq!(vm.stack.len(), 2);
    match &vm.stack[0] {
        RpnValue::Fqa(fqa) => assert_eq!(fqa.value(), 0x1111),
        other => panic!("expected Fqa, got {:?}", other),
    }
    match &vm.stack[1] {
        RpnValue::Fqa(fqa) => assert_eq!(fqa.value(), 0x2222),
        other => panic!("expected Fqa, got {:?}", other),
    }
}

#[test]
fn oid_call_invalid_native_hook_errors() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Register native hook with dispatch_id=5 but don't add any hooks to the table
    let mut builder = OidIndexBuilder::new();
    builder.add_native_hook(Oid::from_segments(&[1, 1, 1, 3, 1]), 5);
    vm.oid_indices.push(builder.build());

    let oid = Oid::from_segments(&[1, 1, 1, 3, 1]);
    let mut bc = encode_oid_push(oid);
    bc.push(RpnOp::UserCall as u8);
    bc.extend_from_slice(&(UserCallOp::OidCall as u64).to_le_bytes());
    bc.extend_from_slice(&0u64.to_le_bytes());
    bc.push(RpnOp::Halt as u8);

    let err = vm.execute(&bc, &mut arenas).unwrap_err();
    assert!(matches!(err, RpnError::OidInvalidNativeHook(5)));
}
