//! Tests for UI draw opcodes in the RPN VM.

use matterstream::arena::TripleArena;
use matterstream::rpn::{MtuiOp, RpnError, RpnOp, RpnVm};
use matterstream::ui_vm::{
    UiDrawCmd, UI_DRAW_CMD_MAX, UI_STATE_STACK_MAX,
};
use matterstream_common::rgba;

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

// ============================================================
// UI opcode tests
// ============================================================

#[test]
fn ui_set_color() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let color = rgba(255, 0, 0, 255); // red
    let mut bc = encode_push32(color);
    bc.push(MtuiOp::SetColor.byte());

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.ui_state.color, color);
    assert!(vm.ui_draws.is_empty());
}

#[test]
fn ui_box_emits_draw_cmd() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let color = rgba(0, 255, 0, 255);
    let mut bc = encode_push32(color);
    bc.push(MtuiOp::SetColor.byte());
    // Push x=10, y=20, w=100, h=50
    bc.extend_from_slice(&encode_push32(10));
    bc.extend_from_slice(&encode_push32(20));
    bc.extend_from_slice(&encode_push32(100));
    bc.extend_from_slice(&encode_push32(50));
    bc.push(MtuiOp::Box.byte());

    vm.execute(&bc, &mut arenas).unwrap();
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
fn ui_slab_emits_draw_cmd() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let color = rgba(0, 0, 255, 200);
    let mut bc = encode_push32(color);
    bc.push(MtuiOp::SetColor.byte());
    // Push x=5, y=5, w=80, h=40, radius=8
    bc.extend_from_slice(&encode_push32(5));
    bc.extend_from_slice(&encode_push32(5));
    bc.extend_from_slice(&encode_push32(80));
    bc.extend_from_slice(&encode_push32(40));
    bc.extend_from_slice(&encode_push32(8));
    bc.push(MtuiOp::Slab.byte());

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.ui_draws.len(), 1);
    assert_eq!(
        vm.ui_draws[0],
        UiDrawCmd::Slab {
            x: 5,
            y: 5,
            w: 80,
            h: 40,
            radius: 8,
            color,
        }
    );
}

#[test]
fn ui_circle_emits_draw_cmd() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let color = rgba(255, 255, 0, 128);
    let mut bc = encode_push32(color);
    bc.push(MtuiOp::SetColor.byte());
    // Push x=50, y=50, r=25
    bc.extend_from_slice(&encode_push32(50));
    bc.extend_from_slice(&encode_push32(50));
    bc.extend_from_slice(&encode_push32(25));
    bc.push(MtuiOp::Circle.byte());

    vm.execute(&bc, &mut arenas).unwrap();
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
fn ui_text_emits_draw_cmd() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let color = 0xFFFFFFFF;
    let mut bc = Vec::new();
    // Push x=10, y=100, size=16, slot=0
    bc.extend_from_slice(&encode_push32(10));
    bc.extend_from_slice(&encode_push32(100));
    bc.extend_from_slice(&encode_push32(16));
    bc.extend_from_slice(&encode_push32(0));
    bc.push(MtuiOp::Text.byte());

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.ui_draws.len(), 1);
    assert_eq!(
        vm.ui_draws[0],
        UiDrawCmd::Text {
            x: 10,
            y: 100,
            size: 16,
            slot: 0,
            color,
        }
    );
}

#[test]
fn ui_line_emits_draw_cmd() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let color = rgba(128, 128, 128, 255);
    let mut bc = encode_push32(color);
    bc.push(MtuiOp::SetColor.byte());
    // Push x1=0, y1=0, x2=100, y2=100
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(100));
    bc.extend_from_slice(&encode_push32(100));
    bc.push(MtuiOp::Line.byte());

    vm.execute(&bc, &mut arenas).unwrap();
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

// ============================================================
// Push/Pop state tests
// ============================================================

#[test]
fn ui_push_pop_state_preserves_color() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let red = rgba(255, 0, 0, 255);
    let blue = rgba(0, 0, 255, 255);

    let mut bc = Vec::new();
    // Set color to red
    bc.extend_from_slice(&encode_push32(red));
    bc.push(MtuiOp::SetColor.byte());
    // Push state
    bc.push(MtuiOp::PushState.byte());
    // Set color to blue
    bc.extend_from_slice(&encode_push32(blue));
    bc.push(MtuiOp::SetColor.byte());
    // Pop state — should restore red
    bc.push(MtuiOp::PopState.byte());

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.ui_state.color, red);
}

#[test]
fn ui_push_pop_state_preserves_offset() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = Vec::new();
    // Apply offset (10, 20)
    bc.extend_from_slice(&encode_push32(10));
    bc.extend_from_slice(&encode_push32(20));
    bc.push(MtuiOp::ApplyOffset.byte());
    // Push state (copies transform)
    bc.push(MtuiOp::PushState.byte());
    // Apply offset (50, 60) — accumulates on top of (10,20) => (60, 80)
    bc.extend_from_slice(&encode_push32(50));
    bc.extend_from_slice(&encode_push32(60));
    bc.push(MtuiOp::ApplyOffset.byte());
    // Pop state — should restore transform to (10, 20)
    bc.push(MtuiOp::PopState.byte());

    vm.execute(&bc, &mut arenas).unwrap();
    // Transform stack top should be back to (10, 20)
    let t = vm.transform_stack.last().unwrap();
    assert_eq!(t[12] as i32, 10);
    assert_eq!(t[13] as i32, 20);
}

