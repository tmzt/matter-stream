//! Tests for UI helper methods and shorthand opcodes.

use matterstream_vm_asm::Asm;
use matterstream_vm::rpn::RpnVm;
use matterstream_vm::ui_vm::UiDrawCmd;
use matterstream_vm_arena::TripleArena;

#[test]
fn test_set_color_helper() {
    let mut asm = Asm::new();
    asm.set_color(255, 0, 0, 255);
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    assert_eq!(vm.ui_state.color, 0xFF0000FF);
}

#[test]
fn test_draw_box_helper() {
    let mut asm = Asm::new();
    asm.set_color(0, 255, 0, 255);
    asm.draw_box(10, 20, 100, 50);
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    assert_eq!(vm.ui_draws.len(), 1);
    assert_eq!(
        vm.ui_draws[0],
        UiDrawCmd::Box {
            x: 10,
            y: 20,
            w: 100,
            h: 50,
            color: 0x00FF00FF,
        }
    );
}

#[test]
fn test_draw_slab_helper() {
    let mut asm = Asm::new();
    asm.set_color(255, 255, 255, 255);
    asm.draw_slab(0, 0, 200, 100, 8);
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    assert_eq!(vm.ui_draws.len(), 1);
    match &vm.ui_draws[0] {
        UiDrawCmd::Slab { w, h, radius, .. } => {
            assert_eq!(*w, 200);
            assert_eq!(*h, 100);
            assert_eq!(*radius, 8);
        }
        _ => panic!("Expected Slab"),
    }
}

#[test]
fn test_draw_circle_helper() {
    let mut asm = Asm::new();
    asm.set_color(255, 0, 0, 255);
    asm.draw_circle(50, 50, 25);
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    assert_eq!(vm.ui_draws.len(), 1);
    assert_eq!(
        vm.ui_draws[0],
        UiDrawCmd::Circle {
            x: 50,
            y: 50,
            r: 25,
            color: 0xFF0000FF,
        }
    );
}

#[test]
fn test_draw_text_str_helper() {
    let mut asm = Asm::new();
    let hello = asm.def_string("Hello");
    asm.set_color(255, 255, 255, 255);
    asm.draw_text_str(10, 20, 16, hello);
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    vm.string_table = output.string_table;
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    assert_eq!(vm.ui_draws.len(), 1);
    match &vm.ui_draws[0] {
        UiDrawCmd::TextStr { str_idx, size, .. } => {
            assert_eq!(*str_idx, 0);
            assert_eq!(*size, 16);
        }
        _ => panic!("Expected TextStr"),
    }
}

#[test]
fn test_draw_line_helper() {
    let mut asm = Asm::new();
    asm.set_color(255, 255, 255, 255);
    asm.draw_line(0, 0, 100, 100);
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    assert_eq!(vm.ui_draws.len(), 1);
    assert_eq!(
        vm.ui_draws[0],
        UiDrawCmd::Line {
            x1: 0,
            y1: 0,
            x2: 100,
            y2: 100,
            color: 0xFFFFFFFF,
        }
    );
}

#[test]
fn test_arithmetic_shorthands() {
    let mut asm = Asm::new();
    asm.push64(10);
    asm.push64(3);
    asm.add();
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    assert_eq!(vm.stack[0].as_u64(), Some(13));
}

#[test]
fn test_comparison_shorthands() {
    let mut asm = Asm::new();
    asm.push64(10);
    asm.push64(20);
    asm.cmp_lt();
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    assert_eq!(vm.stack[0].as_u64(), Some(1));
}

#[test]
fn test_ui_push_pop_state_helpers() {
    let mut asm = Asm::new();
    asm.set_color(255, 0, 0, 255);
    asm.ui_push_state();
    asm.set_color(0, 0, 255, 255);
    asm.ui_pop_state();
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    // Color should be restored to red
    assert_eq!(vm.ui_state.color, 0xFF0000FF);
}

#[test]
fn test_ui_set_offset_helper() {
    let mut asm = Asm::new();
    asm.set_color(255, 255, 255, 255);
    asm.ui_set_offset(100, 200);
    asm.draw_box(0, 0, 50, 50);
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    match &vm.ui_draws[0] {
        UiDrawCmd::Box { x, y, .. } => {
            assert_eq!(*x, 100);
            assert_eq!(*y, 200);
        }
        _ => panic!("Expected Box"),
    }
}
