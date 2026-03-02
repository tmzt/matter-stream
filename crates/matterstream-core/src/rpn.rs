//! Modified RPN stack language VM (MTSM-RPN-Bincode).
//!
//! Features:
//! - Per-opcode gas metering with configurable budgets
//! - Backward-jump detection and loop limiting
//! - Execution trace/profiling
//! - Control flow: Jmp, JmpIf, Halt, comparisons
//! - UI draw opcodes (0x40–0x48) for 2D rendering

use matterstream_vm_arena::arena::TripleArena;
use matterstream_vm_addressing::fqa::Fqa;
use matterstream_vm_addressing::ova::Ova;
use crate::ui_vm::{UiDrawCmd, UiDrawState, UI_DRAW_CMD_MAX, UI_STATE_STACK_MAX};
use std::collections::HashMap;
use std::fmt;

/// RPN opcodes (u8 repr).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RpnOp {
    Nop = 0x00,
    Push32 = 0x01,
    Push64 = 0x02,
    PushFqa = 0x03,
    Dup = 0x04,
    Drop = 0x05,
    Swap = 0x06,
    Add = 0x07,
    Sub = 0x08,
    Mul = 0x09,
    Div = 0x0A,
    Load = 0x0B,
    Store = 0x0C,
    Call = 0x0D,
    Ret = 0x0E,
    Sync = 0x0F,
    MapNew = 0x10,
    MapSet = 0x11,
    MapGet = 0x12,
    // Control flow & comparison opcodes (v0.1.1)
    Jmp = 0x13,
    JmpIf = 0x14,
    Halt = 0x15,
    Mod = 0x16,
    CmpEq = 0x17,
    CmpLt = 0x18,
    CmpGt = 0x19,
    // UI draw opcodes (v0.2.0)
    UiSetColor = 0x40,
    UiBox = 0x41,
    UiSlab = 0x42,
    UiCircle = 0x43,
    UiText = 0x44,
    UiPushState = 0x45,
    UiPopState = 0x46,
    UiSetOffset = 0x47,
    UiLine = 0x48,
}

impl RpnOp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(RpnOp::Nop),
            0x01 => Some(RpnOp::Push32),
            0x02 => Some(RpnOp::Push64),
            0x03 => Some(RpnOp::PushFqa),
            0x04 => Some(RpnOp::Dup),
            0x05 => Some(RpnOp::Drop),
            0x06 => Some(RpnOp::Swap),
            0x07 => Some(RpnOp::Add),
            0x08 => Some(RpnOp::Sub),
            0x09 => Some(RpnOp::Mul),
            0x0A => Some(RpnOp::Div),
            0x0B => Some(RpnOp::Load),
            0x0C => Some(RpnOp::Store),
            0x0D => Some(RpnOp::Call),
            0x0E => Some(RpnOp::Ret),
            0x0F => Some(RpnOp::Sync),
            0x10 => Some(RpnOp::MapNew),
            0x11 => Some(RpnOp::MapSet),
            0x12 => Some(RpnOp::MapGet),
            0x13 => Some(RpnOp::Jmp),
            0x14 => Some(RpnOp::JmpIf),
            0x15 => Some(RpnOp::Halt),
            0x16 => Some(RpnOp::Mod),
            0x17 => Some(RpnOp::CmpEq),
            0x18 => Some(RpnOp::CmpLt),
            0x19 => Some(RpnOp::CmpGt),
            0x40 => Some(RpnOp::UiSetColor),
            0x41 => Some(RpnOp::UiBox),
            0x42 => Some(RpnOp::UiSlab),
            0x43 => Some(RpnOp::UiCircle),
            0x44 => Some(RpnOp::UiText),
            0x45 => Some(RpnOp::UiPushState),
            0x46 => Some(RpnOp::UiPopState),
            0x47 => Some(RpnOp::UiSetOffset),
            0x48 => Some(RpnOp::UiLine),
            _ => None,
        }
    }

    /// Payload size in bytes for this opcode.
    pub fn payload_size(&self) -> usize {
        match self {
            RpnOp::Push32 => 4,
            RpnOp::Push64 | RpnOp::Jmp | RpnOp::JmpIf => 8,
            RpnOp::PushFqa => 16,
            _ => 0,
        }
    }
}

