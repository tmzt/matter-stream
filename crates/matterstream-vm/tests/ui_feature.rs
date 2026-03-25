//! Tests for UI feature gating based on memory-core-demo use cases.
//!
//! These tests mirror the real-world patterns from the demo:
//! - Compile TSX → execute → extract draws, cards, skills
//! - Access VM UI state after execution
//! - Card bounds computation from draw commands

#[cfg(feature = "ui")]
mod with_ui {
    use matterstream_vm::rpn::{RpnOp, RpnVm};
    use matterstream_vm::ui_vm::{UiDrawCmd, CardDef};
    use matterstream_vm_arena::TripleArena;

    /// Pattern from memory-core-demo: hand-assemble bytecode that draws a slab,
    /// execute, verify draw commands appear in vm.ui_draws.
    #[test]
    fn execute_ui_bytecode_produces_draws() {
        let mut vm = RpnVm::new();
        let mut arenas = TripleArena::new();

        // SetColor(blue) + Slab(0, 0, 200, 100, 12) + Halt
        let blue = matterstream_common::rgba(50, 100, 255, 255);
        let mut bc = Vec::new();
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&blue.to_le_bytes());
        bc.push(RpnOp::UiSetColor as u8);
        for val in [0u32, 0, 200, 100, 12] {
            bc.push(RpnOp::Push32 as u8);
            bc.extend_from_slice(&val.to_le_bytes());
        }
        bc.push(RpnOp::UiSlab as u8);
        bc.push(RpnOp::Halt as u8);

        vm.execute(&bc, &mut arenas).unwrap();

