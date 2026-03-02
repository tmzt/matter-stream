//! Tests for float arithmetic, ZeroPage i32, and component-aware bank opcodes.

use matterstream_vm::rpn::{RpnOp, RpnVm, BANK_VEC3, BANK_VEC4};
use matterstream_vm_arena::TripleArena;

fn push_f32_bytes(val: f32) -> Vec<u8> {
    let bits = f32::to_bits(val);
    let mut v = vec![RpnOp::Push32 as u8];
    v.extend_from_slice(&bits.to_le_bytes());
    v
}

fn run(bytecode: &[u8]) -> RpnVm {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(bytecode, &mut arenas).expect("execution failed");
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

// ── Float Arithmetic ──

#[test]
fn test_fadd() {
    let mut bc = Vec::new();
    bc.extend(push_f32_bytes(3.5));
    bc.extend(push_f32_bytes(2.25));
    bc.push(RpnOp::FAdd as u8);
    let mut vm = run(&bc);
    let result = pop_f32(&mut vm);
    assert!((result - 5.75).abs() < 1e-6, "FAdd: expected 5.75, got {}", result);
}

#[test]
fn test_fsub() {
    let mut bc = Vec::new();
    bc.extend(push_f32_bytes(10.0));
    bc.extend(push_f32_bytes(3.5));
    bc.push(RpnOp::FSub as u8);
    let mut vm = run(&bc);
    let result = pop_f32(&mut vm);
    assert!((result - 6.5).abs() < 1e-6, "FSub: expected 6.5, got {}", result);
}

#[test]
fn test_fmul() {
    let mut bc = Vec::new();
    bc.extend(push_f32_bytes(4.0));
    bc.extend(push_f32_bytes(2.5));
    bc.push(RpnOp::FMul as u8);
    let mut vm = run(&bc);
    let result = pop_f32(&mut vm);
    assert!((result - 10.0).abs() < 1e-6, "FMul: expected 10.0, got {}", result);
}

#[test]
fn test_fdiv() {
    let mut bc = Vec::new();
    bc.extend(push_f32_bytes(15.0));
    bc.extend(push_f32_bytes(4.0));
    bc.push(RpnOp::FDiv as u8);
    let mut vm = run(&bc);
    let result = pop_f32(&mut vm);
    assert!((result - 3.75).abs() < 1e-6, "FDiv: expected 3.75, got {}", result);
}

#[test]
fn test_fdiv_by_zero() {
    let mut bc = Vec::new();
    bc.extend(push_f32_bytes(1.0));
    bc.extend(push_f32_bytes(0.0));
    bc.push(RpnOp::FDiv as u8);
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let err = vm.execute(&bc, &mut arenas).unwrap_err();
    assert_eq!(err, matterstream_vm::rpn::RpnError::DivisionByZero);
}

#[test]
fn test_fneg() {
    let mut bc = Vec::new();
    bc.extend(push_f32_bytes(7.5));
    bc.push(RpnOp::FNeg as u8);
    let mut vm = run(&bc);
    let result = pop_f32(&mut vm);
    assert!((result + 7.5).abs() < 1e-6, "FNeg: expected -7.5, got {}", result);
}

#[test]
fn test_fabs() {
    let mut bc = Vec::new();
    bc.extend(push_f32_bytes(-42.0));
    bc.push(RpnOp::FAbs as u8);
    let mut vm = run(&bc);
    let result = pop_f32(&mut vm);
    assert!((result - 42.0).abs() < 1e-6, "FAbs: expected 42.0, got {}", result);
}

// ── Float Comparisons ──

#[test]
fn test_fcmp_gt() {
    let mut bc = Vec::new();
    bc.extend(push_f32_bytes(5.0));
    bc.extend(push_f32_bytes(3.0));
    bc.push(RpnOp::FCmpGt as u8);
    let mut vm = run(&bc);
    assert_eq!(pop_u64(&mut vm), 1);

    let mut bc2 = Vec::new();
    bc2.extend(push_f32_bytes(1.0));
    bc2.extend(push_f32_bytes(3.0));
    bc2.push(RpnOp::FCmpGt as u8);
    let mut vm2 = run(&bc2);
    assert_eq!(pop_u64(&mut vm2), 0);
}

#[test]
fn test_fcmp_lt() {
    let mut bc = Vec::new();
    bc.extend(push_f32_bytes(1.0));
    bc.extend(push_f32_bytes(3.0));
    bc.push(RpnOp::FCmpLt as u8);
    let mut vm = run(&bc);
    assert_eq!(pop_u64(&mut vm), 1);
}

#[test]
fn test_fcmp_eq() {
    let mut bc = Vec::new();
    bc.extend(push_f32_bytes(2.5));
    bc.extend(push_f32_bytes(2.5));
    bc.push(RpnOp::FCmpEq as u8);
    let mut vm = run(&bc);
    assert_eq!(pop_u64(&mut vm), 1);

    let mut bc2 = Vec::new();
    bc2.extend(push_f32_bytes(2.5));
    bc2.extend(push_f32_bytes(3.0));
    bc2.push(RpnOp::FCmpEq as u8);
    let mut vm2 = run(&bc2);
    assert_eq!(pop_u64(&mut vm2), 0);
}

// ── Type Conversion ──

#[test]
fn test_i2f() {
    let mut bc = Vec::new();
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&42u32.to_le_bytes());
    bc.push(RpnOp::I2F as u8);
    let mut vm = run(&bc);
    let result = pop_f32(&mut vm);
    assert!((result - 42.0).abs() < 1e-6, "I2F: expected 42.0, got {}", result);
}