/// Per-opcode gas costs. Higher costs for expensive operations.
#[derive(Debug, Clone)]
pub struct GasConfig {
    pub gas_budget: u64,
    pub max_backward_jumps: u64,
    // Per-opcode costs
    pub cost_nop: u64,
    pub cost_push: u64,
    pub cost_stack_op: u64,
    pub cost_arithmetic: u64,
    pub cost_memory: u64,
    pub cost_call: u64,
    pub cost_sync: u64,
    pub cost_map: u64,
    pub cost_jump: u64,
    pub cost_compare: u64,
    pub cost_ui: u64,
}

impl GasConfig {
    pub fn new(budget: u64) -> Self {
        Self {
            gas_budget: budget,
            max_backward_jumps: 10_000,
            cost_nop: 1,
            cost_push: 1,
            cost_stack_op: 1,
            cost_arithmetic: 2,
            cost_memory: 10,
            cost_call: 5,
            cost_sync: 100,
            cost_map: 5,
            cost_jump: 2,
            cost_compare: 2,
            cost_ui: 5,
        }
    }

    /// Gas cost for a given opcode.
    pub fn cost_of(&self, op: RpnOp) -> u64 {
        match op {
            RpnOp::Nop | RpnOp::Halt => self.cost_nop,
            RpnOp::Push32 | RpnOp::Push64 | RpnOp::PushFqa => self.cost_push,
            RpnOp::Dup | RpnOp::Drop | RpnOp::Swap => self.cost_stack_op,
            RpnOp::Add | RpnOp::Sub | RpnOp::Mul | RpnOp::Div | RpnOp::Mod => self.cost_arithmetic,
            RpnOp::Load | RpnOp::Store => self.cost_memory,
            RpnOp::Call | RpnOp::Ret => self.cost_call,
            RpnOp::Sync => self.cost_sync,
            RpnOp::MapNew | RpnOp::MapSet | RpnOp::MapGet => self.cost_map,
            RpnOp::Jmp | RpnOp::JmpIf => self.cost_jump,
            RpnOp::CmpEq | RpnOp::CmpLt | RpnOp::CmpGt => self.cost_compare,
            RpnOp::UiSetColor | RpnOp::UiBox | RpnOp::UiSlab | RpnOp::UiCircle
            | RpnOp::UiText | RpnOp::UiPushState | RpnOp::UiPopState
            | RpnOp::UiSetOffset | RpnOp::UiLine => self.cost_ui,
        }
    }
}

impl Default for GasConfig {
    fn default() -> Self {
        Self::new(10_000_000)
    }
}

/// Execution trace collected during VM run.
#[derive(Debug, Clone, Default)]
pub struct ExecTrace {
    pub gas_consumed: u64,
    pub opcodes_executed: u64,
    pub backward_jumps: u64,
    pub forward_jumps: u64,
    pub calls: u64,
    pub syncs: u64,
    pub max_stack_depth_seen: usize,
    pub halted: bool,
}

/// Values on the RPN stack.
#[derive(Debug, Clone)]
pub enum RpnValue {
    U32(u32),
    U64(u64),
    Fqa(Fqa),
    Ova(Ova),
    Map(HashMap<u64, RpnValue>),
}