        assert_eq!(vm.ui_draws.len(), 1);
        match &vm.ui_draws[0] {
            UiDrawCmd::Slab { x, y, w, h, radius, color } => {
                assert_eq!(*x, 0);
                assert_eq!(*y, 0);
                assert_eq!(*w, 200);
                assert_eq!(*h, 100);
                assert_eq!(*radius, 12);
                assert_eq!(*color, blue);
            }
            other => panic!("expected Slab, got {:?}", other),
        }
    }

    /// Pattern from session.rs: execute produces ui_draws, compute bounding box.
    #[test]
    fn card_bounds_from_draws() {
        let draws = vec![
            UiDrawCmd::Slab { x: 10, y: 20, w: 300, h: 80, radius: 8, color: 0xFF },
            UiDrawCmd::Text { x: 16, y: 16, size: 20, slot: 0, color: 0xFF },
        ];

        let (max_x, max_y) = compute_bounds(&draws);
        assert_eq!(max_x, 310); // 10 + 300
        assert_eq!(max_y, 100); // 20 + 80
    }

    /// Pattern from tsx_detect.rs: compile TSX, execute, check card_outputs.
    #[test]
    fn card_output_captures_draws() {
        let mut vm = RpnVm::new();
        let mut arenas = TripleArena::new();

        // Simulate CardBegin + draw + CardEnd via raw bytecode
        // CardBegin pushes a new card, draws go to card_active, CardEnd finalizes
        let mut bc = Vec::new();

        // Push card name string index
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&0u32.to_le_bytes());
        bc.push(RpnOp::CardBegin as u8);

        // Draw a slab inside the card
        let color = matterstream_common::rgba(0x33, 0x66, 0x99, 0xFF);
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&color.to_le_bytes());
        bc.push(RpnOp::UiSetColor as u8);
        for val in [0u32, 0, 200, 100, 8] {
            bc.push(RpnOp::Push32 as u8);
            bc.extend_from_slice(&val.to_le_bytes());
        }
        bc.push(RpnOp::UiSlab as u8);

        bc.push(RpnOp::CardEnd as u8);
        bc.push(RpnOp::Halt as u8);

        vm.string_table = vec!["test_card".to_string()];
        vm.execute(&bc, &mut arenas).unwrap();

        // Card captures draws, not ui_draws
        assert!(vm.ui_draws.is_empty(), "draws should go to card, not ui_draws");
        assert_eq!(vm.card_outputs.len(), 1);
        assert_eq!(vm.card_outputs[0].name, "test_card");
        assert_eq!(vm.card_outputs[0].draws.len(), 1);
    }

    /// Pattern from tsx_detect.rs: skill output extraction after execution.
    #[test]
    fn skill_output_extraction() {
        let mut vm = RpnVm::new();
        let mut arenas = TripleArena::new();

        // SkillBegin (name_idx=0) + Step(name_idx=1, action_idx=2) + SkillEnd
        let mut bc = Vec::new();

        // SetCR to SKLL mode
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&0u32.to_le_bytes()); // CR index 0
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&matterstream_vm::ui_vm::FOURCC_SKLL.to_le_bytes());
        bc.push(RpnOp::SetCR as u8);

        // SkillBegin with name string index 0
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&0u32.to_le_bytes());
        bc.push(RpnOp::SkillBegin as u8);

        // SkillStep with name string index 1
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&1u32.to_le_bytes());
        bc.push(RpnOp::SkillStep as u8);

        bc.push(RpnOp::SkillEnd as u8);
        bc.push(RpnOp::Halt as u8);

        vm.string_table = vec![
            "my_skill".to_string(),
            "step_one".to_string(),
        ];
        vm.execute(&bc, &mut arenas).unwrap();

        assert_eq!(vm.skill_outputs.len(), 1);
        assert_eq!(vm.skill_outputs[0].name, "my_skill");
        assert_eq!(vm.skill_outputs[0].steps.len(), 1);
    }

    /// Pattern from session.rs: multiple draw types in sequence.
    #[test]
    fn multiple_draw_types() {
        let mut vm = RpnVm::new();
        let mut arenas = TripleArena::new();
        let mut bc = Vec::new();

        let white = matterstream_common::rgba(255, 255, 255, 255);
        let red = matterstream_common::rgba(255, 0, 0, 255);

        // Draw a box
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&white.to_le_bytes());
        bc.push(RpnOp::UiSetColor as u8);
        for val in [0u32, 0, 400, 300] {
            bc.push(RpnOp::Push32 as u8);
            bc.extend_from_slice(&val.to_le_bytes());
        }
        bc.push(RpnOp::UiBox as u8);

        // Draw a circle
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&red.to_le_bytes());
        bc.push(RpnOp::UiSetColor as u8);
        for val in [200u32, 150, 50] {
            bc.push(RpnOp::Push32 as u8);
            bc.extend_from_slice(&val.to_le_bytes());
        }
        bc.push(RpnOp::UiCircle as u8);

        // Draw a line
        for val in [10u32, 10, 390, 290] {
            bc.push(RpnOp::Push32 as u8);
            bc.extend_from_slice(&val.to_le_bytes());
        }
        bc.push(RpnOp::UiLine as u8);

        bc.push(RpnOp::Halt as u8);

        vm.execute(&bc, &mut arenas).unwrap();

        assert_eq!(vm.ui_draws.len(), 3);
        assert!(matches!(vm.ui_draws[0], UiDrawCmd::Box { .. }));
        assert!(matches!(vm.ui_draws[1], UiDrawCmd::Circle { .. }));
        assert!(matches!(vm.ui_draws[2], UiDrawCmd::Line { .. }));
    }

    /// Pattern from session.rs: transform stack affects draw positions.
    #[test]
    fn transform_stack_offsets_draws() {
        let mut vm = RpnVm::new();
        let mut arenas = TripleArena::new();
        let mut bc = Vec::new();

        let color = matterstream_common::rgba(255, 255, 255, 255);
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&color.to_le_bytes());
        bc.push(RpnOp::UiSetColor as u8);

        // PushState + ApplyOffset(100, 50)
        bc.push(RpnOp::UiPushState as u8);
        for val in [100u32, 50] {
            bc.push(RpnOp::Push32 as u8);
            bc.extend_from_slice(&val.to_le_bytes());
        }
        bc.push(RpnOp::UiApplyOffset as u8);

        // Draw a box at (0,0) — should appear at (100, 50) due to transform
        for val in [0u32, 0, 50, 50] {
            bc.push(RpnOp::Push32 as u8);
            bc.extend_from_slice(&val.to_le_bytes());
        }
        bc.push(RpnOp::UiBox as u8);

        bc.push(RpnOp::UiPopState as u8);

        // Draw another box at (0,0) — should appear at (0, 0) after pop
        for val in [0u32, 0, 50, 50] {
            bc.push(RpnOp::Push32 as u8);
            bc.extend_from_slice(&val.to_le_bytes());
        }
        bc.push(RpnOp::UiBox as u8);

        bc.push(RpnOp::Halt as u8);

        vm.execute(&bc, &mut arenas).unwrap();

        assert_eq!(vm.ui_draws.len(), 2);
        match &vm.ui_draws[0] {
            UiDrawCmd::Box { x, y, .. } => {
                assert_eq!(*x, 100);
                assert_eq!(*y, 50);
            }
            _ => panic!("expected Box"),
        }
        match &vm.ui_draws[1] {
            UiDrawCmd::Box { x, y, .. } => {
                assert_eq!(*x, 0);
                assert_eq!(*y, 0);
            }
            _ => panic!("expected Box"),
        }
    }

    /// Pattern from session.rs: execute clears ui_draws between calls.
    #[test]
    fn execute_clears_ui_state() {
        let mut vm = RpnVm::new();
        let mut arenas = TripleArena::new();

        let color = matterstream_common::rgba(255, 0, 0, 255);
        let mut bc = Vec::new();
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&color.to_le_bytes());
        bc.push(RpnOp::UiSetColor as u8);
        for val in [0u32, 0, 100, 100] {
            bc.push(RpnOp::Push32 as u8);
            bc.extend_from_slice(&val.to_le_bytes());
        }
        bc.push(RpnOp::UiBox as u8);
        bc.push(RpnOp::Halt as u8);

        vm.execute(&bc, &mut arenas).unwrap();
        assert_eq!(vm.ui_draws.len(), 1);

        // Second execute should clear
        vm.execute(&bc, &mut arenas).unwrap();
        assert_eq!(vm.ui_draws.len(), 1); // fresh, not 2
    }

    fn compute_bounds(draws: &[UiDrawCmd]) -> (i32, i32) {
        let mut max_x = 0i32;
        let mut max_y = 0i32;
        for cmd in draws {
            match cmd {
                UiDrawCmd::Box { x, y, w, h, .. }
                | UiDrawCmd::Slab { x, y, w, h, .. }
                | UiDrawCmd::Action { x, y, w, h, .. } => {
                    max_x = max_x.max(x + *w as i32);
                    max_y = max_y.max(y + *h as i32);
                }
                UiDrawCmd::Circle { x, y, r, .. } => {
                    max_x = max_x.max(x + *r as i32);
                    max_y = max_y.max(y + *r as i32);
                }
                UiDrawCmd::Text { x, y, size, .. }
                | UiDrawCmd::TextStr { x, y, size, .. } => {
                    max_x = max_x.max(x + (*size as i32) * 6);
                    max_y = max_y.max(y + *size as i32);
                }
                UiDrawCmd::Line { x1, y1, x2, y2, .. } => {
                    max_x = max_x.max(*x1).max(*x2);
                    max_y = max_y.max(*y1).max(*y2);
                }
            }
        }
        (max_x, max_y)
    }
}
