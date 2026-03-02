//! Tests for float arithmetic, ZeroPage i32, and component-aware assembler helpers.

use matterstream_vm::hooks::{BankKind, StateSlot};
use matterstream_vm::rpn::RpnVm;
use matterstream_vm_arena::TripleArena;
use matterstream_vm_asm::Asm;

fn run_asm(asm: Asm) -> RpnVm {
    let output = asm.finish().expect("assembly failed");
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.string_table = output.string_table;
    vm.execute(&output.bytecode, &mut arenas).expect("execution failed");
    vm
}

fn pop_f32(vm: &mut RpnVm) -> f32 {
    let val = vm.stack.pop().unwrap();
    let bits = val.as_u64().unwrap() as u32;
    f32::from_bits(bits)
}

fn pop_u64(vm: &mut RpnVm) -> u64 {
    vm.stack.pop().unwrap().as_u64().unwrap()
}

// ── push_f32 ──

#[test]
fn test_push_f32() {
    let mut asm = Asm::new();
    asm.push_f32(3.14);
    let mut vm = run_asm(asm);
    let result = pop_f32(&mut vm);
    assert!((result - 3.14).abs() < 1e-5, "Expected ~3.14, got {}", result);
}

// ── Float arithmetic helpers ──

#[test]
fn test_fadd_helper() {
    let mut asm = Asm::new();
    asm.push_f32(1.5);
    asm.push_f32(2.5);
    asm.fadd();
    let mut vm = run_asm(asm);
    let result = pop_f32(&mut vm);
    assert!((result - 4.0).abs() < 1e-6);
}

#[test]
fn test_fsub_helper() {
    let mut asm = Asm::new();
    asm.push_f32(10.0);
    asm.push_f32(3.0);
    asm.fsub();
    let mut vm = run_asm(asm);
    let result = pop_f32(&mut vm);
    assert!((result - 7.0).abs() < 1e-6);
}

#[test]
fn test_fmul_helper() {
    let mut asm = Asm::new();
    asm.push_f32(6.0);
    asm.push_f32(7.0);
    asm.fmul();
    let mut vm = run_asm(asm);
    let result = pop_f32(&mut vm);
    assert!((result - 42.0).abs() < 1e-6);
}

#[test]
fn test_fdiv_helper() {
    let mut asm = Asm::new();
    asm.push_f32(22.0);
    asm.push_f32(7.0);
    asm.fdiv();
    let mut vm = run_asm(asm);
    let result = pop_f32(&mut vm);
    assert!((result - 22.0 / 7.0).abs() < 1e-5);
}

#[test]
fn test_fneg_helper() {
    let mut asm = Asm::new();
    asm.push_f32(5.0);
    asm.fneg();
    let mut vm = run_asm(asm);
    let result = pop_f32(&mut vm);
    assert!((result + 5.0).abs() < 1e-6);
}

#[test]
fn test_fabs_helper() {
    let mut asm = Asm::new();
    asm.push_f32(-123.0);
    asm.fabs();
    let mut vm = run_asm(asm);
    let result = pop_f32(&mut vm);
    assert!((result - 123.0).abs() < 1e-6);
}

#[test]
fn test_fcmp_gt_helper() {
    let mut asm = Asm::new();
    asm.push_f32(10.0);
    asm.push_f32(5.0);
    asm.fcmp_gt();
    let mut vm = run_asm(asm);
    assert_eq!(pop_u64(&mut vm), 1);
}

#[test]
fn test_fcmp_lt_helper() {
    let mut asm = Asm::new();
    asm.push_f32(2.0);
    asm.push_f32(8.0);
    asm.fcmp_lt();
    let mut vm = run_asm(asm);
    assert_eq!(pop_u64(&mut vm), 1);
}

#[test]
fn test_fcmp_eq_helper() {
    let mut asm = Asm::new();
    asm.push_f32(7.0);
    asm.push_f32(7.0);
    asm.fcmp_eq();
    let mut vm = run_asm(asm);
    assert_eq!(pop_u64(&mut vm), 1);
}