impl RpnValue {
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            RpnValue::U32(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            RpnValue::U64(v) => Some(*v),
            RpnValue::U32(v) => Some(*v as u64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpnError {
    StackUnderflow,
    StackOverflow,
    TypeMismatch,
    DivisionByZero,
    CycleLimitExceeded,
    InvalidOpcode(u8),
    TruncatedBytecode,
    ArenaError(String),
    CallStackUnderflow,
    GasExhausted { used: u64, budget: u64 },
    BackwardJumpLimitExceeded { count: u64, limit: u64 },
    InvalidJumpTarget(u64),
    UiStateStackOverflow,
    UiStateStackUnderflow,
    UiDrawLimitExceeded,
}

impl fmt::Display for RpnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpnError::StackUnderflow => write!(f, "RPN stack underflow"),
            RpnError::StackOverflow => write!(f, "RPN stack overflow"),
            RpnError::TypeMismatch => write!(f, "RPN type mismatch"),
            RpnError::DivisionByZero => write!(f, "RPN division by zero"),
            RpnError::InvalidOpcode(op) => write!(f, "RPN invalid opcode: {:#04x}", op),
            RpnError::TruncatedBytecode => write!(f, "RPN truncated bytecode"),
            RpnError::ArenaError(e) => write!(f, "RPN arena error: {}", e),
            RpnError::CallStackUnderflow => write!(f, "RPN call stack underflow"),
            RpnError::CycleLimitExceeded => write!(f, "RPN cycle limit exceeded"),
            RpnError::GasExhausted { used, budget } => {
                write!(f, "RPN gas exhausted: used {} of {} budget", used, budget)
            }
            RpnError::BackwardJumpLimitExceeded { count, limit } => {
                write!(f, "RPN backward jump limit exceeded: {} jumps (limit {})", count, limit)
            }
            RpnError::InvalidJumpTarget(target) => {
                write!(f, "RPN invalid jump target: {}", target)
            }
            RpnError::UiStateStackOverflow => write!(f, "RPN UI state stack overflow"),
            RpnError::UiStateStackUnderflow => write!(f, "RPN UI state stack underflow"),
            RpnError::UiDrawLimitExceeded => write!(f, "RPN UI draw limit exceeded"),
        }
    }
}

/// The RPN stack machine.
pub struct RpnVm {
    pub stack: Vec<RpnValue>,
    pub call_stack: Vec<usize>,
    pub pc: usize,
    pub max_stack_depth: usize,
    pub max_cycles: usize,
    pub synced: bool,
    pub gas: GasConfig,
    pub trace: ExecTrace,
    // UI draw state
    pub ui_draws: Vec<UiDrawCmd>,
    pub ui_state: UiDrawState,
    pub ui_state_stack: Vec<UiDrawState>,
}

