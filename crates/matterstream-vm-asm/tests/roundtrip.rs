//! Roundtrip tests: assemble → execute → verify results.

use matterstream_vm_asm::Asm;
use matterstream_vm::hooks::HookContext;
use matterstream_vm::rpn::RpnVm;
use matterstream_vm::ui_vm::UiDrawCmd;
use matterstream_vm_arena::TripleArena;

#[test]
fn test_assemble_and_execute_simple_arithmetic() {
    let mut asm = Asm::new();
    asm.push64(10);
    asm.push64(20);
    asm.add();
    asm.push64(5);
    asm.mul();
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    // (10 + 20) * 5 = 150
    assert_eq!(vm.stack[0].as_u64(), Some(150));
}

#[test]
fn test_assemble_and_execute_bank_load_store() {
    let mut hooks = HookContext::new();
    let score = hooks.use_state_i32(0);

    let mut asm = Asm::new();
    // Store 42 to score slot
    asm.push64(42);
    asm.store(score);
    // Load it back
    asm.load(score);
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    // VM int bank should have 42
    assert_eq!(vm.int_bank[score.index as usize], 42);
    // Stack should have the loaded value
    assert_eq!(vm.stack[0].as_u64(), Some(42));
}

#[test]
fn test_assemble_ui_scene() {
    let mut asm = Asm::new();
    let title = asm.def_string("Score: 0");

    // Background
    asm.set_color(30, 30, 50, 255);
    asm.draw_box(0, 0, 800, 600);

    // Title text
    asm.set_color(255, 255, 255, 255);
    asm.draw_text_str(10, 10, 24, title);

    // Game board — blue rectangle
    asm.set_color(0, 0, 200, 255);
    asm.draw_slab(50, 80, 700, 500, 12);

    // A piece — red circle
    asm.set_color(255, 50, 50, 255);
    asm.draw_circle(200, 200, 30);

    asm.halt();

    let output = asm.finish().unwrap();

    let mut vm = RpnVm::new();
    vm.string_table = output.string_table.clone();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    // Should have 4 draw commands
    assert_eq!(vm.ui_draws.len(), 4);

    // Background box
    match &vm.ui_draws[0] {
        UiDrawCmd::Box { w, h, .. } => {
            assert_eq!(*w, 800);
            assert_eq!(*h, 600);
        }
        _ => panic!("Expected Box"),
    }

    // Title text
    match &vm.ui_draws[1] {
        UiDrawCmd::TextStr { str_idx, size, .. } => {
            assert_eq!(*str_idx, 0);
            assert_eq!(*size, 24);
            assert_eq!(output.string_table[*str_idx as usize], "Score: 0");
        }
        _ => panic!("Expected TextStr"),
    }

    // Board slab
    match &vm.ui_draws[2] {
        UiDrawCmd::Slab { radius, .. } => assert_eq!(*radius, 12),
        _ => panic!("Expected Slab"),
    }

    // Piece circle
    match &vm.ui_draws[3] {
        UiDrawCmd::Circle { x, y, r, .. } => {
            assert_eq!((*x, *y, *r), (200, 200, 30));
        }
        _ => panic!("Expected Circle"),
    }
}

#[test]
fn test_assemble_loop_counting() {
    // Count from 0 to 5 using a loop
    let mut asm = Asm::new();
    let loop_top = asm.def_label();
    let loop_end = asm.def_label();

    asm.push64(0); // counter

    asm.mark(loop_top);
    asm.dup();
    asm.push64(5);
    asm.cmp_ge();
    asm.jmp_if(loop_end);

    asm.push64(1);
    asm.add();
    asm.jmp(loop_top);

    asm.mark(loop_end);
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    assert_eq!(vm.stack[0].as_u64(), Some(5));
}

#[test]
fn test_assemble_conditional_draw() {
    // If a condition is true, draw a box; otherwise draw a circle
    let mut asm = Asm::new();
    let else_branch = asm.def_label();
    let end = asm.def_label();

    asm.set_color(255, 255, 255, 255);

    // Condition: 1 (true)
    asm.push64(0); // false → draw circle branch
    asm.jmp_if(else_branch);

    // Else: draw circle
    asm.draw_circle(50, 50, 20);
    asm.jmp(end);

    // Then: draw box
    asm.mark(else_branch);
    asm.draw_box(0, 0, 100, 100);

    asm.mark(end);
    asm.halt();

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    // Condition was 0 (false), so circle was drawn
    assert_eq!(vm.ui_draws.len(), 1);
    assert!(matches!(vm.ui_draws[0], UiDrawCmd::Circle { .. }));
}

#[test]
fn test_assemble_event_poll_and_respond() {
    use matterstream_vm::event::VmEvent;

    let mut asm = Asm::new();
    // Poll event → pushes (data, type)
    asm.ev_poll();
    // type is on top, drop it
    asm.drop_();
    // data (key code) is now on top
    asm.halt();

    let output = asm.finish().unwrap();

    let mut vm = RpnVm::new();
    vm.event_queue.push_back(VmEvent::key_down(42));
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    // Key code 42 should be on stack
    assert_eq!(vm.stack[0].as_u64(), Some(42));
}
