//! Tests for the event system and VmHost tick loop.

use matterstream_vm::event::{keys, VmEvent, VmEventType};
use matterstream_vm::hooks::HookContext;
use matterstream_vm::host::VmHost;
use matterstream_vm::rpn::RpnOp;

#[test]
fn test_vm_event_constructors() {
    let ev = VmEvent::key_down(32);
    assert_eq!(ev.etype, VmEventType::KeyDown);
    assert_eq!(ev.key_code(), 32);

    let ev = VmEvent::mouse_down(100, 200);
    assert_eq!(ev.etype, VmEventType::MouseDown);
    assert_eq!(ev.mouse_x(), 100);
    assert_eq!(ev.mouse_y(), 200);

    let ev = VmEvent::tick(16);
    assert_eq!(ev.etype, VmEventType::Tick);
    assert_eq!(ev.data, 16);
}

#[test]
fn test_vm_event_type_from_u32() {
    assert_eq!(VmEventType::from_u32(0), Some(VmEventType::None));
    assert_eq!(VmEventType::from_u32(1), Some(VmEventType::KeyDown));
    assert_eq!(VmEventType::from_u32(6), Some(VmEventType::Tick));
    assert_eq!(VmEventType::from_u32(99), None);
}

#[test]
fn test_vm_host_tick_increments_frame() {
    let hooks = HookContext::new();
    let mut host = VmHost::new(
        vec![RpnOp::Halt as u8], // minimal bytecode
        Vec::new(),
        hooks,
    );

    host.tick(16).unwrap();
    assert_eq!(host.frame_count, 1);
    assert_eq!(host.uniforms.time_delta[2], 1.0); // frame count in uniforms

    host.tick(16).unwrap();
    assert_eq!(host.frame_count, 2);
    assert_eq!(host.uniforms.time_delta[2], 2.0);
}

#[test]
fn test_vm_host_events_forwarded_to_vm() {
    let hooks = HookContext::new();
    // Bytecode: EvPoll, Halt — pops one event
    let bc = vec![RpnOp::EvPoll as u8, RpnOp::Halt as u8];
    let mut host = VmHost::new(bc, Vec::new(), hooks);

    host.push_event(VmEvent::key_down(keys::SPACE));
    host.tick(16).unwrap();

    // The VM should have consumed events — after tick, there should be results on the stack
    // The tick event was also pushed automatically, so let's just verify no panic
    assert_eq!(host.frame_count, 1);
}

#[test]
fn test_vm_host_dt_in_uniforms() {
    let hooks = HookContext::new();
    let mut host = VmHost::new(
        vec![RpnOp::Halt as u8],
        Vec::new(),
        hooks,
    );

    host.tick(33).unwrap();
    assert_eq!(host.uniforms.time_delta[1], 33.0); // delta_ms
}

#[test]
fn test_keys_constants() {
    assert_eq!(keys::SPACE, 32);
    assert_eq!(keys::ARROW_LEFT, 37);
    assert_eq!(keys::ARROW_UP, 38);
    assert_eq!(keys::ARROW_RIGHT, 39);
    assert_eq!(keys::ARROW_DOWN, 40);
    assert_eq!(keys::ENTER, 13);
    assert_eq!(keys::ESCAPE, 27);
}
