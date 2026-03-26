//! User atomics demo — simulates mic state toggle with background thread.
//!
//! Pattern: useMicState() hook backed by ReadUserAtomic / SubmitUserSemaphore.
//! A background thread toggles the mic state every 500ms.
//! The VM reads the state each tick and prints the result.
//! The VM can also submit a toggle request back to the host.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use matterstream_vm::rpn::{RpnOp, RpnVm, RpnValue, UserCallOp};
use matterstream_vm_arena::TripleArena;

const SLOT_MIC_STATE: u32 = 0;
const SLOT_MIC_TOGGLE: u32 = 0;

fn main() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Bytecode: ReadUserAtomic(0) → pushes mic state to stack
    let read_bc = {
        let mut bc = Vec::new();
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&SLOT_MIC_STATE.to_le_bytes());
        bc.push(RpnOp::UserCall as u8);
        bc.extend_from_slice(&(UserCallOp::ReadUserAtomic as u64).to_le_bytes());
        bc.extend_from_slice(&0u64.to_le_bytes());
        bc.push(RpnOp::Halt as u8);
        bc
    };

    // Bytecode: SubmitUserSemaphore(0, 1) → signal toggle to host
    let submit_bc = {
        let mut bc = Vec::new();
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&SLOT_MIC_TOGGLE.to_le_bytes());
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&1u32.to_le_bytes());
        bc.push(RpnOp::UserCall as u8);
        bc.extend_from_slice(&(UserCallOp::SubmitUserSemaphore as u64).to_le_bytes());
        bc.extend_from_slice(&0u64.to_le_bytes());
        bc.push(RpnOp::Halt as u8);
        bc
    };

    println!("=== User Atomics Demo: useMicState() ===\n");

    // Shared state between host thread and main loop
    let mic_state = Arc::new(AtomicU32::new(0));
    let toggle_request = Arc::new(AtomicU32::new(0));

    // Background "host" thread — simulates audio system toggling mic
    let host_mic = Arc::clone(&mic_state);
    let host_toggle = Arc::clone(&toggle_request);
    let handle = thread::spawn(move || {
        for i in 0..10 {
            thread::sleep(Duration::from_millis(500));

            // Check if VM submitted a toggle request
            let req = host_toggle.swap(0, Ordering::Relaxed);
            if req != 0 {
                println!("  [host] Received toggle request from VM!");
            }

            // Toggle mic state
            let current = host_mic.load(Ordering::Relaxed);
            let new_val = if current == 0 { 1 } else { 0 };
            host_mic.store(new_val, Ordering::Relaxed);
            println!("  [host] Tick {}: mic → {}", i, if new_val == 1 { "ON" } else { "OFF" });
        }
    });

    // Main loop: sync shared state ↔ VM atomics, execute bytecode
    for tick in 0..20 {
        thread::sleep(Duration::from_millis(250));

        // Host → VM: sync readable atomics
        vm.user_atomics_readable[SLOT_MIC_STATE as usize]
            .store(mic_state.load(Ordering::Relaxed), Ordering::Relaxed);

        // Execute: read mic state via ReadUserAtomic
        vm.execute(&read_bc, &mut arenas).unwrap();

        let mic_on = match &vm.stack[0] {
            RpnValue::U32(v) => *v != 0,
            _ => false,
        };
        println!("[vm] Tick {:2}: mic = {} {}", tick,
            if mic_on { 1 } else { 0 },
            if mic_on { "🎤 ON" } else { "   off" }
        );

        // At tick 7: VM submits a toggle request
        if tick == 7 {
            println!("[vm] >>> Submitting toggle request via SubmitUserSemaphore");
            vm.execute(&submit_bc, &mut arenas).unwrap();

            // VM → Host: sync submit semaphore
            let val = vm.user_atomics_submit[SLOT_MIC_TOGGLE as usize]
                .swap(0, Ordering::Relaxed);
            if val != 0 {
                toggle_request.store(val, Ordering::Relaxed);
            }
        }
    }

    handle.join().unwrap();
    println!("\n=== Demo complete ===");
}
