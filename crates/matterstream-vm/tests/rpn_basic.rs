//! Tests ported from main-vm0-stackmachine + basic VM tests for all original opcodes.

use matterstream_vm::rpn::{RpnError, RpnOp, RpnVm};
use matterstream_vm_arena::TripleArena;

fn run(bytecode: &[u8]) -> RpnVm {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(bytecode, &mut arenas).unwrap();
    vm
}

fn run_result(bytecode: &[u8]) -> Result<RpnVm, RpnError> {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(bytecode, &mut arenas)?;
    Ok(vm)
}

fn encode(instructions: &[(RpnOp, Option<&[u8]>)]) -> Vec<u8> {
    RpnVm::encode(instructions)
}

#[test]
fn test_nop() {
    let bc = encode(&[(RpnOp::Nop, None), (RpnOp::Nop, None)]);
    let vm = run(&bc);
    assert!(vm.stack.is_empty());
}

#[test]
fn test_push32() {
    let bc = encode(&[(RpnOp::Push32, Some(&42u32.to_le_bytes()))]);
    let vm = run(&bc);
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u32(), Some(42));
}

#[test]
fn test_push64() {
    let val = 0xDEADBEEF_CAFEBABE_u64;
    let bc = encode(&[(RpnOp::Push64, Some(&val.to_le_bytes()))]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(val));
}

#[test]
fn test_dup() {
    let bc = encode(&[
        (RpnOp::Push32, Some(&10u32.to_le_bytes())),
        (RpnOp::Dup, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack.len(), 2);
    assert_eq!(vm.stack[0].as_u32(), Some(10));
    assert_eq!(vm.stack[1].as_u32(), Some(10));
}

#[test]
fn test_drop() {
    let bc = encode(&[
        (RpnOp::Push32, Some(&10u32.to_le_bytes())),
        (RpnOp::Push32, Some(&20u32.to_le_bytes())),
        (RpnOp::Drop, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u32(), Some(10));
}

#[test]
fn test_swap() {
    let bc = encode(&[
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::Push32, Some(&2u32.to_le_bytes())),
        (RpnOp::Swap, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u32(), Some(2));
    assert_eq!(vm.stack[1].as_u32(), Some(1));
}

#[test]
fn test_add() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::Push64, Some(&20u64.to_le_bytes())),
        (RpnOp::Add, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(30));
}

#[test]
fn test_sub() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&50u64.to_le_bytes())),
        (RpnOp::Push64, Some(&20u64.to_le_bytes())),
        (RpnOp::Sub, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(30));
}

#[test]
fn test_mul() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&6u64.to_le_bytes())),
        (RpnOp::Push64, Some(&7u64.to_le_bytes())),
        (RpnOp::Mul, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(42));
}

#[test]
fn test_div() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&100u64.to_le_bytes())),
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::Div, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(10));
}

#[test]
fn test_div_by_zero() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::Push64, Some(&0u64.to_le_bytes())),
        (RpnOp::Div, None),
    ]);
    let result = run_result(&bc);
    assert_eq!(result.unwrap_err(), RpnError::DivisionByZero);
}

#[test]
fn test_mod() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&17u64.to_le_bytes())),
        (RpnOp::Push64, Some(&5u64.to_le_bytes())),
        (RpnOp::Mod, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(2));
}

#[test]
fn test_cmp_eq_true() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&42u64.to_le_bytes())),
        (RpnOp::Push64, Some(&42u64.to_le_bytes())),
        (RpnOp::CmpEq, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(1));
}

#[test]
fn test_cmp_eq_false() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&42u64.to_le_bytes())),
        (RpnOp::Push64, Some(&43u64.to_le_bytes())),
        (RpnOp::CmpEq, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(0));
}

#[test]
fn test_cmp_lt() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::Push64, Some(&20u64.to_le_bytes())),
        (RpnOp::CmpLt, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(1));
}

#[test]
fn test_cmp_gt() {
    let bc = encode(&[
        (RpnOp::Push64, Some(&20u64.to_le_bytes())),
        (RpnOp::Push64, Some(&10u64.to_le_bytes())),
        (RpnOp::CmpGt, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u64(), Some(1));
}

#[test]
fn test_halt() {
    let bc = encode(&[
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::Halt, None),
        (RpnOp::Push32, Some(&2u32.to_le_bytes())), // should not execute
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack.len(), 1);
    assert!(vm.trace.halted);
}

#[test]
fn test_jmp() {
    // Push 1, Jmp over Push 2, Push 3
    let mut bc = Vec::new();
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&1u32.to_le_bytes());
    // Jmp to offset 18 (past the Push32 2)
    bc.push(RpnOp::Jmp as u8);
    bc.extend_from_slice(&18u64.to_le_bytes());
    // This Push32 2 should be skipped (offset 14)
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&2u32.to_le_bytes());
    // Push32 3 at offset 18 (should execute)
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&3u32.to_le_bytes());

    let vm = run(&bc);
    assert_eq!(vm.stack.len(), 2);
    assert_eq!(vm.stack[0].as_u32(), Some(1));
    assert_eq!(vm.stack[1].as_u32(), Some(3));
}

