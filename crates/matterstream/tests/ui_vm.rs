//! Tests for UI draw opcodes in the RPN VM.

use matterstream::arena::TripleArena;
use matterstream::rpn::{RpnError, RpnOp, RpnVm};
use matterstream::ui_vm::{
    blend_pixel, render_ui_draws, rgba, rgba_unpack, UiDrawCmd, UI_DRAW_CMD_MAX,
    UI_STATE_STACK_MAX,
};

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
    bc.push(RpnOp::UiSetColor as u8);

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
    bc.push(RpnOp::UiSetColor as u8);
    // Push x=10, y=20, w=100, h=50
    bc.extend_from_slice(&encode_push32(10));
    bc.extend_from_slice(&encode_push32(20));
    bc.extend_from_slice(&encode_push32(100));
    bc.extend_from_slice(&encode_push32(50));
    bc.push(RpnOp::UiBox as u8);

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
    bc.push(RpnOp::UiSetColor as u8);
    // Push x=5, y=5, w=80, h=40, radius=8
    bc.extend_from_slice(&encode_push32(5));
    bc.extend_from_slice(&encode_push32(5));
    bc.extend_from_slice(&encode_push32(80));
    bc.extend_from_slice(&encode_push32(40));
    bc.extend_from_slice(&encode_push32(8));
    bc.push(RpnOp::UiSlab as u8);

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
    bc.push(RpnOp::UiSetColor as u8);
    // Push x=50, y=50, r=25
    bc.extend_from_slice(&encode_push32(50));
    bc.extend_from_slice(&encode_push32(50));
    bc.extend_from_slice(&encode_push32(25));
    bc.push(RpnOp::UiCircle as u8);

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
    bc.push(RpnOp::UiText as u8);

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
    bc.push(RpnOp::UiSetColor as u8);
    // Push x1=0, y1=0, x2=100, y2=100
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(100));
    bc.extend_from_slice(&encode_push32(100));
    bc.push(RpnOp::UiLine as u8);

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
    bc.push(RpnOp::UiSetColor as u8);
    // Push state
    bc.push(RpnOp::UiPushState as u8);
    // Set color to blue
    bc.extend_from_slice(&encode_push32(blue));
    bc.push(RpnOp::UiSetColor as u8);
    // Pop state — should restore red
    bc.push(RpnOp::UiPopState as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.ui_state.color, red);
}

#[test]
fn ui_push_pop_state_preserves_offset() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = Vec::new();
    // Set offset to (10, 20)
    bc.extend_from_slice(&encode_push32(10));
    bc.extend_from_slice(&encode_push32(20));
    bc.push(RpnOp::UiSetOffset as u8);
    // Push state
    bc.push(RpnOp::UiPushState as u8);
    // Set offset to (50, 60)
    bc.extend_from_slice(&encode_push32(50));
    bc.extend_from_slice(&encode_push32(60));
    bc.push(RpnOp::UiSetOffset as u8);
    // Pop state — should restore (10, 20)
    bc.push(RpnOp::UiPopState as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.ui_state.offset_x, 10);
    assert_eq!(vm.ui_state.offset_y, 20);
}

#[test]
fn ui_offset_applied_to_draws() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = Vec::new();
    // Set offset (100, 200)
    bc.extend_from_slice(&encode_push32(100));
    bc.extend_from_slice(&encode_push32(200));
    bc.push(RpnOp::UiSetOffset as u8);
    // Draw box at (10, 20, 30, 40) — should be offset to (110, 220)
    bc.extend_from_slice(&encode_push32(10));
    bc.extend_from_slice(&encode_push32(20));
    bc.extend_from_slice(&encode_push32(30));
    bc.extend_from_slice(&encode_push32(40));
    bc.push(RpnOp::UiBox as u8);

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
        bc.push(RpnOp::UiPushState as u8);
    }

    let result = vm.execute(&bc, &mut arenas);
    assert!(matches!(result, Err(RpnError::UiStateStackOverflow)));
}

#[test]
fn ui_state_stack_underflow() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let bc = vec![RpnOp::UiPopState as u8];
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
        bc.push(RpnOp::UiBox as u8);
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
    bc.push(RpnOp::UiSetColor as u8);
    // Box: 4 pushes + UiBox
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(0));
    bc.extend_from_slice(&encode_push32(10));
    bc.extend_from_slice(&encode_push32(10));
    bc.push(RpnOp::UiBox as u8);

    let trace = vm.execute_metered(&bc, &mut arenas).unwrap();
    // 5 Push32 (cost_push=1 each) + UiSetColor (cost_ui=5) + UiBox (cost_ui=5) = 15
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
    bc.push(RpnOp::UiBox as u8);

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
    let instructions: Vec<(RpnOp, Option<&[u8]>)> = vec![
        (RpnOp::UiSetColor, None),
        (RpnOp::UiBox, None),
        (RpnOp::UiSlab, None),
        (RpnOp::UiCircle, None),
        (RpnOp::UiText, None),
        (RpnOp::UiPushState, None),
        (RpnOp::UiPopState, None),
        (RpnOp::UiSetOffset, None),
        (RpnOp::UiLine, None),
    ];

    let bytecode = RpnVm::encode(&instructions);
    let decoded = RpnVm::decode(&bytecode).unwrap();

    assert_eq!(decoded.len(), 9);
    assert_eq!(decoded[0].0, RpnOp::UiSetColor);
    assert_eq!(decoded[1].0, RpnOp::UiBox);
    assert_eq!(decoded[2].0, RpnOp::UiSlab);
    assert_eq!(decoded[3].0, RpnOp::UiCircle);
    assert_eq!(decoded[4].0, RpnOp::UiText);
    assert_eq!(decoded[5].0, RpnOp::UiPushState);
    assert_eq!(decoded[6].0, RpnOp::UiPopState);
    assert_eq!(decoded[7].0, RpnOp::UiSetOffset);
    assert_eq!(decoded[8].0, RpnOp::UiLine);
}

