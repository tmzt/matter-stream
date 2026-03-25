//! Tests for UI draw opcodes, state stack, and draw limits.

use matterstream_vm::rpn::{MtuiOp, RpnOp, RpnVm, RpnError};
use matterstream_vm::ui_vm::{UiDrawCmd, UI_STATE_STACK_MAX};
use matterstream_vm_arena::TripleArena;

/// Build bytecode manually since OR page ops (0x80+) can't go through RpnVm::encode.
fn push32(bc: &mut Vec<u8>, val: u32) {
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&val.to_le_bytes());
}

#[test]
fn test_ui_set_color() {
    let color = 0xFF0000FFu32; // red, full alpha
    let mut bc = Vec::new();
    push32(&mut bc, color);
    bc.push(MtuiOp::SetColor.byte());
    bc.push(RpnOp::Halt as u8);
    let vm = run(&bc);
    assert_eq!(vm.ui_state.color, color);
}

#[test]
fn test_ui_box() {
    let color = 0x00FF00FFu32;
    let mut bc = Vec::new();
    push32(&mut bc, color);
    bc.push(MtuiOp::SetColor.byte());
    push32(&mut bc, 10);  // x
    push32(&mut bc, 20);  // y
    push32(&mut bc, 100); // w
    push32(&mut bc, 50);  // h
    bc.push(MtuiOp::Box.byte());
    bc.push(RpnOp::Halt as u8);
    let vm = run(&bc);
    assert_eq!(vm.ui_draws.len(), 1);
    assert_eq!(
        vm.ui_draws[0],
        UiDrawCmd::Box {
            x: 10,
            y: 20,
            w: 100,
            h: 50,
            color,
        }
    );
}

#[test]
fn test_ui_slab() {
    let color = 0xFFFFFFFFu32;
    let mut bc = Vec::new();
    push32(&mut bc, color);
    bc.push(MtuiOp::SetColor.byte());
    push32(&mut bc, 0);   // x
    push32(&mut bc, 0);   // y
    push32(&mut bc, 200); // w
    push32(&mut bc, 100); // h
    push32(&mut bc, 8);   // radius
    bc.push(MtuiOp::Slab.byte());
    bc.push(RpnOp::Halt as u8);
    let vm = run(&bc);
    assert_eq!(vm.ui_draws.len(), 1);
    match &vm.ui_draws[0] {
        UiDrawCmd::Slab { radius, .. } => assert_eq!(*radius, 8),
        _ => panic!("Expected Slab draw command"),
    }
}

#[test]
fn test_ui_circle() {
    let color = 0xFF0000FFu32;
    let mut bc = Vec::new();
    push32(&mut bc, color);
    bc.push(MtuiOp::SetColor.byte());
    push32(&mut bc, 50);  // cx
    push32(&mut bc, 50);  // cy
    push32(&mut bc, 25);  // r
    bc.push(MtuiOp::Circle.byte());
    bc.push(RpnOp::Halt as u8);
    let vm = run(&bc);
    assert_eq!(vm.ui_draws.len(), 1);
    assert_eq!(
        vm.ui_draws[0],
        UiDrawCmd::Circle {
            x: 50,
            y: 50,
            r: 25,
            color,
        }
    );
}

#[test]
fn test_ui_line() {
    let color = 0xFFFFFFFFu32;
    let mut bc = Vec::new();
    push32(&mut bc, color);
    bc.push(MtuiOp::SetColor.byte());
    push32(&mut bc, 0);   // x1
    push32(&mut bc, 0);   // y1
    push32(&mut bc, 100); // x2
    push32(&mut bc, 100); // y2
    bc.push(MtuiOp::Line.byte());
    bc.push(RpnOp::Halt as u8);
    let vm = run(&bc);
    assert_eq!(vm.ui_draws.len(), 1);
    assert_eq!(
        vm.ui_draws[0],
        UiDrawCmd::Line {
            x1: 0,
            y1: 0,
            x2: 100,
            y2: 100,
            color,
        }
    );
}

#[test]
fn test_ui_push_pop_state() {
    let mut bc = Vec::new();
    // Set color to red
    push32(&mut bc, 0xFF0000FFu32);
    bc.push(MtuiOp::SetColor.byte());
    // Push state
    bc.push(MtuiOp::PushState.byte());
    // Change color to blue
    push32(&mut bc, 0x0000FFFFu32);
    bc.push(MtuiOp::SetColor.byte());
    // Pop state — should restore red
    bc.push(MtuiOp::PopState.byte());
    bc.push(RpnOp::Halt as u8);
    let vm = run(&bc);
    assert_eq!(vm.ui_state.color, 0xFF0000FF);
}

#[test]
fn test_ui_state_stack_overflow() {
    // Push state 17 times (limit is 16)
    let mut bc = Vec::new();
    for _ in 0..=UI_STATE_STACK_MAX {
        bc.push(MtuiOp::PushState.byte());
    }
    bc.push(RpnOp::Halt as u8);
    let result = run_result(&bc);
    assert_eq!(result.unwrap_err(), RpnError::UiStateStackOverflow);
}

#[test]
fn test_ui_state_stack_underflow() {
    let mut bc = Vec::new();
    bc.push(MtuiOp::PopState.byte());
    bc.push(RpnOp::Halt as u8);
    let result = run_result(&bc);
    assert_eq!(result.unwrap_err(), RpnError::UiStateStackUnderflow);
}

#[test]
fn test_ui_apply_offset() {
    let mut bc = Vec::new();
    push32(&mut bc, 0xFFFFFFFFu32);
    bc.push(MtuiOp::SetColor.byte());
    // Apply offset (10, 20)
    push32(&mut bc, 10);
    push32(&mut bc, 20);
    bc.push(MtuiOp::ApplyOffset.byte());
    // Draw box at (5, 5) → should become (15, 25)
    push32(&mut bc, 5);
    push32(&mut bc, 5);
    push32(&mut bc, 50);
    push32(&mut bc, 50);
    bc.push(MtuiOp::Box.byte());
    bc.push(RpnOp::Halt as u8);
    let vm = run(&bc);
    match &vm.ui_draws[0] {
        UiDrawCmd::Box { x, y, .. } => {
            assert_eq!(*x, 15);
            assert_eq!(*y, 25);
        }
        _ => panic!("Expected Box"),
    }
}

#[test]
fn test_ui_text_str() {
    let mut vm = RpnVm::new();
    vm.string_table = vec!["Hello".to_string(), "World".to_string()];
    let mut arenas = TripleArena::new();

    let mut bc = Vec::new();
    push32(&mut bc, 0xFFFFFFFFu32);
    bc.push(MtuiOp::SetColor.byte());
    push32(&mut bc, 10);  // x
    push32(&mut bc, 20);  // y
    push32(&mut bc, 16);  // size
    push32(&mut bc, 1);   // str_idx = 1 ("World")
    bc.push(MtuiOp::TextStr.byte());
    bc.push(RpnOp::Halt as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.ui_draws.len(), 1);
    match &vm.ui_draws[0] {
        UiDrawCmd::TextStr { str_idx, .. } => assert_eq!(*str_idx, 1),
        _ => panic!("Expected TextStr"),
    }
}

// Pixel-level tests (rgba, blend_pixel, render_*) moved to matterstream-ui-soft

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