#[test]
fn test_jmp_if_taken() {
    let mut bc = Vec::new();
    // Push 1 (condition = true)
    bc.push(RpnOp::Push64 as u8);
    bc.extend_from_slice(&1u64.to_le_bytes());
    // JmpIf to offset 23
    bc.push(RpnOp::JmpIf as u8);
    bc.extend_from_slice(&23u64.to_le_bytes());
    // Push32 99 (skipped)
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&99u32.to_le_bytes());
    // Push32 42 at offset 23
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&42u32.to_le_bytes());

    let vm = run(&bc);
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u32(), Some(42));
}

#[test]
fn test_jmp_if_not_taken() {
    let mut bc = Vec::new();
    // Push 0 (condition = false)
    bc.push(RpnOp::Push64 as u8);
    bc.extend_from_slice(&0u64.to_le_bytes());
    // JmpIf to offset 23
    bc.push(RpnOp::JmpIf as u8);
    bc.extend_from_slice(&23u64.to_le_bytes());
    // Push32 99 (should execute)
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&99u32.to_le_bytes());

    let vm = run(&bc);
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u32(), Some(99));
}

#[test]
fn test_map_operations() {
    let bc = encode(&[
        (RpnOp::MapNew, None),
        // Set key 1 = 42
        (RpnOp::Push64, Some(&1u64.to_le_bytes())),
        (RpnOp::Push32, Some(&42u32.to_le_bytes())),
        (RpnOp::MapSet, None),
        // Get key 1
        (RpnOp::Push64, Some(&1u64.to_le_bytes())),
        (RpnOp::MapGet, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.stack[0].as_u32(), Some(42));
}

#[test]
fn test_stack_underflow() {
    let bc = encode(&[(RpnOp::Drop, None)]);
    let result = run_result(&bc);
    assert_eq!(result.unwrap_err(), RpnError::StackUnderflow);
}

#[test]
fn test_invalid_opcode() {
    let bc = vec![0xFE]; // invalid opcode
    let result = run_result(&bc);
    assert_eq!(result.unwrap_err(), RpnError::InvalidOpcode(0xFE));
}

#[test]
fn test_gas_metering() {
    let mut vm = RpnVm::with_gas(10);
    let mut arenas = TripleArena::new();
    // Each Nop costs 1 gas, so 11 Nops should exhaust the budget
    let bc: Vec<u8> = vec![RpnOp::Nop as u8; 11];
    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::GasExhausted { .. })));
}

#[test]
fn test_encode_decode_roundtrip() {
    let bc = encode(&[
        (RpnOp::Push32, Some(&42u32.to_le_bytes())),
        (RpnOp::Push64, Some(&100u64.to_le_bytes())),
        (RpnOp::Add, None),
        (RpnOp::Halt, None),
    ]);
    let decoded = RpnVm::decode(&bc).unwrap();
    assert_eq!(decoded.len(), 4);
    assert_eq!(decoded[0].0, RpnOp::Push32);
    assert_eq!(decoded[1].0, RpnOp::Push64);
    assert_eq!(decoded[2].0, RpnOp::Add);
    assert_eq!(decoded[3].0, RpnOp::Halt);
}

#[test]
fn test_disassemble() {
    let bc = encode(&[
        (RpnOp::Push32, Some(&42u32.to_le_bytes())),
        (RpnOp::Halt, None),
    ]);
    let disasm = RpnVm::disassemble(&bc).unwrap();
    assert!(disasm.contains("Push32 42"));
    assert!(disasm.contains("Halt"));
}

#[test]
fn test_banks_persist_between_executions() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // First execution: store 42 in int bank slot 0
    vm.int_bank[0] = 42;
    let bc1 = encode(&[(RpnOp::Nop, None)]);
    vm.execute(&bc1, &mut arenas).unwrap();

    // Bank should still have 42
    assert_eq!(vm.int_bank[0], 42);

    // Second execution: int bank still has 42
    let bc2 = encode(&[(RpnOp::Nop, None)]);
    vm.execute(&bc2, &mut arenas).unwrap();
    assert_eq!(vm.int_bank[0], 42);
}
