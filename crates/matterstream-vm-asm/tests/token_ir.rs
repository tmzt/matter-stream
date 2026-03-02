//! Tests that IR tokens are typed (no strings), and IDs are opaque.

use matterstream_vm_asm::{Asm, LabelId, GlobalId, StringId};

#[test]
fn test_label_ids_are_sequential() {
    let mut asm = Asm::new();
    let l0 = asm.def_label();
    let l1 = asm.def_label();
    let l2 = asm.def_label();
    assert_eq!(l0, LabelId(0));
    assert_eq!(l1, LabelId(1));
    assert_eq!(l2, LabelId(2));
}

#[test]
fn test_global_ids_are_sequential() {
    let mut asm = Asm::new();
    let g0 = asm.def_global();
    let g1 = asm.def_global();
    assert_eq!(g0, GlobalId(0));
    assert_eq!(g1, GlobalId(1));
}

#[test]
fn test_string_ids_are_sequential() {
    let mut asm = Asm::new();
    let s0 = asm.def_string("hello");
    let s1 = asm.def_string("world");
    assert_eq!(s0, StringId(0));
    assert_eq!(s1, StringId(1));
}

#[test]
fn test_string_table_in_output() {
    let mut asm = Asm::new();
    let _s0 = asm.def_string("hello");
    let _s1 = asm.def_string("world");
    asm.halt();

    let output = asm.finish().unwrap();
    assert_eq!(output.string_table.len(), 2);
    assert_eq!(output.string_table[0], "hello");
    assert_eq!(output.string_table[1], "world");
}

#[test]
fn test_global_count_in_output() {
    let mut asm = Asm::new();
    asm.def_global();
    asm.def_global();
    asm.def_global();
    asm.halt();

    let output = asm.finish().unwrap();
    assert_eq!(output.global_count, 3);
}

#[test]
fn test_minimal_program() {
    let mut asm = Asm::new();
    asm.halt();
    let output = asm.finish().unwrap();
    assert_eq!(output.bytecode.len(), 1);
    assert_eq!(output.bytecode[0], 0x15); // Halt opcode
}

#[test]
fn test_push32_emits_5_bytes() {
    let mut asm = Asm::new();
    asm.push32(42);
    let output = asm.finish().unwrap();
    assert_eq!(output.bytecode.len(), 5);
    assert_eq!(output.bytecode[0], 0x01); // Push32 opcode
    assert_eq!(
        u32::from_le_bytes(output.bytecode[1..5].try_into().unwrap()),
        42
    );
}

#[test]
fn test_push64_emits_9_bytes() {
    let mut asm = Asm::new();
    asm.push64(1234567890);
    let output = asm.finish().unwrap();
    assert_eq!(output.bytecode.len(), 9);
    assert_eq!(output.bytecode[0], 0x02); // Push64 opcode
}
