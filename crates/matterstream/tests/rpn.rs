//! Tests for RPN stack language VM.

use matterstream::arena::TripleArena;
use matterstream::ova::ArenaId;
use matterstream::rpn::{GasConfig, RpnError, RpnOp, RpnVm};

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

// ============================================================
// Gas metering tests
// ============================================================

#[test]
fn gas_metering_tracks_consumption() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // 3 Nops -> 3 gas (cost_nop=1 each)
    let bc = vec![RpnOp::Nop as u8; 3];
    let trace = vm.execute_metered(&bc, &mut arenas).unwrap();
    assert_eq!(trace.opcodes_executed, 3);
    assert_eq!(trace.gas_consumed, 3); // 3 * cost_nop(1)
}

#[test]
fn gas_exhaustion_error() {
    let mut vm = RpnVm::with_gas(5);
    let mut arenas = TripleArena::new();

    // Push64 costs 1 gas, Add costs 2 gas
    // Push64 + Push64 + Add = 1 + 1 + 2 = 4 (ok)
    // Push64 + Push64 + Add + Push64 + Push64 + Add = 4 + 4 = 8 > 5
    let mut bc = encode_push64(1);
    bc.extend_from_slice(&encode_push64(2));
    bc.push(RpnOp::Add as u8);
    bc.extend_from_slice(&encode_push64(3));
    bc.extend_from_slice(&encode_push64(4));
    bc.push(RpnOp::Add as u8);

    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::GasExhausted { .. })));
}

#[test]
fn gas_sync_costs_more() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let bc = vec![RpnOp::Sync as u8];
    let trace = vm.execute_metered(&bc, &mut arenas).unwrap();
    assert_eq!(trace.gas_consumed, vm.gas.cost_sync); // 100
    assert_eq!(trace.syncs, 1);
}

#[test]
fn gas_custom_config() {
    let mut config = GasConfig::new(50);
    config.cost_arithmetic = 10;
    let mut vm = RpnVm::with_gas_config(config);
    let mut arenas = TripleArena::new();

    // Push64(1) + Push64(2) + Add = 1 + 1 + 10 = 12
    let mut bc = encode_push64(1);
    bc.extend_from_slice(&encode_push64(2));
    bc.push(RpnOp::Add as u8);

    let trace = vm.execute_metered(&bc, &mut arenas).unwrap();
    assert_eq!(trace.gas_consumed, 12);
}

// ============================================================
// Loop detection tests
// ============================================================

#[test]
fn backward_jump_detected() {
    let mut vm = RpnVm::new();
    vm.gas.max_backward_jumps = 5;
    let mut arenas = TripleArena::new();

    // Bytecode: [Jmp 0x0000] -> infinite backward loop to offset 0
    let bc = encode_jmp(0);

    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(
        result,
        Err(RpnError::BackwardJumpLimitExceeded { count: 6, limit: 5 })
    ));
}

#[test]
fn forward_jump_not_limited() {
    let mut vm = RpnVm::new();
    vm.gas.max_backward_jumps = 1; // strict limit
    let mut arenas = TripleArena::new();

    // Bytecode: Push64(1), Jmp(forward past end)
    let mut bc = encode_push64(1);
    let end = bc.len() + 9; // past the Jmp instruction itself
    bc.extend_from_slice(&encode_jmp(end as u64));

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 1);
    assert_eq!(vm.trace.forward_jumps, 1);
    assert_eq!(vm.trace.backward_jumps, 0);
}

