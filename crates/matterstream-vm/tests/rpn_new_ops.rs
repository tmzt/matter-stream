//! Tests for new opcodes: bitwise, bank access, comparisons, event, rand.

use matterstream_vm::event::{VmEvent, VmEventType};
use matterstream_vm::rpn::{
    RpnError, RpnOp, RpnVm, BANK_INT, BANK_ZERO_PAGE,
};
use matterstream_vm_arena::TripleArena;

fn encode(instructions: &[(RpnOp, Option<&[u8]>)]) -> Vec<u8> {
    RpnVm::encode(instructions)
}

fn run(bytecode: &[u8]) -> RpnVm {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(bytecode, &mut arenas).unwrap();
    vm
}

// ── Bitwise operations ──

#[test]
fn test_and() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&0xFF00u64.to_le_bytes())),
        (RpnOp::Push64, Some(&0x0FF0u64.to_le_bytes())),
        (RpnOp::And, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(0x0F00));
}

#[test]
fn test_or() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&0xFF00u64.to_le_bytes())),
        (RpnOp::Push64, Some(&0x00FFu64.to_le_bytes())),
        (RpnOp::Or, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(0xFFFF));
}

#[test]
fn test_xor() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&0xFFFFu64.to_le_bytes())),
        (RpnOp::Push64, Some(&0xFF00u64.to_le_bytes())),
        (RpnOp::Xor, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(0x00FF));
}

#[test]
fn test_shl() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&1u64.to_le_bytes())),
        (RpnOp::Push64, Some(&8u64.to_le_bytes())),
        (RpnOp::Shl, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(256));
}

#[test]
fn test_shr() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&256u64.to_le_bytes())),
        (RpnOp::Push64, Some(&4u64.to_le_bytes())),
        (RpnOp::Shr, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(16));
}

#[test]
fn test_not_zero() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&0u64.to_le_bytes())),
        (RpnOp::Not, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(1));
}

#[test]
fn test_not_nonzero() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&42u64.to_le_bytes())),
        (RpnOp::Not, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(0));
}

// ── Extended comparisons ──

#[test]
fn test_cmp_ge() {
    // 10 >= 10 → 1
    let bc = encode(&[
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::CmpGe, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(1));

    // 9 >= 10 → 0
    let bc2 = encode(&[
        (RpnOp::Push64, Some(&9u64.to_le_bytes())),
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::CmpGe, None),
    ]);
    let vm2 = run(&bc2);
    assert_eq!(vm2.stack[0].as_u64(), Some(0));
}

#[test]
fn test_cmp_le() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::CmpLe, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(1));
}

#[test]
fn test_cmp_ne() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::Push64, Some(&20u64.to_le_bytes())),
        (RpnOp::CmpNe, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(1));

    let bc2 = encode(&[
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::CmpNe, None),
    ]);
    let vm2 = run(&bc2);
    assert_eq!(vm2.stack[0].as_u64(), Some(0));
}

// ── Bank access ──

#[test]
fn test_load_store_int_bank() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Store 42 to int bank slot 3: push value, push bank, push slot, StoreBank
    let bc = encode(&[
        (RpnOp::Push64, Some(&42u64.to_le_bytes())),
        (RpnOp::Push32, Some(&BANK_INT.to_le_bytes())),
        (RpnOp::Push32, Some(&3u32.to_le_bytes())),
        (RpnOp::StoreBank, None),
        // Load back
        (RpnOp::Push32, Some(&BANK_INT.to_le_bytes())),
        (RpnOp::Push32, Some(&3u32.to_le_bytes())),
        (RpnOp::LoadBank, None),
    ]);
    vm.execute(&bc, &mut arenas).unwrap();

    assert_eq!(vm.int_bank[3], 42);
    assert_eq!(vm.stack[0].as_u64(), Some(42));
}

#[test]
fn test_load_store_zero_page() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Store 255 to ZP slot 10
    let bc = encode(&[
        (RpnOp::Push64, Some(&255u64.to_le_bytes())),
        (RpnOp::Push32, Some(&BANK_ZERO_PAGE.to_le_bytes())),
        (RpnOp::Push32, Some(&10u32.to_le_bytes())),
        (RpnOp::StoreBank, None),
        // Load back
        (RpnOp::Push32, Some(&BANK_ZERO_PAGE.to_le_bytes())),
        (RpnOp::Push32, Some(&10u32.to_le_bytes())),
        (RpnOp::LoadBank, None),
    ]);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.zero_page[10], 255);
    assert_eq!(vm.stack[0].as_u64(), Some(255));
}

#[test]
fn test_invalid_bank_id() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let bc = encode(&[
        (RpnOp::Push32, Some(&99u32.to_le_bytes())), // invalid bank
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::LoadBank, None),
    ]);
    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::InvalidBankId(99))));
}

#[test]
fn test_invalid_bank_slot() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let bc = encode(&[
        (RpnOp::Push32, Some(&BANK_INT.to_le_bytes())),
        (RpnOp::Push32, Some(&999u32.to_le_bytes())), // out of bounds
        (RpnOp::LoadBank, None),
    ]);
    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(
        result,
        Err(RpnError::InvalidBankSlot { bank: 1, slot: 999 })
    ));
}

// ── Event opcodes ──

#[test]
fn test_ev_poll_empty() {
    let bc = encode(&[(RpnOp::EvPoll, None)]);
    let vm = run(&bc);
    // Should push (type=0, data=0) — type on top
    assert_eq!(vm.stack.len(), 2);
    assert_eq!(vm.stack[1].as_u32(), Some(VmEventType::None as u32));
    assert_eq!(vm.stack[0].as_u64(), Some(0));
}

#[test]
fn test_ev_poll_with_event() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.event_queue.push_back(VmEvent::key_down(32)); // space

    let bc = encode(&[(RpnOp::EvPoll, None)]);
    vm.execute(&bc, &mut arenas).unwrap();

    assert_eq!(vm.stack.len(), 2);
    assert_eq!(vm.stack[1].as_u32(), Some(VmEventType::KeyDown as u32));
    assert_eq!(vm.stack[0].as_u64(), Some(32)); // space key code
}

#[test]
fn test_ev_has_event() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // No events → 0
    let bc = encode(&[(RpnOp::EvHasEvent, None)]);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64(), Some(0));

    // Add event → 1
    vm.event_queue.push_back(VmEvent::tick(16));
    let bc2 = encode(&[(RpnOp::EvHasEvent, None)]);
    vm.execute(&bc2, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64(), Some(1));
}

#[test]
fn test_frame_count() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.frame_count = 123;

    let bc = encode(&[(RpnOp::FrameCount, None)]);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64(), Some(123));
}

#[test]
fn test_rand() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let bc = encode(&[
        (RpnOp::Push32, Some(&100u32.to_le_bytes())),
        (RpnOp::Rand, None),
    ]);
    vm.execute(&bc, &mut arenas).unwrap();

    let val = vm.stack[0].as_u32().unwrap();
    assert!(val < 100);
}

#[test]
fn test_rand_deterministic() {
    // Two VMs with same seed should produce same result
    let bc = encode(&[
        (RpnOp::Push32, Some(&1000u32.to_le_bytes())),
        (RpnOp::Rand, None),
    ]);

    let vm1 = run(&bc);
    let vm2 = run(&bc);
    assert_eq!(vm1.stack[0].as_u32(), vm2.stack[0].as_u32());
}
