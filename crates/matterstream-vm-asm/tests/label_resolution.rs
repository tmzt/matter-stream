//! Tests for label resolution: forward jumps, backward jumps, errors.

use matterstream_vm_asm::{Asm, AsmError};
use matterstream_vm::rpn::RpnOp;

#[test]
fn test_forward_jump() {
    let mut asm = Asm::new();
    let end = asm.def_label();

    asm.jmp(end);
    asm.push32(99); // should be skipped
    asm.mark(end);
    asm.halt();

    let output = asm.finish().unwrap();
    // Verify the Jmp target points past the Push32
    // Jmp = 9 bytes (1 + 8), Push32 = 5 bytes → target should be 14
    assert_eq!(output.bytecode[0], RpnOp::Jmp as u8);
    let target = u64::from_le_bytes(output.bytecode[1..9].try_into().unwrap());
    assert_eq!(target, 14);
}

#[test]
fn test_backward_jump() {
    let mut asm = Asm::new();
    let top = asm.def_label();

    asm.mark(top);
    asm.nop();
    asm.halt(); // stop so we don't loop forever

    let output = asm.finish().unwrap();
    // Label at offset 0, Nop at 0, Halt at 1
    assert_eq!(output.bytecode.len(), 2);
}

#[test]
fn test_conditional_forward_jump() {
    let mut asm = Asm::new();
    let skip = asm.def_label();

    asm.push64(1); // condition = true
    asm.jmp_if(skip);
    asm.push32(99); // skipped when condition true
    asm.mark(skip);
    asm.push32(42);
    asm.halt();

    let output = asm.finish().unwrap();

    // Execute to verify
    let mut vm = matterstream_vm::rpn::RpnVm::new();
    let mut arenas = matterstream_vm_arena::TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    // Should have 42 on stack (99 was skipped)
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u32(), Some(42));
}

#[test]
fn test_unresolved_label_error() {
    let mut asm = Asm::new();
    let label = asm.def_label();
    asm.jmp(label); // never mark()'d

    let result = asm.finish();
    assert!(matches!(result, Err(AsmError::UnresolvedLabel(_))));
}

#[test]
fn test_duplicate_label_error() {
    let mut asm = Asm::new();
    let label = asm.def_label();
    asm.mark(label);
    asm.mark(label); // duplicate!
    asm.halt();

    let result = asm.finish();
    assert!(matches!(result, Err(AsmError::DuplicateLabel(_))));
}

#[test]
fn test_multiple_labels() {
    let mut asm = Asm::new();
    let a = asm.def_label();
    let b = asm.def_label();
    let c = asm.def_label();

    asm.jmp(b);
    asm.mark(a);
    asm.push32(1);
    asm.jmp(c);
    asm.mark(b);
    asm.push32(2);
    asm.jmp(a);
    asm.mark(c);
    asm.halt();

    let output = asm.finish().unwrap();

    let mut vm = matterstream_vm::rpn::RpnVm::new();
    let mut arenas = matterstream_vm_arena::TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    // Execution path: jmp(b) → push32(2) → jmp(a) → push32(1) → jmp(c) → halt
    assert_eq!(vm.stack.len(), 2);
    assert_eq!(vm.stack[0].as_u32(), Some(2));
    assert_eq!(vm.stack[1].as_u32(), Some(1));
}

#[test]
fn test_loop_with_backward_jump() {
    // Count from 0 to 3: push 0, loop: dup, push 3, cmp_lt, jmp_if(loop_body), halt
    let mut asm = Asm::new();
    let loop_top = asm.def_label();
    let loop_end = asm.def_label();

    // counter = 0
    asm.push64(0);

    // loop_top:
    asm.mark(loop_top);
    // dup counter, compare with 3
    asm.dup();
    asm.push64(3);
    asm.cmp_ge(); // counter >= 3?
    asm.jmp_if(loop_end);

    // counter += 1
    asm.push64(1);
    asm.add();
    asm.jmp(loop_top);

    asm.mark(loop_end);
    asm.halt();

    let output = asm.finish().unwrap();

    let mut vm = matterstream_vm::rpn::RpnVm::new();
    let mut arenas = matterstream_vm_arena::TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    // Stack should have: [3] (counter reached 3 and the cmp_ge result was dropped via jmp_if consuming it)
    // Actually: the JmpIf consumed the cmp result, counter=3 remains
    assert_eq!(vm.stack[0].as_u64(), Some(3));
}
