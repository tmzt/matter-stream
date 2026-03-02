//! Tests for UI draw opcodes, state stack, and draw limits.

use matterstream_vm::rpn::{RpnError, RpnOp, RpnVm};
use matterstream_vm::ui_vm::{
    self, UiDrawCmd, UI_STATE_STACK_MAX,
    rgba, rgba_unpack, blend_pixel,
};
use matterstream_vm_arena::TripleArena;

fn encode(instructions: &[(RpnOp, Option<&[u8]>)]) -> Vec<u8> {
    RpnVm::encode(instructions)
}

#[test]
fn test_ui_set_color() {
    let color = 0xFF0000FFu32; // red, full alpha
    let bc = encode(&[
        (RpnOp::Push32, Some(&color.to_le_bytes())),
        (RpnOp::UiSetColor, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.ui_state.color, color);
}

#[test]
fn test_ui_box() {
    let color = 0x00FF00FFu32;
    let bc = encode(&[
        (RpnOp::Push32, Some(&color.to_le_bytes())),
        (RpnOp::UiSetColor, None),
        (RpnOp::Push32, Some(&10u32.to_le_bytes())),  // x
        (RpnOp::Push32, Some(&20u32.to_le_bytes())),  // y
        (RpnOp::Push32, Some(&100u32.to_le_bytes())), // w
        (RpnOp::Push32, Some(&50u32.to_le_bytes())),  // h
        (RpnOp::UiBox, None),
    ]);
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
    let bc = encode(&[
        (RpnOp::Push32, Some(&color.to_le_bytes())),
        (RpnOp::UiSetColor, None),
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),   // x
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),   // y
        (RpnOp::Push32, Some(&200u32.to_le_bytes())), // w
        (RpnOp::Push32, Some(&100u32.to_le_bytes())), // h
        (RpnOp::Push32, Some(&8u32.to_le_bytes())),   // radius
        (RpnOp::UiSlab, None),
    ]);
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
    let bc = encode(&[
        (RpnOp::Push32, Some(&color.to_le_bytes())),
        (RpnOp::UiSetColor, None),
        (RpnOp::Push32, Some(&50u32.to_le_bytes())),  // cx
        (RpnOp::Push32, Some(&50u32.to_le_bytes())),  // cy
        (RpnOp::Push32, Some(&25u32.to_le_bytes())),  // r
        (RpnOp::UiCircle, None),
    ]);
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
    let bc = encode(&[
        (RpnOp::Push32, Some(&color.to_le_bytes())),
        (RpnOp::UiSetColor, None),
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),   // x1
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),   // y1
        (RpnOp::Push32, Some(&100u32.to_le_bytes())), // x2
        (RpnOp::Push32, Some(&100u32.to_le_bytes())), // y2
        (RpnOp::UiLine, None),
    ]);
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
    let bc = encode(&[
        // Set color to red
        (RpnOp::Push32, Some(&0xFF0000FFu32.to_le_bytes())),
        (RpnOp::UiSetColor, None),
        // Push state
        (RpnOp::UiPushState, None),
        // Change color to blue
        (RpnOp::Push32, Some(&0x0000FFFFu32.to_le_bytes())),
        (RpnOp::UiSetColor, None),
        // Pop state — should restore red
        (RpnOp::UiPopState, None),
    ]);
    let vm = run(&bc);
    assert_eq!(vm.ui_state.color, 0xFF0000FF);
}

#[test]
fn test_ui_state_stack_overflow() {
    // Push state 17 times (limit is 16)
    let mut instructions: Vec<(RpnOp, Option<&[u8]>)> = Vec::new();
    for _ in 0..=UI_STATE_STACK_MAX {
        instructions.push((RpnOp::UiPushState, None));
    }
    let bc = encode(&instructions);
    let result = run_result(&bc);
    assert_eq!(result.unwrap_err(), RpnError::UiStateStackOverflow);
}

#[test]
fn test_ui_state_stack_underflow() {
    let bc = encode(&[(RpnOp::UiPopState, None)]);
    let result = run_result(&bc);
    assert_eq!(result.unwrap_err(), RpnError::UiStateStackUnderflow);
}

#[test]
fn test_ui_set_offset() {
    let bc = encode(&[
        (RpnOp::Push32, Some(&0xFFFFFFFFu32.to_le_bytes())),
        (RpnOp::UiSetColor, None),
        // Set offset (10, 20)
        (RpnOp::Push32, Some(&10u32.to_le_bytes())),
        (RpnOp::Push32, Some(&20u32.to_le_bytes())),
        (RpnOp::UiSetOffset, None),
        // Draw box at (5, 5) → should become (15, 25)
        (RpnOp::Push32, Some(&5u32.to_le_bytes())),
        (RpnOp::Push32, Some(&5u32.to_le_bytes())),
        (RpnOp::Push32, Some(&50u32.to_le_bytes())),
        (RpnOp::Push32, Some(&50u32.to_le_bytes())),
        (RpnOp::UiBox, None),
    ]);
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

    let bc = encode(&[
        (RpnOp::Push32, Some(&0xFFFFFFFFu32.to_le_bytes())),
        (RpnOp::UiSetColor, None),
        (RpnOp::Push32, Some(&10u32.to_le_bytes())),  // x
        (RpnOp::Push32, Some(&20u32.to_le_bytes())),  // y
        (RpnOp::Push32, Some(&16u32.to_le_bytes())),  // size
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),   // str_idx = 1 ("World")
        (RpnOp::UiTextStr, None),
    ]);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.ui_draws.len(), 1);
    match &vm.ui_draws[0] {
        UiDrawCmd::TextStr { str_idx, .. } => assert_eq!(*str_idx, 1),
        _ => panic!("Expected TextStr"),
    }
}

// ── Color helpers ──

#[test]
fn test_rgba_pack_unpack() {
    let packed = rgba(255, 128, 0, 200);
    let (r, g, b, a) = rgba_unpack(packed);
    assert_eq!((r, g, b, a), (255, 128, 0, 200));
}

#[test]
fn test_blend_pixel_opaque() {
    let result = blend_pixel(0x00FF00, rgba(255, 0, 0, 255));
    assert_eq!(result, 0xFF0000); // fully opaque red
}

#[test]
fn test_blend_pixel_transparent() {
    let result = blend_pixel(0x00FF00, rgba(255, 0, 0, 0));
    assert_eq!(result, 0x00FF00); // fully transparent, background unchanged
}

// ── Softbuffer rasterizer ──

#[test]
fn test_render_ui_draws_box() {
    let draws = vec![UiDrawCmd::Box {
        x: 0,
        y: 0,
        w: 10,
        h: 10,
        color: rgba(255, 0, 0, 255),
    }];
    let mut buf = vec![0u32; 100];
    ui_vm::render_ui_draws(&draws, &mut buf, 10, 10);
    // Every pixel should be red (0x00RRGGBB format)
    for p in &buf {
        assert_eq!(*p, 0xFF0000);
    }
}

#[test]
fn test_render_ui_draws_circle() {
    let draws = vec![UiDrawCmd::Circle {
        x: 5,
        y: 5,
        r: 3,
        color: rgba(0, 255, 0, 255),
    }];
    let mut buf = vec![0u32; 100];
    ui_vm::render_ui_draws(&draws, &mut buf, 10, 10);
    // Center pixel should be green
    assert_eq!(buf[5 * 10 + 5], 0x00FF00);
    // Corner pixel should be untouched
    assert_eq!(buf[0], 0);
}

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