#[test]
fn disassemble_ui_opcodes() {
    let instructions: Vec<(RpnOp, Option<&[u8]>)> = vec![
        (RpnOp::UiSetColor, None),
        (RpnOp::UiBox, None),
        (RpnOp::UiCircle, None),
        (RpnOp::UiPushState, None),
        (RpnOp::UiPopState, None),
        (RpnOp::UiLine, None),
    ];

    let bytecode = RpnVm::encode(&instructions);
    let disasm = RpnVm::disassemble(&bytecode).unwrap();

    assert!(disasm.contains("UiSetColor"));
    assert!(disasm.contains("UiBox"));
    assert!(disasm.contains("UiCircle"));
    assert!(disasm.contains("UiPushState"));
    assert!(disasm.contains("UiPopState"));
    assert!(disasm.contains("UiLine"));
}

// ============================================================
// Pixel-level render tests
// ============================================================

#[test]
fn render_box_pixels() {
    let draws = vec![UiDrawCmd::Box {
        x: 1,
        y: 1,
        w: 2,
        h: 2,
        color: rgba(255, 0, 0, 255),
    }];

    let (w, h) = (4, 4);
    let mut buf = vec![0u32; (w * h) as usize];
    render_ui_draws(&draws, &mut buf, w, h);

    // Red = 0x00FF0000 in softbuffer format
    let red_pixel = 0x00FF0000;
    assert_eq!(buf[(w + 1) as usize], red_pixel);
    assert_eq!(buf[(w + 2) as usize], red_pixel);
    assert_eq!(buf[(2 * w + 1) as usize], red_pixel);
    assert_eq!(buf[(2 * w + 2) as usize], red_pixel);
    // Outside the box should be 0
    assert_eq!(buf[0], 0);
    assert_eq!(buf[(3 * w + 3) as usize], 0);
}

#[test]
fn render_circle_pixels() {
    let draws = vec![UiDrawCmd::Circle {
        x: 5,
        y: 5,
        r: 2,
        color: rgba(0, 255, 0, 255),
    }];

    let (w, h) = (11, 11);
    let mut buf = vec![0u32; (w * h) as usize];
    render_ui_draws(&draws, &mut buf, w, h);

    // Center should be green
    let green_pixel = 0x0000FF00;
    assert_eq!(buf[(5 * w + 5) as usize], green_pixel);
    // Points at distance 2 on axis should be green (5+2=7, 5-2=3)
    assert_eq!(buf[(5 * w + 7) as usize], green_pixel);
    assert_eq!(buf[(5 * w + 3) as usize], green_pixel);
    // Point clearly outside (distance > 2)
    assert_eq!(buf[0], 0);
}

#[test]
fn alpha_blending() {
    // Semi-transparent red over white background
    let dst = 0x00FFFFFF; // white in softbuffer
    let src = rgba(255, 0, 0, 128); // 50% red
    let blended = blend_pixel(dst, src);

    let r = (blended >> 16) as u8;
    let g = (blended >> 8) as u8;
    let b = blended as u8;

    // Red should be high (close to 255), green and blue reduced
    assert!(r > 200);
    assert!(g < 130 && g > 100);
    assert!(b < 130 && b > 100);
}

#[test]
fn rgba_pack_unpack_roundtrip() {
    let (r, g, b, a) = (0x12, 0x34, 0x56, 0x78);
    let packed = rgba(r, g, b, a);
    assert_eq!(packed, 0x12345678);
    let (ur, ug, ub, ua) = rgba_unpack(packed);
    assert_eq!((ur, ug, ub, ua), (r, g, b, a));
}

#[test]
fn blend_pixel_fully_transparent() {
    let dst = 0x00AABBCC;
    let src = rgba(255, 0, 0, 0); // fully transparent
    assert_eq!(blend_pixel(dst, src), dst);
}

#[test]
fn blend_pixel_fully_opaque() {
    let dst = 0x00AABBCC;
    let src = rgba(0x11, 0x22, 0x33, 255); // fully opaque
    assert_eq!(blend_pixel(dst, src), 0x00112233);
}