#[test]
fn counted_loop_with_jmpif() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Implement: counter=5, loop: counter-=1, if counter>0 goto loop
    // Layout:
    //   0x00: Push64 5       (9 bytes -> next at 0x09)
    //   0x09: Push64 1       (9 bytes -> next at 0x12)
    //   0x12: Sub             (1 byte  -> next at 0x13)
    //   0x13: Dup             (1 byte  -> next at 0x14)
    //   0x14: Push64 0       (9 bytes -> next at 0x1d)
    //   0x1d: CmpGt           (1 byte  -> next at 0x1e)
    //   0x1e: JmpIf 0x09     (9 bytes -> next at 0x27)
    //   0x27: Halt

    let mut bc = encode_push64(5);           // 0x00
    let loop_start = bc.len();               // 0x09
    bc.extend_from_slice(&encode_push64(1)); // 0x09
    bc.push(RpnOp::Sub as u8);              // 0x12
    bc.push(RpnOp::Dup as u8);             // 0x13
    bc.extend_from_slice(&encode_push64(0)); // 0x14
    bc.push(RpnOp::CmpGt as u8);           // 0x1d
    bc.extend_from_slice(&encode_jmpif(loop_start as u64)); // 0x1e
    bc.push(RpnOp::Halt as u8);            // 0x27

    vm.execute(&bc, &mut arenas).unwrap();

    // After 5 iterations: 5-1-1-1-1-1 = 0
    assert_eq!(vm.stack[0].as_u64().unwrap(), 0);
    // 4 backward jumps (iterations 1-4 jump back, iteration 5 falls through)
    assert_eq!(vm.trace.backward_jumps, 4);
    assert!(vm.trace.halted);
}

// ============================================================
// New opcode tests
// ============================================================

#[test]
fn halt_stops_execution() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(42);
    bc.push(RpnOp::Halt as u8);
    bc.extend_from_slice(&encode_push64(99)); // should not execute

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u64().unwrap(), 42);
    assert!(vm.trace.halted);
}

#[test]
fn jmp_unconditional() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Jmp over Push64(99) to Push64(42), Halt
    let jmp_target = 9 + 9; // Jmp(9) + Push64(9) = 18
    let mut bc = encode_jmp(jmp_target as u64); // 0x00: Jmp -> 0x12
    bc.extend_from_slice(&encode_push64(99));   // 0x09: Push64 99 (skipped)
    bc.extend_from_slice(&encode_push64(42));   // 0x12: Push64 42
    bc.push(RpnOp::Halt as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u64().unwrap(), 42);
}

#[test]
fn jmpif_conditional_true() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Push 1 (truthy), JmpIf target, Push 99 (skipped), target: Push 42
    let jmpif_end = 9 + 9 + 9; // Push64(9) + JmpIf(9) + Push64(9) = 27
    let mut bc = encode_push64(1);                     // condition: true
    bc.extend_from_slice(&encode_jmpif(jmpif_end as u64));
    bc.extend_from_slice(&encode_push64(99));           // skipped
    bc.extend_from_slice(&encode_push64(42));           // target
    bc.push(RpnOp::Halt as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u64().unwrap(), 42);
}

#[test]
fn jmpif_conditional_false() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Push 0 (falsy), JmpIf target, Push 99 (executed), Halt
    let mut bc = encode_push64(0);                        // condition: false
    bc.extend_from_slice(&encode_jmpif(100));              // target won't be reached
    bc.extend_from_slice(&encode_push64(99));              // executed because condition is false
    bc.push(RpnOp::Halt as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u64().unwrap(), 99);
}

#[test]
fn mod_operation() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(17);
    bc.extend_from_slice(&encode_push64(5));
    bc.push(RpnOp::Mod as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 2); // 17 % 5 = 2
}

#[test]
fn mod_division_by_zero() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(10);
    bc.extend_from_slice(&encode_push64(0));
    bc.push(RpnOp::Mod as u8);

    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::DivisionByZero)));
}

#[test]
fn cmp_eq_true() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(42);
    bc.extend_from_slice(&encode_push64(42));
    bc.push(RpnOp::CmpEq as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 1);
}

#[test]
fn cmp_eq_false() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(42);
    bc.extend_from_slice(&encode_push64(43));
    bc.push(RpnOp::CmpEq as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 0);
}

#[test]
fn cmp_lt() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(3);
    bc.extend_from_slice(&encode_push64(5));
    bc.push(RpnOp::CmpLt as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 1); // 3 < 5

    vm.stack.clear();
    let mut bc2 = encode_push64(5);
    bc2.extend_from_slice(&encode_push64(3));
    bc2.push(RpnOp::CmpLt as u8);

    vm.execute(&bc2, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 0); // 5 < 3 = false
}

#[test]
fn cmp_gt() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(10);
    bc.extend_from_slice(&encode_push64(5));
    bc.push(RpnOp::CmpGt as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 1); // 10 > 5
}