#[test]
fn ui_offset_applied_to_draws() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = Vec::new();
    // Apply offset (100, 200)
    bc.extend_from_slice(&encode_push32(100));
    bc.extend_from_slice(&encode_push32(200));
    bc.push(MtuiOp::ApplyOffset.byte());
    // Draw box at (10, 20, 30, 40) — should be offset to (110, 220)
    bc.extend_from_slice(&encode_push32(10));
    bc.extend_from_slice(&encode_push32(20));
    bc.extend_from_slice(&encode_push32(30));
    bc.extend_from_slice(&encode_push32(40));
    bc.push(MtuiOp::Box.byte());

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(
        vm.ui_draws[0],
        UiDrawCmd::Box {
            x: 110,
            y: 220,
            w: 30,
            h: 40,
            color: 0xFFFFFFFF,
        }
    );
}

// ============================================================
// Error condition tests
// ============================================================

#[test]
fn ui_state_stack_overflow() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = Vec::new();
    for _ in 0..=UI_STATE_STACK_MAX {
        bc.push(MtuiOp::PushState.byte());
    }

    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::UiStateStackOverflow)));
}

#[test]
fn ui_state_stack_underflow() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let bc = vec![MtuiOp::PopState.byte()];
    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::UiStateStackUnderflow)));
}

#[test]
fn ui_draw_limit_exceeded() {
    let mut vm = RpnVm::new();
    vm.max_cycles = UI_DRAW_CMD_MAX + 100_000;
    vm.gas = matterstream::rpn::GasConfig::new(100_000_000);
    let mut arenas = TripleArena::new();

    let mut bc = Vec::new();
    // Emit UI_DRAW_CMD_MAX + 1 boxes to trigger the limit
    for _ in 0..=UI_DRAW_CMD_MAX {
        bc.extend_from_slice(&encode_push32(0));
        bc.extend_from_slice(&encode_push32(0));
        bc.extend_from_slice(&encode_push32(1));
        bc.extend_from_slice(&encode_push32(1));
        bc.push(MtuiOp::Box.byte());
    }

    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::UiDrawLimitExceeded)));
}

// ============================================================
// Gas metering for UI ops
// ============================================================

#[test]
fn ui_ops_consume_gas() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let color = rgba(255, 0, 0, 255);
    let mut bc = encode_push32(color);
    bc.push(MtuiOp::SetColor.byte());
    // Box: 4 pushes + MtuiOp::Box
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(10));
    bc.extend_from_slice(&encode_push32(10));
    bc.push(MtuiOp::Box.byte());

    let trace = vm.execute_metered(&bc, &mut arenas).unwrap();
    // 5 Push32 (cost_push=1 each) + SetColor (cost_ui=5) + Box (cost_ui=5) = 15
    assert_eq!(trace.gas_consumed, 15);
}

// ============================================================
// Mixed arithmetic + UI interop
// ============================================================

#[test]
fn arithmetic_value_used_as_coordinate() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = Vec::new();
    // Compute 10 + 20 = 30, use as x
    bc.extend_from_slice(&encode_push64(10));
    bc.extend_from_slice(&encode_push64(20));
    bc.push(RpnOp::Add as u8);
    // y=5, w=50, h=50
    bc.extend_from_slice(&encode_push32(5));
    bc.extend_from_slice(&encode_push32(50));
    bc.extend_from_slice(&encode_push32(50));
    bc.push(MtuiOp::Box.byte());

    vm.execute(&bc, &mut arenas).unwrap();
    // x should be 30 (truncated from u64 to u32 then cast to i32)
    assert_eq!(
        vm.ui_draws[0],
        UiDrawCmd::Box {
            x: 30,
            y: 5,
            w: 50,
            h: 50,
            color: 0xFFFFFFFF,
        }
    );
}

// ============================================================
// Encode/decode and disassembly for UI opcodes
// ============================================================

#[test]
fn encode_decode_ui_opcodes_roundtrip() {
    // OR page opcodes are raw bytes, not RpnOp variants.
    // The decoder stores them as (Nop, vec![byte]) entries.
    let mut bytecode = Vec::new();
    bytecode.push(MtuiOp::SetColor.byte());
    bytecode.push(MtuiOp::Box.byte());
    bytecode.push(MtuiOp::Slab.byte());
    bytecode.push(MtuiOp::Circle.byte());
    bytecode.push(MtuiOp::Text.byte());
    bytecode.push(MtuiOp::PushState.byte());
    bytecode.push(MtuiOp::PopState.byte());
    bytecode.push(MtuiOp::ApplyOffset.byte());
    bytecode.push(MtuiOp::Line.byte());

    let decoded = RpnVm::decode(&bytecode).unwrap();

    assert_eq!(decoded.len(), 9);
    // OR page ops decode as (Nop, vec![byte])
    for (i, (op, payload)) in decoded.iter().enumerate() {
        assert_eq!(*op, RpnOp::Nop, "entry {} should decode as Nop placeholder", i);
        assert_eq!(payload.len(), 1, "entry {} should have 1-byte payload", i);
    }
}

#[test]
fn disassemble_ui_opcodes() {
    let mut bytecode = Vec::new();
    bytecode.push(MtuiOp::SetColor.byte());
    bytecode.push(MtuiOp::Box.byte());
    bytecode.push(MtuiOp::Circle.byte());
    bytecode.push(MtuiOp::PushState.byte());
    bytecode.push(MtuiOp::PopState.byte());
    bytecode.push(MtuiOp::Line.byte());

    let disasm = RpnVm::disassemble(&bytecode).unwrap();

    // Disassembly should show something for OR page bytes (0x80+)
    assert!(!disasm.is_empty());
}

// Pixel-level render tests moved to matterstream-ui-soft
// Color math tests moved to matterstream-common
