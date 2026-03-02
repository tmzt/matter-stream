//! React-style hooks that allocate slots in typed memory banks.

use crate::host::GpuUniforms;

/// Which memory bank a hook slot lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankKind {
    /// Tier 1 ScalarBank — f32, 16 slots.
    Scalar = 0,
    /// Tier 1 IntBank — i32, 16 slots.
    Int = 1,
    /// Tier 1 Vec3Bank — [f32; 3], 16 slots.
    Vec3 = 2,
    /// Tier 1 Vec4Bank — [f32; 4], 16 slots.
    Vec4 = 3,
    /// Tier 2 ZeroPage — raw bytes, 256 bytes (for arrays/grids).
    ZeroPage = 4,
}

/// Typed handle for a hook state slot.
#[derive(Debug, Clone, Copy)]
pub struct StateSlot {
    pub bank: BankKind,
    pub index: u32,
    pub count: u32,
}

/// Click handler registered by a Button component.
pub struct OnClickHandler {
    pub button_id: u32,
    pub target: StateSlot,
    pub new_value: u64,
    pub bounds: [f32; 4], // x, y, w, h
}

/// Hook context: tracks all allocated state and handlers.
pub struct HookContext {
    pub slots: Vec<StateSlot>,
    pub click_handlers: Vec<OnClickHandler>,
    next_scalar: u32,
    next_int: u32,
    next_vec3: u32,
    next_vec4: u32,
    next_zp_byte: u32,
    // Initial values to apply to uniforms
    initial_scalars: Vec<(u32, f32)>,
    initial_ints: Vec<(u32, i32)>,
    initial_vec3s: Vec<(u32, [f32; 3])>,
    initial_vec4s: Vec<(u32, [f32; 4])>,
    initial_zp: Vec<(u32, u32, i32)>, // (offset, count, value)
}

