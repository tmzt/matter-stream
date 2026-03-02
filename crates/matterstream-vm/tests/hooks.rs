//! Tests for hook slot allocation, click handler hit testing, and state sync.

use matterstream_vm::hooks::{BankKind, HookContext};
use matterstream_vm::host::GpuUniforms;
use matterstream_vm::rpn::RpnVm;

#[test]
fn test_use_state_i32_allocation() {
    let mut ctx = HookContext::new();
    let s0 = ctx.use_state_i32(10);
    let s1 = ctx.use_state_i32(20);
    let s2 = ctx.use_state_i32(30);

    assert_eq!(s0.bank, BankKind::Int);
    assert_eq!(s0.index, 0);
    assert_eq!(s1.index, 1);
    assert_eq!(s2.index, 2);
    assert_eq!(s0.count, 1);
}

#[test]
fn test_use_state_f32_allocation() {
    let mut ctx = HookContext::new();
    let s0 = ctx.use_state_f32(1.23);
    let s1 = ctx.use_state_f32(2.71);

    assert_eq!(s0.bank, BankKind::Scalar);
    assert_eq!(s0.index, 0);
    assert_eq!(s1.index, 1);
}

#[test]
fn test_use_state_vec4_allocation() {
    let mut ctx = HookContext::new();
    let s = ctx.use_state_vec4([1.0, 2.0, 3.0, 4.0]);
    assert_eq!(s.bank, BankKind::Vec4);
    assert_eq!(s.index, 0);
    assert_eq!(s.count, 1);
}

#[test]
fn test_use_state_vec3_allocation() {
    let mut ctx = HookContext::new();
    let s = ctx.use_state_vec3([1.0, 2.0, 3.0]);
    assert_eq!(s.bank, BankKind::Vec3);
    assert_eq!(s.index, 0);
}

#[test]
fn test_use_state_grid_allocation() {
    let mut ctx = HookContext::new();
    let grid = ctx.use_state_grid(42, 0);
    assert_eq!(grid.bank, BankKind::ZeroPage);
    assert_eq!(grid.index, 0);
    assert_eq!(grid.count, 42);

    // Next grid should start after: 42 * 4 = 168 bytes
    let grid2 = ctx.use_state_grid(16, 0);
    assert_eq!(grid2.index, 168);
    assert_eq!(grid2.count, 16);
}

#[test]
fn test_apply_initial_values() {
    let mut ctx = HookContext::new();
    ctx.use_state_i32(42);
    ctx.use_state_f32(1.23);
    ctx.use_state_vec4([1.0, 2.0, 3.0, 4.0]);

    let mut uniforms = GpuUniforms::default();
    ctx.apply_initial_values(&mut uniforms);

    // Int bank: slot 0 = 42
    assert_eq!(uniforms.int_bank[0][0], 42);
    // Scalar bank: slot 0 ≈ 1.23
    assert!((uniforms.scalar_bank[0][0] - 1.23).abs() < 0.001);
    // Vec4 bank: slot 0 = [1, 2, 3, 4]
    assert_eq!(uniforms.vec4_bank[0], [1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn test_click_handler_hit_test() {
    let mut ctx = HookContext::new();
    let slot = ctx.use_state_i32(0);

    // Register click handler: button at (100, 100, 50, 50), sets value to 1
    ctx.on_click(slot, 1, [100.0, 100.0, 50.0, 50.0]);

    let mut uniforms = GpuUniforms::default();
    ctx.apply_initial_values(&mut uniforms);

    // Click outside — should not hit
    assert!(!ctx.handle_click(50.0, 50.0, &mut uniforms));
    assert_eq!(uniforms.int_bank[0][0], 0);

    // Click inside — should hit and update
    assert!(ctx.handle_click(120.0, 120.0, &mut uniforms));
    assert_eq!(uniforms.int_bank[0][0], 1);
}

#[test]
fn test_sync_vm_to_uniforms() {
    let ctx = HookContext::new();
    let mut vm = RpnVm::new();
    let mut uniforms = GpuUniforms::default();

    // Set some values on the VM
    vm.int_bank[0] = 42;
    vm.int_bank[3] = 99;
    vm.scalar_bank[0] = 1.23;
    vm.vec4_bank[0] = [1.0, 2.0, 3.0, 4.0];
    vm.vec3_bank[0] = [10.0, 20.0, 30.0];
    vm.zero_page[0] = 0xAB;
    vm.zero_page[1] = 0xCD;
    vm.zero_page[2] = 0xEF;
    vm.zero_page[3] = 0x01;

    ctx.sync_vm_to_uniforms(&vm, &mut uniforms);

    assert_eq!(uniforms.int_bank[0][0], 42);
    assert_eq!(uniforms.int_bank[0][3], 99);
    assert!((uniforms.scalar_bank[0][0] - 1.23).abs() < 0.001);
    assert_eq!(uniforms.vec4_bank[0], [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(uniforms.vec3_bank[0], [10.0, 20.0, 30.0, 0.0]);
    assert_eq!(uniforms.zero_page[0][0], 0x01EFCDAB);
}

#[test]
fn test_mixed_bank_allocation() {
    let mut ctx = HookContext::new();

    // Allocate from different banks in any order
    let a = ctx.use_state_i32(1);
    let b = ctx.use_state_f32(2.0);
    let c = ctx.use_state_i32(3);
    let d = ctx.use_state_vec4([4.0; 4]);
    let e = ctx.use_state_grid(10, 0);

    // Each bank has independent counters
    assert_eq!(a.index, 0); // first int
    assert_eq!(b.index, 0); // first scalar
    assert_eq!(c.index, 1); // second int
    assert_eq!(d.index, 0); // first vec4
    assert_eq!(e.index, 0); // first zp byte

    assert_eq!(ctx.slots.len(), 5);
}