#[test]
fn test_f2i() {
    let mut bc = Vec::new();
    bc.extend(push_f32_bytes(99.7));
    bc.push(RpnOp::F2I as u8);
    let mut vm = run(&bc);
    let val = pop_u64(&mut vm) as u32 as i32;
    assert_eq!(val, 99, "F2I: expected 99, got {}", val);
}

// ── ZeroPage i32 Load/Store ──

#[test]
fn test_load_store_zp_i32() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Store value 12345 at ZP address 8
    let mut bc = Vec::new();
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&12345u32.to_le_bytes());  // value
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&8u32.to_le_bytes());      // addr
    bc.push(RpnOp::StoreZpI32 as u8);

    // Load it back
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&8u32.to_le_bytes());
    bc.push(RpnOp::LoadZpI32 as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    let val = vm.stack.pop().unwrap().as_u64().unwrap() as u32 as i32;
    assert_eq!(val, 12345);
}

#[test]
fn test_zp_i32_negative() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Store -42 at ZP address 0
    let neg42 = (-42i32) as u32;
    let mut bc = Vec::new();
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&neg42.to_le_bytes());
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&0u32.to_le_bytes());
    bc.push(RpnOp::StoreZpI32 as u8);

    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&0u32.to_le_bytes());
    bc.push(RpnOp::LoadZpI32 as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    let val = vm.stack.pop().unwrap().as_u64().unwrap() as u32 as i32;
    assert_eq!(val, -42);
}

#[test]
fn test_zp_i32_out_of_bounds() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Try to load from address 253 (needs 4 bytes: 253,254,255,256 — out of bounds)
    let mut bc = Vec::new();
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&253u32.to_le_bytes());
    bc.push(RpnOp::LoadZpI32 as u8);

    let err = vm.execute(&bc, &mut arenas).unwrap_err();
    match err {
        matterstream_vm::rpn::RpnError::InvalidBankSlot { bank: 4, slot: 253 } => {}
        _ => panic!("Expected InvalidBankSlot, got {:?}", err),
    }
}

// ── Component-Aware Bank Access ──