impl HookContext {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            click_handlers: Vec::new(),
            next_scalar: 0,
            next_int: 0,
            next_vec3: 0,
            next_vec4: 0,
            next_zp_byte: 0,
            initial_scalars: Vec::new(),
            initial_ints: Vec::new(),
            initial_vec3s: Vec::new(),
            initial_vec4s: Vec::new(),
            initial_zp: Vec::new(),
        }
    }

    /// useState<f32>(initial) — allocate 1 scalar bank register.
    pub fn use_state_f32(&mut self, initial: f32) -> StateSlot {
        let idx = self.next_scalar;
        self.next_scalar += 1;
        let slot = StateSlot {
            bank: BankKind::Scalar,
            index: idx,
            count: 1,
        };
        self.slots.push(slot);
        self.initial_scalars.push((idx, initial));
        slot
    }

    /// useState<i32>(initial) — allocate 1 int bank register.
    pub fn use_state_i32(&mut self, initial: i32) -> StateSlot {
        let idx = self.next_int;
        self.next_int += 1;
        let slot = StateSlot {
            bank: BankKind::Int,
            index: idx,
            count: 1,
        };
        self.slots.push(slot);
        self.initial_ints.push((idx, initial));
        slot
    }

    /// useState<[f32;4]>(initial) — allocate 1 vec4 bank register.
    pub fn use_state_vec4(&mut self, initial: [f32; 4]) -> StateSlot {
        let idx = self.next_vec4;
        self.next_vec4 += 1;
        let slot = StateSlot {
            bank: BankKind::Vec4,
            index: idx,
            count: 1,
        };
        self.slots.push(slot);
        self.initial_vec4s.push((idx, initial));
        slot
    }

    /// useState<[f32;3]>(initial) — allocate 1 vec3 bank register.
    pub fn use_state_vec3(&mut self, initial: [f32; 3]) -> StateSlot {
        let idx = self.next_vec3;
        self.next_vec3 += 1;
        let slot = StateSlot {
            bank: BankKind::Vec3,
            index: idx,
            count: 1,
        };
        self.slots.push(slot);
        self.initial_vec3s.push((idx, initial));
        slot
    }

    /// useState for i32 array — allocate N bytes in ZeroPage (for game grids).
    /// Each i32 element takes 4 bytes in the zero page.
    pub fn use_state_grid(&mut self, count: u32, initial: i32) -> StateSlot {
        let byte_offset = self.next_zp_byte;
        let byte_count = count * 4; // 4 bytes per i32
        self.next_zp_byte += byte_count;
        let slot = StateSlot {
            bank: BankKind::ZeroPage,
            index: byte_offset,
            count,
        };
        self.slots.push(slot);
        self.initial_zp.push((byte_offset, count, initial));
        slot
    }

    /// Register a click handler.
    pub fn on_click(&mut self, target: StateSlot, value: u64, bounds: [f32; 4]) {
        let button_id = self.click_handlers.len() as u32;
        self.click_handlers.push(OnClickHandler {
            button_id,
            target,
            new_value: value,
            bounds,
        });
    }

    /// Handle a mouse click, updating state if a button is hit.
    pub fn handle_click(&self, x: f32, y: f32, uniforms: &mut GpuUniforms) -> bool {
        for handler in &self.click_handlers {
            let [bx, by, bw, bh] = handler.bounds;
            if x >= bx && x < bx + bw && y >= by && y < by + bh {
                let slot = handler.target;
                match slot.bank {
                    BankKind::Scalar => {
                        let idx = slot.index as usize;
                        let pack_idx = idx / 4;
                        let sub_idx = idx % 4;
                        if pack_idx < 4 {
                            uniforms.scalar_bank[pack_idx][sub_idx] =
                                f32::from_bits(handler.new_value as u32);
                        }
                    }
                    BankKind::Int => {
                        let idx = slot.index as usize;
                        let pack_idx = idx / 4;
                        let sub_idx = idx % 4;
                        if pack_idx < 4 {
                            uniforms.int_bank[pack_idx][sub_idx] = handler.new_value as i32;
                        }
                    }
                    BankKind::Vec4 => {
                        let idx = slot.index as usize;
                        if idx < 16 {
                            uniforms.vec4_bank[idx][0] =
                                f32::from_bits(handler.new_value as u32);
                        }
                    }
                    BankKind::Vec3 => {
                        let idx = slot.index as usize;
                        if idx < 16 {
                            uniforms.vec3_bank[idx][0] =
                                f32::from_bits(handler.new_value as u32);
                        }
                    }
                    BankKind::ZeroPage => {
                        let byte_offset = slot.index as usize;
                        let word_idx = byte_offset / 16;
                        let sub_idx = (byte_offset % 16) / 4;
                        if word_idx < 16 {
                            uniforms.zero_page[word_idx][sub_idx] = handler.new_value as u32;
                        }
                    }
                }
                return true;
            }
        }
        false
    }

    /// Apply initial values to GpuUniforms.
    pub fn apply_initial_values(&self, uniforms: &mut GpuUniforms) {
        for &(idx, val) in &self.initial_scalars {
            let pack_idx = idx as usize / 4;
            let sub_idx = idx as usize % 4;
            if pack_idx < 4 {
                uniforms.scalar_bank[pack_idx][sub_idx] = val;
            }
        }
        for &(idx, val) in &self.initial_ints {
            let pack_idx = idx as usize / 4;
            let sub_idx = idx as usize % 4;
            if pack_idx < 4 {
                uniforms.int_bank[pack_idx][sub_idx] = val;
            }
        }
        for &(idx, val) in &self.initial_vec3s {
            if (idx as usize) < 16 {
                uniforms.vec3_bank[idx as usize] = [val[0], val[1], val[2], 0.0];
            }
        }
        for &(idx, val) in &self.initial_vec4s {
            if (idx as usize) < 16 {
                uniforms.vec4_bank[idx as usize] = val;
            }
        }
        for &(byte_offset, count, val) in &self.initial_zp {
            for i in 0..count {
                let off = (byte_offset + i * 4) as usize;
                let word_idx = off / 16;
                let sub_idx = (off % 16) / 4;
                if word_idx < 16 {
                    uniforms.zero_page[word_idx][sub_idx] = val as u32;
                }
            }
        }
    }

    /// Sync VM typed banks → GpuUniforms for GPU upload.
    pub fn sync_vm_to_uniforms(
        &self,
        vm: &crate::rpn::RpnVm,
        uniforms: &mut GpuUniforms,
    ) {
        // Sync scalar bank
        for i in 0..16usize {
            let pack_idx = i / 4;
            let sub_idx = i % 4;
            if pack_idx < 4 {
                uniforms.scalar_bank[pack_idx][sub_idx] = vm.scalar_bank[i];
            }
        }
        // Sync int bank
        for i in 0..16usize {
            let pack_idx = i / 4;
            let sub_idx = i % 4;
            if pack_idx < 4 {
                uniforms.int_bank[pack_idx][sub_idx] = vm.int_bank[i];
            }
        }
        // Sync vec3 bank
        for i in 0..16usize {
            uniforms.vec3_bank[i] = [vm.vec3_bank[i][0], vm.vec3_bank[i][1], vm.vec3_bank[i][2], 0.0];
        }
        // Sync vec4 bank
        for i in 0..16usize {
            uniforms.vec4_bank[i] = vm.vec4_bank[i];
        }
        // Sync zero page
        for word in 0..16usize {
            for sub in 0..4usize {
                let byte_offset = word * 16 + sub * 4;
                if byte_offset + 3 < 256 {
                    uniforms.zero_page[word][sub] = u32::from_le_bytes([
                        vm.zero_page[byte_offset],
                        vm.zero_page[byte_offset + 1],
                        vm.zero_page[byte_offset + 2],
                        vm.zero_page[byte_offset + 3],
                    ]);
                }
            }
        }
    }
}

impl Default for HookContext {
    fn default() -> Self {
        Self::new()
    }
}
