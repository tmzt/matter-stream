//! VmHost — frame-based execution runtime with GPU-compatible uniforms.

use std::collections::VecDeque;

use matterstream_vm_arena::TripleArena;

use crate::event::VmEvent;
use crate::hooks::HookContext;
use crate::rpn::{RpnError, RpnVm, SimpleRng};

/// GPU-compatible uniforms — separates system globals (Tier 0) from app state (Tier 1+2).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuUniforms {
    // Tier 0: System Globals
    pub time_delta: [f32; 4],    // [time_s, delta_ms, frame_count, 0]
    pub resolution: [f32; 4],    // [width, height, scale_factor, 0]
    pub mouse: [f32; 4],         // [mouse_x, mouse_y, button_state, 0]
    pub theme: [f32; 4],         // [is_dark, accent_r, accent_g, accent_b]

    // Tier 1: App-Specific Typed Register Banks
    pub vec4_bank: [[f32; 4]; 16],   // colors, RGBA, compound state
    pub vec3_bank: [[f32; 4]; 16],   // positions, velocities (padded to vec4)
    pub scalar_bank: [[f32; 4]; 4],  // 16 f32s packed into 4 vec4s
    pub int_bank: [[i32; 4]; 4],     // 16 i32s packed into 4 ivec4s

    // Tier 2: App-Specific ZeroPage (256 bytes for arrays/grids)
    pub zero_page: [[u32; 4]; 16],   // 256 bytes as 16 uvec4s
}

// Safety: GpuUniforms is repr(C) and contains only Pod types
unsafe impl bytemuck::Pod for GpuUniforms {}
unsafe impl bytemuck::Zeroable for GpuUniforms {}

impl Default for GpuUniforms {
    fn default() -> Self {
        Self {
            time_delta: [0.0; 4],
            resolution: [800.0, 600.0, 1.0, 0.0],
            mouse: [0.0; 4],
            theme: [0.0, 0.2, 0.5, 1.0],
            vec4_bank: [[0.0; 4]; 16],
            vec3_bank: [[0.0; 4]; 16],
            scalar_bank: [[0.0; 4]; 4],
            int_bank: [[0; 4]; 4],
            zero_page: [[0; 4]; 16],
        }
    }
}

/// Frame-based execution host.
pub struct VmHost {
    pub vm: RpnVm,
    pub arenas: TripleArena,
    pub logic_bytecode: Vec<u8>,
    pub render_bytecode: Vec<u32>,
    pub uniforms: GpuUniforms,
    pub event_queue: VecDeque<VmEvent>,
    pub frame_count: u64,
    pub rng: SimpleRng,
    pub hook_ctx: HookContext,
}

impl VmHost {
    pub fn new(logic: Vec<u8>, render: Vec<u32>, hooks: HookContext) -> Self {
        let mut uniforms = GpuUniforms::default();
        hooks.apply_initial_values(&mut uniforms);
        Self {
            vm: RpnVm::new(),
            arenas: TripleArena::new(),
            logic_bytecode: logic,
            render_bytecode: render,
            uniforms,
            event_queue: VecDeque::new(),
            frame_count: 0,
            rng: SimpleRng::new(0xCAFE_BABE),
            hook_ctx: hooks,
        }
    }

    /// Push an event into the host's event queue.
    pub fn push_event(&mut self, event: VmEvent) {
        self.event_queue.push_back(event);
    }

    /// Run one frame of game logic on the CPU RPN VM.
    /// Pushes a Tick event, drains the event queue into the VM,
    /// executes logic bytecode, then syncs hook state → GpuUniforms.
    pub fn tick(&mut self, dt_ms: u32) -> Result<(), RpnError> {
        // Push tick event
        self.event_queue.push_back(VmEvent::tick(dt_ms));

        // Transfer events to VM
        while let Some(ev) = self.event_queue.pop_front() {
            self.vm.event_queue.push_back(ev);
        }

        // Update frame count
        self.frame_count += 1;
        self.vm.frame_count = self.frame_count;
        self.vm.rng = self.rng.clone();

        // Execute logic bytecode
        self.vm.execute(&self.logic_bytecode, &mut self.arenas)?;

        // Sync RNG state back
        self.rng = self.vm.rng.clone();

        // Sync VM typed banks → GpuUniforms
        self.hook_ctx.sync_vm_to_uniforms(&self.vm, &mut self.uniforms);

        // Update system uniforms
        self.uniforms.time_delta[1] = dt_ms as f32;
        self.uniforms.time_delta[2] = self.frame_count as f32;

        Ok(())
    }

    /// Access uniforms for GPU upload.
    pub fn uniforms(&self) -> &GpuUniforms {
        &self.uniforms
    }

    /// Access render bytecode for GPU upload.
    pub fn render_bytecode(&self) -> &[u32] {
        &self.render_bytecode
    }
}
