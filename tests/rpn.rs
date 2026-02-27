//! Tests for RPN stack language VM.

use matterstream::arena::TripleArena;
use matterstream::ova::ArenaId;
use matterstream::rpn::{RpnError, RpnOp, RpnVm};

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

#[test]
fn push_pop_roundtrip() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push32(42);
    bc.push(RpnOp::Drop as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert!(vm.stack.is_empty());
}

#[test]
fn arithmetic_add() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Push 3, Push 4, Add -> should give 7
    let mut bc = encode_push64(3);
    bc.extend_from_slice(&encode_push64(4));
    bc.push(RpnOp::Add as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 1);
    let result = vm.stack[0].as_u64().unwrap();
    assert_eq!(result, 7);
}

#[test]
fn arithmetic_sub() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(10);
    bc.extend_from_slice(&encode_push64(3));
    bc.push(RpnOp::Sub as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 7);
}

#[test]
fn arithmetic_mul() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(6);
    bc.extend_from_slice(&encode_push64(7));
    bc.push(RpnOp::Mul as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 42);
}

#[test]
fn arithmetic_div() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(20);
    bc.extend_from_slice(&encode_push64(4));
    bc.push(RpnOp::Div as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 5);
}

#[test]
fn division_by_zero_error() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(10);
    bc.extend_from_slice(&encode_push64(0));
    bc.push(RpnOp::Div as u8);

    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::DivisionByZero)));
}

#[test]
fn stack_underflow_error() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let bc = vec![RpnOp::Add as u8]; // No values on stack
    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::StackUnderflow)));
}

#[test]
fn stack_overflow_error() {
    let mut vm = RpnVm::new();
    vm.max_stack_depth = 2;
    let mut arenas = TripleArena::new();

    let mut bc = encode_push32(1);
    bc.extend_from_slice(&encode_push32(2));
    bc.extend_from_slice(&encode_push32(3)); // should overflow

    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::StackOverflow)));
}

#[test]
fn dup_and_swap() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Push 5, Dup -> [5, 5], Push 10, Swap -> [5, 10, 5]
    let mut bc = encode_push64(5);
    bc.push(RpnOp::Dup as u8);
    bc.extend_from_slice(&encode_push64(10));
    bc.push(RpnOp::Swap as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 3);
    assert_eq!(vm.stack[0].as_u64().unwrap(), 5);
    assert_eq!(vm.stack[1].as_u64().unwrap(), 10); // swapped
    assert_eq!(vm.stack[2].as_u64().unwrap(), 5);  // swapped
}

#[test]
fn sync_swaps_arenas() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    assert_eq!(arenas.active_arena(), ArenaId::DynamicA);

    let bc = vec![RpnOp::Sync as u8];
    vm.execute(&bc, &mut arenas).unwrap();

    assert!(vm.synced);
    assert_eq!(arenas.active_arena(), ArenaId::DynamicB);
}

#[test]
fn load_store_via_ova() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Allocate an object in nursery
    let ova = arenas.alloc_nursery(16).unwrap();

    // Store 0x42 at OVA
    let mut bc = encode_push64(0x42);
    let mut ova_push = vec![RpnOp::Push32 as u8];
    ova_push.extend_from_slice(&ova.0.to_le_bytes());
    bc.extend_from_slice(&ova_push);
    bc.push(RpnOp::Store as u8);

    // Load back from OVA
    bc.extend_from_slice(&ova_push);
    bc.push(RpnOp::Load as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u32().unwrap(), 0x42);
}

#[test]
fn map_new_set_get() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // MapNew, Push key 1, Push value 99, MapSet, Push key 1, MapGet
    let mut bc = vec![RpnOp::MapNew as u8];
    bc.extend_from_slice(&encode_push64(1)); // key
    bc.extend_from_slice(&encode_push64(99)); // value
    bc.push(RpnOp::MapSet as u8);
    bc.extend_from_slice(&encode_push64(1)); // key
    bc.push(RpnOp::MapGet as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u64().unwrap(), 99);
}

#[test]
fn nop_is_no_op() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let bc = vec![RpnOp::Nop as u8, RpnOp::Nop as u8, RpnOp::Nop as u8];
    vm.execute(&bc, &mut arenas).unwrap();
    assert!(vm.stack.is_empty());
}

#[test]
fn invalid_opcode_rejected() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let bc = vec![0xFF]; // invalid opcode
    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::InvalidOpcode(0xFF))));
}

#[test]
fn encode_decode_bytecode_roundtrip() {
    let push32_bytes = 42u32.to_le_bytes();
    let push64_bytes = 100u64.to_le_bytes();
    let instructions: Vec<(RpnOp, Option<&[u8]>)> = vec![
        (RpnOp::Push32, Some(&push32_bytes)),
        (RpnOp::Push64, Some(&push64_bytes)),
        (RpnOp::Add, None),
        (RpnOp::Nop, None),
        (RpnOp::Sync, None),
    ];

    let bytecode = RpnVm::encode(&instructions);
    let decoded = RpnVm::decode(&bytecode).unwrap();

    assert_eq!(decoded.len(), 5);
    assert_eq!(decoded[0].0, RpnOp::Push32);
    assert_eq!(decoded[1].0, RpnOp::Push64);
    assert_eq!(decoded[2].0, RpnOp::Add);
    assert_eq!(decoded[3].0, RpnOp::Nop);
    assert_eq!(decoded[4].0, RpnOp::Sync);
}

#[test]
fn push32_promotes_to_u64_for_arithmetic() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Push32(3) + Push32(4) via u64 promotion
    let mut bc = encode_push32(3);
    bc.extend_from_slice(&encode_push32(4));
    bc.push(RpnOp::Add as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 7);
}