#[test]
fn test_i2f_helper() {
    let mut asm = Asm::new();
    asm.push32(100);
    asm.i2f();
    let mut vm = run_asm(asm);
    let result = pop_f32(&mut vm);
    assert!((result - 100.0).abs() < 1e-6);
}

#[test]
fn test_f2i_helper() {
    let mut asm = Asm::new();
    asm.push_f32(99.9);
    asm.f2i();
    let mut vm = run_asm(asm);
    let val = pop_u64(&mut vm) as u32 as i32;
    assert_eq!(val, 99);
}

// ── ZeroPage i32 helpers ──

#[test]
fn test_load_store_zp_i32_helper() {
    let mut asm = Asm::new();
    // Store 42 at ZP[12]
    asm.push32(42);
    asm.store_zp_i32(12);
    // Load it back
    asm.load_zp_i32(12);
    let mut vm = run_asm(asm);
    let val = pop_u64(&mut vm) as u32 as i32;
    assert_eq!(val, 42);
}

#[test]
fn test_zp_i32_multiple_slots() {
    let mut asm = Asm::new();
    // Store 100 at ZP[0], 200 at ZP[4], 300 at ZP[8]
    asm.push32(100);
    asm.store_zp_i32(0);
    asm.push32(200);
    asm.store_zp_i32(4);
    asm.push32(300);
    asm.store_zp_i32(8);
    // Load all three back
    asm.load_zp_i32(0);
    asm.load_zp_i32(4);
    asm.load_zp_i32(8);
    let mut vm = run_asm(asm);

    assert_eq!(pop_u64(&mut vm) as i32, 300);
    assert_eq!(pop_u64(&mut vm) as i32, 200);
    assert_eq!(pop_u64(&mut vm) as i32, 100);
}

// ── Component-aware bank helpers ──

#[test]
fn test_load_bank_comp_helper() {
    let bird = StateSlot {
        bank: BankKind::Vec3,
        index: 0,
        count: 1,
    };
    let mut asm = Asm::new();
    asm.load_bank_comp(bird, 1);
    let mut vm = RpnVm::new();
    vm.vec3_bank[0] = [10.0, 20.0, 30.0];
    let mut arenas = TripleArena::new();
    let output = asm.finish().unwrap();
    vm.execute(&output.bytecode, &mut arenas).unwrap();
    let bits = vm.stack.pop().unwrap().as_u64().unwrap() as u32;
    let val = f32::from_bits(bits);
    assert!((val - 20.0).abs() < 1e-6);
}

#[test]
fn test_store_bank_comp_helper() {
    let data = StateSlot {
        bank: BankKind::Vec4,
        index: 3,
        count: 1,
    };
    let mut asm = Asm::new();
    asm.push_f32(42.0);
    asm.store_bank_comp(data, 2);
    let mut vm = RpnVm::new();
    vm.vec4_bank[3] = [0.0, 0.0, 0.0, 0.0];
    let mut arenas = TripleArena::new();
    let output = asm.finish().unwrap();
    vm.execute(&output.bytecode, &mut arenas).unwrap();
    assert!((vm.vec4_bank[3][2] - 42.0).abs() < 1e-6);
}

// ── Physics pipeline via assembler ──

#[test]
fn test_assembler_physics_pipeline() {
    let bird = StateSlot {
        bank: BankKind::Vec3,
        index: 0,
        count: 1,
    };

    let mut asm = Asm::new();

    // velocity += gravity (0.5)
    asm.load_bank_comp(bird, 2);  // vel
    asm.push_f32(0.5);
    asm.fadd();
    asm.store_bank_comp(bird, 2);

    // y += velocity
    asm.load_bank_comp(bird, 1);  // y
    asm.load_bank_comp(bird, 2);  // vel
    asm.fadd();
    asm.store_bank_comp(bird, 1);

    let output = asm.finish().unwrap();
    let mut vm = RpnVm::new();
    vm.vec3_bank[0] = [80.0, 100.0, 0.0]; // [x, y, vel]
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).unwrap();

    assert!((vm.vec3_bank[0][2] - 0.5).abs() < 1e-6, "vel should be 0.5");
    assert!((vm.vec3_bank[0][1] - 100.5).abs() < 1e-6, "y should be 100.5");
}