impl RpnVm {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            call_stack: Vec::new(),
            pc: 0,
            max_stack_depth: 256,
            max_cycles: 1_000_000,
            synced: false,
            gas: GasConfig::default(),
            trace: ExecTrace::default(),
            ui_draws: Vec::new(),
            ui_state: UiDrawState::default(),
            ui_state_stack: Vec::new(),
        }
    }

    /// Create a VM with a specific gas budget.
    pub fn with_gas(budget: u64) -> Self {
        let mut vm = Self::new();
        vm.gas = GasConfig::new(budget);
        vm
    }

    /// Create a VM with a specific gas configuration.
    pub fn with_gas_config(config: GasConfig) -> Self {
        let mut vm = Self::new();
        vm.gas = config;
        vm
    }

    fn push(&mut self, val: RpnValue) -> Result<(), RpnError> {
        if self.stack.len() >= self.max_stack_depth {
            return Err(RpnError::StackOverflow);
        }
        self.stack.push(val);
        if self.stack.len() > self.trace.max_stack_depth_seen {
            self.trace.max_stack_depth_seen = self.stack.len();
        }
        Ok(())
    }

    fn pop(&mut self) -> Result<RpnValue, RpnError> {
        self.stack.pop().ok_or(RpnError::StackUnderflow)
    }

    fn pop_u64(&mut self) -> Result<u64, RpnError> {
        let v = self.pop()?;
        v.as_u64().ok_or(RpnError::TypeMismatch)
    }

    fn pop_u32_coerce(&mut self) -> Result<u32, RpnError> {
        let v = self.pop()?;
        match v {
            RpnValue::U32(x) => Ok(x),
            RpnValue::U64(x) => Ok(x as u32),
            _ => Err(RpnError::TypeMismatch),
        }
    }

    fn read_u8(bytecode: &[u8], pos: usize) -> Result<u8, RpnError> {
        bytecode.get(pos).copied().ok_or(RpnError::TruncatedBytecode)
    }

    fn read_u32(bytecode: &[u8], pos: usize) -> Result<u32, RpnError> {
        if pos + 4 > bytecode.len() {
            return Err(RpnError::TruncatedBytecode);
        }
        Ok(u32::from_le_bytes(bytecode[pos..pos + 4].try_into().unwrap()))
    }

    fn read_u64(bytecode: &[u8], pos: usize) -> Result<u64, RpnError> {
        if pos + 8 > bytecode.len() {
            return Err(RpnError::TruncatedBytecode);
        }
        Ok(u64::from_le_bytes(bytecode[pos..pos + 8].try_into().unwrap()))
    }

    fn read_u128(bytecode: &[u8], pos: usize) -> Result<u128, RpnError> {
        if pos + 16 > bytecode.len() {
            return Err(RpnError::TruncatedBytecode);
        }
        Ok(u128::from_le_bytes(bytecode[pos..pos + 16].try_into().unwrap()))
    }

    /// Consume gas for an opcode. Returns error if budget exceeded.
    fn consume_gas(&mut self, op: RpnOp) -> Result<(), RpnError> {
        let cost = self.gas.cost_of(op);
        self.trace.gas_consumed += cost;
        if self.trace.gas_consumed > self.gas.gas_budget {
            return Err(RpnError::GasExhausted {
                used: self.trace.gas_consumed,
                budget: self.gas.gas_budget,
            });
        }
        Ok(())
    }

    /// Track a jump and check backward-jump limits.
    fn track_jump(&mut self, from_pc: usize, to_pc: usize) -> Result<(), RpnError> {
        if to_pc <= from_pc {
            self.trace.backward_jumps += 1;
            if self.trace.backward_jumps > self.gas.max_backward_jumps {
                return Err(RpnError::BackwardJumpLimitExceeded {
                    count: self.trace.backward_jumps,
                    limit: self.gas.max_backward_jumps,
                });
            }
        } else {
            self.trace.forward_jumps += 1;
        }
        Ok(())
    }

    /// Execute bytecode against arenas. Returns when bytecode is exhausted or Ret with empty call stack.
    pub fn execute(
        &mut self,
        bytecode: &[u8],
        arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        self.pc = 0;
        self.synced = false;
        self.trace = ExecTrace::default();
        self.ui_draws.clear();
        self.ui_state = UiDrawState::default();
        self.ui_state_stack.clear();
        let mut cycles = 0usize;

        while self.pc < bytecode.len() {
            cycles += 1;
            if cycles > self.max_cycles {
                return Err(RpnError::CycleLimitExceeded);
            }
            self.step(bytecode, arenas)?;
            if self.trace.halted {
                break;
            }
        }

        Ok(())
    }

    /// Execute bytecode with gas metering. Returns the execution trace on success.
    pub fn execute_metered(
        &mut self,
        bytecode: &[u8],
        arenas: &mut TripleArena,
    ) -> Result<ExecTrace, RpnError> {
        self.execute(bytecode, arenas)?;
        Ok(self.trace.clone())
    }

    /// Decode and execute a single instruction.
    pub fn step(
        &mut self,
        bytecode: &[u8],
        arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        let op_byte = Self::read_u8(bytecode, self.pc)?;
        let op = RpnOp::from_u8(op_byte).ok_or(RpnError::InvalidOpcode(op_byte))?;

        // Gas metering
        self.consume_gas(op)?;
        self.trace.opcodes_executed += 1;

        match op {
            RpnOp::Nop => {
                self.pc += 1;
            }
            RpnOp::Push32 => {
                let val = Self::read_u32(bytecode, self.pc + 1)?;
                self.push(RpnValue::U32(val))?;
                self.pc += 5;
            }
            RpnOp::Push64 => {
                let val = Self::read_u64(bytecode, self.pc + 1)?;
                self.push(RpnValue::U64(val))?;
                self.pc += 9;
            }
            RpnOp::PushFqa => {
                let val = Self::read_u128(bytecode, self.pc + 1)?;
                self.push(RpnValue::Fqa(Fqa::new(val)))?;
                self.pc += 17;
            }
            RpnOp::Dup => {
                let val = self.pop()?;
                self.push(val.clone())?;
                self.push(val)?;
                self.pc += 1;
            }
            RpnOp::Drop => {
                self.pop()?;
                self.pc += 1;
            }
            RpnOp::Swap => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(b)?;
                self.push(a)?;
                self.pc += 1;
            }
            RpnOp::Add => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a.wrapping_add(b)))?;
                self.pc += 1;
            }
            RpnOp::Sub => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a.wrapping_sub(b)))?;
                self.pc += 1;
            }
            RpnOp::Mul => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a.wrapping_mul(b)))?;
                self.pc += 1;
            }
            RpnOp::Div => {
                let b = self.pop_u64()?;
                if b == 0 {
                    return Err(RpnError::DivisionByZero);
                }
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a / b))?;
                self.pc += 1;
            }
            RpnOp::Mod => {
                let b = self.pop_u64()?;
                if b == 0 {
                    return Err(RpnError::DivisionByZero);
                }
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a % b))?;
                self.pc += 1;
            }
            RpnOp::Load => {
                let ova_val = self.pop()?;
                let ova = match ova_val {
                    RpnValue::Ova(o) => o,
                    RpnValue::U32(v) => Ova(v),
                    _ => return Err(RpnError::TypeMismatch),
                };
                let data = arenas
                    .read(ova)
                    .map_err(|e| RpnError::ArenaError(e.to_string()))?;
                // Load first 4 bytes as u32
                if data.len() >= 4 {
                    let val = u32::from_le_bytes(data[..4].try_into().unwrap());
                    self.push(RpnValue::U32(val))?;
                } else {
                    self.push(RpnValue::U32(0))?;
                }
                self.pc += 1;
            }
            RpnOp::Store => {
                let ova_val = self.pop()?;
                let ova = match ova_val {
                    RpnValue::Ova(o) => o,
                    RpnValue::U32(v) => Ova(v),
                    _ => return Err(RpnError::TypeMismatch),
                };
                let val = self.pop_u64()?;
                let bytes = (val as u32).to_le_bytes();
                arenas
                    .write(ova, &bytes)
                    .map_err(|e| RpnError::ArenaError(e.to_string()))?;
                self.pc += 1;
            }
            RpnOp::Call => {
                let target = self.pop_u64()? as usize;
                if target >= bytecode.len() {
                    return Err(RpnError::InvalidJumpTarget(target as u64));
                }
                let from_pc = self.pc;
                self.call_stack.push(self.pc + 1);
                self.pc = target;
                self.trace.calls += 1;
                self.track_jump(from_pc, target)?;
            }
            RpnOp::Ret => {
                if let Some(return_pc) = self.call_stack.pop() {
                    self.pc = return_pc;
                } else {
                    // Ret with empty call stack -- halt execution
                    self.pc = bytecode.len();
                }
            }
            RpnOp::Sync => {
                arenas.sync();
                self.synced = true;
                self.trace.syncs += 1;
                self.pc += 1;
            }
            RpnOp::MapNew => {
                self.push(RpnValue::Map(HashMap::new()))?;
                self.pc += 1;
            }
            RpnOp::MapSet => {
                let val = self.pop()?;
                let key = self.pop_u64()?;
                let mut map_val = self.pop()?;
                if let RpnValue::Map(ref mut map) = map_val {
                    map.insert(key, val);
                    self.push(map_val)?;
                } else {
                    return Err(RpnError::TypeMismatch);
                }
                self.pc += 1;
            }
            RpnOp::MapGet => {
                let key = self.pop_u64()?;
                let map_val = self.pop()?;
                if let RpnValue::Map(ref map) = map_val {
                    if let Some(val) = map.get(&key) {
                        self.push(val.clone())?;
                    } else {
                        self.push(RpnValue::U32(0))?;
                    }
                } else {
                    return Err(RpnError::TypeMismatch);
                }
                self.pc += 1;
            }
            // --- New control flow opcodes ---
            RpnOp::Jmp => {
                // Unconditional jump: target is inline u64
                let target = Self::read_u64(bytecode, self.pc + 1)? as usize;
                if target > bytecode.len() {
                    return Err(RpnError::InvalidJumpTarget(target as u64));
                }
                let from_pc = self.pc;
                self.pc = target;
                self.track_jump(from_pc, target)?;
            }
            RpnOp::JmpIf => {
                // Conditional jump: pops condition, target is inline u64
                let target = Self::read_u64(bytecode, self.pc + 1)? as usize;
                let cond = self.pop_u64()?;
                if cond != 0 {
                    if target > bytecode.len() {
                        return Err(RpnError::InvalidJumpTarget(target as u64));
                    }
                    let from_pc = self.pc;
                    self.pc = target;
                    self.track_jump(from_pc, target)?;
                } else {
                    self.pc += 9; // skip opcode + 8-byte target
                }
            }
            RpnOp::Halt => {
                self.trace.halted = true;
                self.pc = bytecode.len();
            }
            RpnOp::CmpEq => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(if a == b { 1 } else { 0 }))?;
                self.pc += 1;
            }
            RpnOp::CmpLt => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(if a < b { 1 } else { 0 }))?;
                self.pc += 1;
            }
            RpnOp::CmpGt => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(if a > b { 1 } else { 0 }))?;
                self.pc += 1;
            }
            // --- UI draw opcodes ---
            RpnOp::UiSetColor => {
                let rgba = self.pop_u32_coerce()?;
                self.ui_state.color = rgba;
                self.pc += 1;
            }
            RpnOp::UiBox => {
                if self.ui_draws.len() >= UI_DRAW_CMD_MAX {
                    return Err(RpnError::UiDrawLimitExceeded);
                }
                let h = self.pop_u32_coerce()?;
                let w = self.pop_u32_coerce()?;
                let y = self.pop_u32_coerce()? as i32 + self.ui_state.offset_y;
                let x = self.pop_u32_coerce()? as i32 + self.ui_state.offset_x;
                self.ui_draws.push(UiDrawCmd::Box {
                    x, y, w, h,
                    color: self.ui_state.color,
                });
                self.pc += 1;
            }
            RpnOp::UiSlab => {
                if self.ui_draws.len() >= UI_DRAW_CMD_MAX {
                    return Err(RpnError::UiDrawLimitExceeded);
                }
                let radius = self.pop_u32_coerce()?;
                let h = self.pop_u32_coerce()?;
                let w = self.pop_u32_coerce()?;
                let y = self.pop_u32_coerce()? as i32 + self.ui_state.offset_y;
                let x = self.pop_u32_coerce()? as i32 + self.ui_state.offset_x;
                self.ui_draws.push(UiDrawCmd::Slab {
                    x, y, w, h, radius,
                    color: self.ui_state.color,
                });
                self.pc += 1;
            }
            RpnOp::UiCircle => {
                if self.ui_draws.len() >= UI_DRAW_CMD_MAX {
                    return Err(RpnError::UiDrawLimitExceeded);
                }
                let r = self.pop_u32_coerce()?;
                let y = self.pop_u32_coerce()? as i32 + self.ui_state.offset_y;
                let x = self.pop_u32_coerce()? as i32 + self.ui_state.offset_x;
                self.ui_draws.push(UiDrawCmd::Circle {
                    x, y, r,
                    color: self.ui_state.color,
                });
                self.pc += 1;
            }
            RpnOp::UiText => {
                if self.ui_draws.len() >= UI_DRAW_CMD_MAX {
                    return Err(RpnError::UiDrawLimitExceeded);
                }
                let slot = self.pop_u32_coerce()?;
                let size = self.pop_u32_coerce()?;
                let y = self.pop_u32_coerce()? as i32 + self.ui_state.offset_y;
                let x = self.pop_u32_coerce()? as i32 + self.ui_state.offset_x;
                self.ui_draws.push(UiDrawCmd::Text {
                    x, y, size, slot,
                    color: self.ui_state.color,
                });
                self.pc += 1;
            }
            RpnOp::UiPushState => {
                if self.ui_state_stack.len() >= UI_STATE_STACK_MAX {
                    return Err(RpnError::UiStateStackOverflow);
                }
                self.ui_state_stack.push(self.ui_state);
                self.pc += 1;
            }
            RpnOp::UiPopState => {
                self.ui_state = self.ui_state_stack.pop()
                    .ok_or(RpnError::UiStateStackUnderflow)?;
                self.pc += 1;
            }
            RpnOp::UiSetOffset => {
                let dy = self.pop_u32_coerce()? as i32;
                let dx = self.pop_u32_coerce()? as i32;
                self.ui_state.offset_x = dx;
                self.ui_state.offset_y = dy;
                self.pc += 1;
            }
            RpnOp::UiLine => {
                if self.ui_draws.len() >= UI_DRAW_CMD_MAX {
                    return Err(RpnError::UiDrawLimitExceeded);
                }
                let y2 = self.pop_u32_coerce()? as i32 + self.ui_state.offset_y;
                let x2 = self.pop_u32_coerce()? as i32 + self.ui_state.offset_x;
                let y1 = self.pop_u32_coerce()? as i32 + self.ui_state.offset_y;
                let x1 = self.pop_u32_coerce()? as i32 + self.ui_state.offset_x;
                self.ui_draws.push(UiDrawCmd::Line {
                    x1, y1, x2, y2,
                    color: self.ui_state.color,
                });
                self.pc += 1;
            }
        }

        Ok(())
    }

    /// Encode a sequence of instructions to bytecode.
    pub fn encode(instructions: &[(RpnOp, Option<&[u8]>)]) -> Vec<u8> {
        let mut buf = Vec::new();
        for (op, payload) in instructions {
            buf.push(*op as u8);
            if let Some(data) = payload {
                buf.extend_from_slice(data);
            }
        }
        buf
    }

    /// Decode bytecode to instruction list (for debugging).
    pub fn decode(bytecode: &[u8]) -> Result<Vec<(RpnOp, Vec<u8>)>, RpnError> {
        let mut result = Vec::new();
        let mut pc = 0;

        while pc < bytecode.len() {
            let op_byte = bytecode[pc];
            let op = RpnOp::from_u8(op_byte).ok_or(RpnError::InvalidOpcode(op_byte))?;
            let payload_size = op.payload_size();
            if pc + 1 + payload_size > bytecode.len() {
                return Err(RpnError::TruncatedBytecode);
            }
            let payload = bytecode[pc + 1..pc + 1 + payload_size].to_vec();
            result.push((op, payload));
            pc += 1 + payload_size;
        }

        Ok(result)
    }

    /// Disassemble bytecode to human-readable string.
    pub fn disassemble(bytecode: &[u8]) -> Result<String, RpnError> {
        let instructions = Self::decode(bytecode)?;
        let mut output = String::new();
        let mut pc = 0;
        for (op, payload) in &instructions {
            let line = match op {
                RpnOp::Push32 => {
                    let val = u32::from_le_bytes(payload[..4].try_into().unwrap());
                    format!("{:04x}: Push32 {}", pc, val)
                }
                RpnOp::Push64 => {
                    let val = u64::from_le_bytes(payload[..8].try_into().unwrap());
                    format!("{:04x}: Push64 {}", pc, val)
                }
                RpnOp::Jmp => {
                    let target = u64::from_le_bytes(payload[..8].try_into().unwrap());
                    format!("{:04x}: Jmp -> {:04x}", pc, target)
                }
                RpnOp::JmpIf => {
                    let target = u64::from_le_bytes(payload[..8].try_into().unwrap());
                    format!("{:04x}: JmpIf -> {:04x}", pc, target)
                }
                RpnOp::UiSetColor => format!("{:04x}: UiSetColor", pc),
                RpnOp::UiBox => format!("{:04x}: UiBox", pc),
                RpnOp::UiSlab => format!("{:04x}: UiSlab", pc),
                RpnOp::UiCircle => format!("{:04x}: UiCircle", pc),
                RpnOp::UiText => format!("{:04x}: UiText", pc),
                RpnOp::UiPushState => format!("{:04x}: UiPushState", pc),
                RpnOp::UiPopState => format!("{:04x}: UiPopState", pc),
                RpnOp::UiSetOffset => format!("{:04x}: UiSetOffset", pc),
                RpnOp::UiLine => format!("{:04x}: UiLine", pc),
                _ => format!("{:04x}: {:?}", pc, op),
            };
            output.push_str(&line);
            output.push('\n');
            pc += 1 + op.payload_size();
        }
        Ok(output)
    }
}

impl Default for RpnVm {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for RpnVm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RpnVm(pc={}, stack={}, gas={}/{})",
            self.pc,
            self.stack.len(),
            self.trace.gas_consumed,
            self.gas.gas_budget
        )
    }
}