#[test]
fn invalid_jump_target_error() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Jmp to offset way past end of bytecode
    let bc = encode_jmp(9999);
    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::InvalidJumpTarget(9999))));
}

#[test]
fn trace_max_stack_depth() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = encode_push64(1);
    bc.extend_from_slice(&encode_push64(2));
    bc.extend_from_slice(&encode_push64(3));
    bc.push(RpnOp::Drop as u8);
    bc.push(RpnOp::Drop as u8);

    let trace = vm.execute_metered(&bc, &mut arenas).unwrap();
    assert_eq!(trace.max_stack_depth_seen, 3);
    assert_eq!(vm.stack.len(), 1); // only 1 left after 2 drops
}

#[test]
fn disassemble_output() {
    let mut bc = encode_push64(42);
    bc.extend_from_slice(&encode_push32(10));
    bc.push(RpnOp::Add as u8);
    let jmp_target = bc.len() + 9;
    bc.extend_from_slice(&encode_jmp(jmp_target as u64));
    bc.push(RpnOp::Halt as u8);

    let disasm = RpnVm::disassemble(&bc).unwrap();
    assert!(disasm.contains("Push64 42"));
    assert!(disasm.contains("Push32 10"));
    assert!(disasm.contains("Add"));
    assert!(disasm.contains("Jmp"));
    assert!(disasm.contains("Halt"));
}

#[test]
fn encode_decode_new_opcodes_roundtrip() {
    let jmp_target = 42u64.to_le_bytes();
    let jmpif_target = 100u64.to_le_bytes();
    let instructions: Vec<(RpnOp, Option<&[u8]>)> = vec![
        (RpnOp::Jmp, Some(&jmp_target)),
        (RpnOp::JmpIf, Some(&jmpif_target)),
        (RpnOp::Halt, None),
        (RpnOp::Mod, None),
        (RpnOp::CmpEq, None),
        (RpnOp::CmpLt, None),
        (RpnOp::CmpGt, None),
    ];

    let bytecode = RpnVm::encode(&instructions);
    let decoded = RpnVm::decode(&bytecode).unwrap();

    assert_eq!(decoded.len(), 7);
    assert_eq!(decoded[0].0, RpnOp::Jmp);
    assert_eq!(decoded[1].0, RpnOp::JmpIf);
    assert_eq!(decoded[2].0, RpnOp::Halt);
    assert_eq!(decoded[3].0, RpnOp::Mod);
    assert_eq!(decoded[4].0, RpnOp::CmpEq);
    assert_eq!(decoded[5].0, RpnOp::CmpLt);
    assert_eq!(decoded[6].0, RpnOp::CmpGt);
}

#[test]
fn fibonacci_loop() {
    // Compute fib(10) = 55 using a loop with arena memory
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let ova_n = arenas.alloc_nursery(4).unwrap();
    let ova_a = arenas.alloc_nursery(4).unwrap();
    let ova_b = arenas.alloc_nursery(4).unwrap();

    let mut bc = Vec::new();

    // Store n=10
    bc.extend_from_slice(&encode_push64(10));
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

    // Placeholder JmpIf (jump to body when condition true)
    let jmpif_pos = bc.len();
    bc.extend_from_slice(&encode_jmpif(0));

    // If condition false: jump to end
    let jmp_end_pos = bc.len();
    bc.extend_from_slice(&encode_jmp(0));

    // Body start
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

    // Jump back to loop
    bc.extend_from_slice(&encode_jmp(loop_start as u64));

    let loop_end = bc.len();

    // Load result (a = fib(n) after n iterations)
    bc.extend_from_slice(&encode_push32(ova_a.0));
    bc.push(RpnOp::Load as u8);
    bc.push(RpnOp::Halt as u8);

    // Fix up jump targets
    let body_bytes = (body_start as u64).to_le_bytes();
    bc[jmpif_pos + 1..jmpif_pos + 9].copy_from_slice(&body_bytes);

    let end_bytes = (loop_end as u64).to_le_bytes();
    bc[jmp_end_pos + 1..jmp_end_pos + 9].copy_from_slice(&end_bytes);

    vm.execute(&bc, &mut arenas).unwrap();

    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u64().unwrap(), 55); // fib(10) = 55
    assert_eq!(vm.trace.backward_jumps, 10); // 10 loop iterations
}