#[test]
fn test_load_bank_comp_vec3() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.vec3_bank[2] = [1.0, 2.0, 3.0];

    // LoadBankComp: bank=VEC3, slot=2, comp=1 → should get 2.0
    let mut bc = Vec::new();
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&BANK_VEC3.to_le_bytes());
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&2u32.to_le_bytes());
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&1u32.to_le_bytes());
    bc.push(RpnOp::LoadBankComp as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    let bits = vm.stack.pop().unwrap().as_u64().unwrap() as u32;
    let val = f32::from_bits(bits);
    assert!((val - 2.0).abs() < 1e-6, "Expected 2.0, got {}", val);
}

#[test]
fn test_load_bank_comp_vec4_all_components() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.vec4_bank[5] = [10.0, 20.0, 30.0, 40.0];

    for comp in 0..4u32 {
        vm.stack.clear();
        let mut bc = Vec::new();
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&BANK_VEC4.to_le_bytes());
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&5u32.to_le_bytes());
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&comp.to_le_bytes());
        bc.push(RpnOp::LoadBankComp as u8);

        vm.execute(&bc, &mut arenas).unwrap();
        let bits = vm.stack.pop().unwrap().as_u64().unwrap() as u32;
        let val = f32::from_bits(bits);
        let expected = (comp + 1) as f32 * 10.0;
        assert!((val - expected).abs() < 1e-6,
            "comp {}: expected {}, got {}", comp, expected, val);
    }
}

#[test]
fn test_store_bank_comp_vec3() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.vec3_bank[0] = [0.0, 0.0, 0.0];

    // StoreBankComp: store 99.0 at bank=VEC3, slot=0, comp=2
    let bits = f32::to_bits(99.0);
    let mut bc = Vec::new();
    // push value
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&bits.to_le_bytes());
    // push bank
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&BANK_VEC3.to_le_bytes());
    // push slot
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&0u32.to_le_bytes());
    // push comp
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&2u32.to_le_bytes());
    bc.push(RpnOp::StoreBankComp as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert!((vm.vec3_bank[0][2] - 99.0).abs() < 1e-6);
}

// ── Float math pipeline (realistic game physics) ──

#[test]
fn test_physics_pipeline() {
    // Simulate: velocity += gravity, position += velocity
    // Initial: pos=100.0, vel=0.0, gravity=0.5
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.vec3_bank[0] = [0.0, 100.0, 0.0]; // [x, y, velocity]

    // Step 1: vel += 0.5
    let mut bc = Vec::new();
    // Load vel (comp 2)
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&BANK_VEC3.to_le_bytes());
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&0u32.to_le_bytes());
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&2u32.to_le_bytes());
    bc.push(RpnOp::LoadBankComp as u8);
    // Push gravity
    bc.extend(push_f32_bytes(0.5));
    bc.push(RpnOp::FAdd as u8);
    // Store vel
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&BANK_VEC3.to_le_bytes());
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&0u32.to_le_bytes());
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&2u32.to_le_bytes());
    bc.push(RpnOp::StoreBankComp as u8);

    // Step 2: pos += vel
    // Load pos (comp 1)
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&BANK_VEC3.to_le_bytes());
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&0u32.to_le_bytes());
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&1u32.to_le_bytes());
    bc.push(RpnOp::LoadBankComp as u8);
    // Load vel
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&BANK_VEC3.to_le_bytes());
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&0u32.to_le_bytes());
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&2u32.to_le_bytes());
    bc.push(RpnOp::LoadBankComp as u8);
    bc.push(RpnOp::FAdd as u8);
    // Store pos
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&BANK_VEC3.to_le_bytes());
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&0u32.to_le_bytes());
    bc.push(RpnOp::Push32 as u8); bc.extend_from_slice(&1u32.to_le_bytes());
    bc.push(RpnOp::StoreBankComp as u8);

    vm.execute(&bc, &mut arenas).unwrap();

    let vel = vm.vec3_bank[0][2];
    let pos = vm.vec3_bank[0][1];
    assert!((vel - 0.5).abs() < 1e-6, "vel should be 0.5, got {}", vel);
    assert!((pos - 100.5).abs() < 1e-6, "pos should be 100.5, got {}", pos);
}
