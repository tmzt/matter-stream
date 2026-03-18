//! Modified RPN stack language VM (MTSM-RPN-Bincode).
//!
//! Features:
//! - Per-opcode gas metering with configurable budgets
//! - Backward-jump detection and loop limiting
//! - Execution trace/profiling
//! - Control flow: Jmp, JmpIf, Halt, comparisons
//! - Bitwise ops, typed bank access, event polling
//! - UI draw opcodes (0x40–0x49) for 2D rendering
//! - Persistent typed register banks (Tier 1/2 memory)

use matterstream_vm_addressing::fqa::Fqa;
use matterstream_vm_addressing::ova::Ova;
use matterstream_vm_arena::TripleArena;

use crate::event::{VmEvent, VmEventType};
use crate::ui_vm::{
    UiDrawCmd, UiDrawState, UI_DRAW_CMD_MAX, UI_STATE_STACK_MAX,
    VqlOutput, VqlField, VQL_OUTPUT_MAX,
    SkillDef, SkillStep, SkillReplaceable, LlmUseCase, CronSpec, SKILL_OUTPUT_MAX,
    FOURCC_MTUI,
};
use std::collections::HashMap;
use std::collections::VecDeque;
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
    // Bitwise opcodes (v0.3.0)
    And = 0x1A,
    Or = 0x1B,
    Xor = 0x1C,
    Shl = 0x1D,
    Shr = 0x1E,
    Not = 0x1F,
    // Typed bank access (v0.3.0)
    LoadBank = 0x20,
    StoreBank = 0x21,
    // Extended comparisons (v0.3.0)
    CmpGe = 0x22,
    CmpLe = 0x23,
    CmpNe = 0x24,
    // ZeroPage i32 load/store (v0.4.0)
    LoadZpI32 = 0x25,
    StoreZpI32 = 0x26,
    // Component-aware bank access (v0.4.0)
    LoadBankComp = 0x27,
    StoreBankComp = 0x28,
    // Float arithmetic opcodes (v0.4.0)
    FAdd = 0x30,
    FSub = 0x31,
    FMul = 0x32,
    FDiv = 0x33,
    FCmpGt = 0x34,
    FCmpLt = 0x35,
    FCmpEq = 0x36,
    FNeg = 0x37,
    FAbs = 0x38,
    I2F = 0x39,
    F2I = 0x3A,
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
    // Extended UI (v0.3.0)
    UiTextStr = 0x49,
    UiAction = 0x4A,
    // Event & runtime opcodes (v0.3.0)
    EvPoll = 0x50,
    EvHasEvent = 0x51,
    FrameCount = 0x52,
    Rand = 0x53,
    // Control register (v0.5.0)
    SetCR = 0x60,
    // VQL0 — Vesicle Query Language (v0.5.0)
    VqlBeginQuery = 0x61,
    VqlEndQuery = 0x62,
    VqlBind = 0x63,
    VqlSetField = 0x64,
    VqlSetFieldStr = 0x65,
    VqlFilter = 0x66,
    VqlProject = 0x67,
    VqlParam = 0x68,
    // SKLL — Skill / invocable logic (v0.5.0)
    SkillBegin = 0x70,
    SkillEnd = 0x71,
    SkillStep = 0x72,
    SkillLlmStep = 0x73,
    SkillReplaceable = 0x74,
    SkillInvoke = 0x75,
    SkillInvokeSymbol = 0x76,
    SkillLlmModel = 0x77,
    SkillLlmUseCase = 0x78,
    SkillSetShortDesc = 0x79,
    SkillSetLongDesc = 0x7A,
    SkillCronInterval = 0x7B,
    SkillCronJitter = 0x7C,
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
            0x1A => Some(RpnOp::And),
            0x1B => Some(RpnOp::Or),
            0x1C => Some(RpnOp::Xor),
            0x1D => Some(RpnOp::Shl),
            0x1E => Some(RpnOp::Shr),
            0x1F => Some(RpnOp::Not),
            0x20 => Some(RpnOp::LoadBank),
            0x21 => Some(RpnOp::StoreBank),
            0x22 => Some(RpnOp::CmpGe),
            0x23 => Some(RpnOp::CmpLe),
            0x24 => Some(RpnOp::CmpNe),
            0x25 => Some(RpnOp::LoadZpI32),
            0x26 => Some(RpnOp::StoreZpI32),
            0x27 => Some(RpnOp::LoadBankComp),
            0x28 => Some(RpnOp::StoreBankComp),
            0x30 => Some(RpnOp::FAdd),
            0x31 => Some(RpnOp::FSub),
            0x32 => Some(RpnOp::FMul),
            0x33 => Some(RpnOp::FDiv),
            0x34 => Some(RpnOp::FCmpGt),
            0x35 => Some(RpnOp::FCmpLt),
            0x36 => Some(RpnOp::FCmpEq),
            0x37 => Some(RpnOp::FNeg),
            0x38 => Some(RpnOp::FAbs),
            0x39 => Some(RpnOp::I2F),
            0x3A => Some(RpnOp::F2I),
            0x40 => Some(RpnOp::UiSetColor),
            0x41 => Some(RpnOp::UiBox),
            0x42 => Some(RpnOp::UiSlab),
            0x43 => Some(RpnOp::UiCircle),
            0x44 => Some(RpnOp::UiText),
            0x45 => Some(RpnOp::UiPushState),
            0x46 => Some(RpnOp::UiPopState),
            0x47 => Some(RpnOp::UiSetOffset),
            0x48 => Some(RpnOp::UiLine),
            0x49 => Some(RpnOp::UiTextStr),
            0x4A => Some(RpnOp::UiAction),
            0x50 => Some(RpnOp::EvPoll),
            0x51 => Some(RpnOp::EvHasEvent),
            0x52 => Some(RpnOp::FrameCount),
            0x53 => Some(RpnOp::Rand),
            0x60 => Some(RpnOp::SetCR),
            0x61 => Some(RpnOp::VqlBeginQuery),
            0x62 => Some(RpnOp::VqlEndQuery),
            0x63 => Some(RpnOp::VqlBind),
            0x64 => Some(RpnOp::VqlSetField),
            0x65 => Some(RpnOp::VqlSetFieldStr),
            0x66 => Some(RpnOp::VqlFilter),
            0x67 => Some(RpnOp::VqlProject),
            0x68 => Some(RpnOp::VqlParam),
            0x70 => Some(RpnOp::SkillBegin),
            0x71 => Some(RpnOp::SkillEnd),
            0x72 => Some(RpnOp::SkillStep),
            0x73 => Some(RpnOp::SkillLlmStep),
            0x74 => Some(RpnOp::SkillReplaceable),
            0x75 => Some(RpnOp::SkillInvoke),
            0x76 => Some(RpnOp::SkillInvokeSymbol),
            0x77 => Some(RpnOp::SkillLlmModel),
            0x78 => Some(RpnOp::SkillLlmUseCase),
            0x79 => Some(RpnOp::SkillSetShortDesc),
            0x7A => Some(RpnOp::SkillSetLongDesc),
            0x7B => Some(RpnOp::SkillCronInterval),
            0x7C => Some(RpnOp::SkillCronJitter),
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
    pub cost_bitwise: u64,
    pub cost_bank: u64,
    pub cost_event: u64,
    pub cost_cr: u64,
    pub cost_vql: u64,
    pub cost_skill: u64,
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
            cost_bitwise: 2,
            cost_bank: 3,
            cost_event: 5,
            cost_cr: 2,
            cost_vql: 5,
            cost_skill: 5,
        }
    }

    /// Gas cost for a given opcode.
    pub fn cost_of(&self, op: RpnOp) -> u64 {
        match op {
            RpnOp::Nop | RpnOp::Halt => self.cost_nop,
            RpnOp::Push32 | RpnOp::Push64 | RpnOp::PushFqa => self.cost_push,
            RpnOp::Dup | RpnOp::Drop | RpnOp::Swap => self.cost_stack_op,
            RpnOp::Add | RpnOp::Sub | RpnOp::Mul | RpnOp::Div | RpnOp::Mod
            | RpnOp::FAdd | RpnOp::FSub | RpnOp::FMul | RpnOp::FDiv
            | RpnOp::FNeg | RpnOp::FAbs | RpnOp::I2F | RpnOp::F2I => {
                self.cost_arithmetic
            }
            RpnOp::FCmpGt | RpnOp::FCmpLt | RpnOp::FCmpEq => self.cost_compare,
            RpnOp::Load | RpnOp::Store => self.cost_memory,
            RpnOp::Call | RpnOp::Ret => self.cost_call,
            RpnOp::Sync => self.cost_sync,
            RpnOp::MapNew | RpnOp::MapSet | RpnOp::MapGet => self.cost_map,
            RpnOp::Jmp | RpnOp::JmpIf => self.cost_jump,
            RpnOp::CmpEq | RpnOp::CmpLt | RpnOp::CmpGt | RpnOp::CmpGe | RpnOp::CmpLe
            | RpnOp::CmpNe => self.cost_compare,
            RpnOp::And | RpnOp::Or | RpnOp::Xor | RpnOp::Shl | RpnOp::Shr | RpnOp::Not => {
                self.cost_bitwise
            }
            RpnOp::LoadBank | RpnOp::StoreBank
            | RpnOp::LoadZpI32 | RpnOp::StoreZpI32
            | RpnOp::LoadBankComp | RpnOp::StoreBankComp => self.cost_bank,
            RpnOp::EvPoll | RpnOp::EvHasEvent | RpnOp::FrameCount | RpnOp::Rand => {
                self.cost_event
            }
            RpnOp::UiSetColor
            | RpnOp::UiBox
            | RpnOp::UiSlab
            | RpnOp::UiCircle
            | RpnOp::UiText
            | RpnOp::UiPushState
            | RpnOp::UiPopState
            | RpnOp::UiSetOffset
            | RpnOp::UiLine
            | RpnOp::UiTextStr
            | RpnOp::UiAction => self.cost_ui,
            RpnOp::SetCR => self.cost_cr,
            RpnOp::VqlBeginQuery
            | RpnOp::VqlEndQuery
            | RpnOp::VqlBind
            | RpnOp::VqlSetField
            | RpnOp::VqlSetFieldStr
            | RpnOp::VqlFilter
            | RpnOp::VqlProject
            | RpnOp::VqlParam => self.cost_vql,
            RpnOp::SkillBegin
            | RpnOp::SkillEnd
            | RpnOp::SkillStep
            | RpnOp::SkillLlmStep
            | RpnOp::SkillReplaceable
            | RpnOp::SkillInvoke
            | RpnOp::SkillInvokeSymbol
            | RpnOp::SkillLlmModel
            | RpnOp::SkillLlmUseCase
            | RpnOp::SkillSetShortDesc
            | RpnOp::SkillSetLongDesc
            | RpnOp::SkillCronInterval
            | RpnOp::SkillCronJitter => self.cost_skill,
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
    InvalidBankId(u32),
    InvalidBankSlot { bank: u32, slot: u32 },
    InvalidCRIndex(u32),
    VqlLimitExceeded,
    VqlNoActiveQuery,
    SkillLimitExceeded,
    SkillNoActiveDef,
    SkillNoActiveLlmStep,
    InvalidStringIndex(u32),
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
                write!(
                    f,
                    "RPN backward jump limit exceeded: {} jumps (limit {})",
                    count, limit
                )
            }
            RpnError::InvalidJumpTarget(target) => {
                write!(f, "RPN invalid jump target: {}", target)
            }
            RpnError::UiStateStackOverflow => write!(f, "RPN UI state stack overflow"),
            RpnError::UiStateStackUnderflow => write!(f, "RPN UI state stack underflow"),
            RpnError::UiDrawLimitExceeded => write!(f, "RPN UI draw limit exceeded"),
            RpnError::InvalidBankId(id) => write!(f, "RPN invalid bank id: {}", id),
            RpnError::InvalidBankSlot { bank, slot } => {
                write!(f, "RPN invalid bank slot: bank={}, slot={}", bank, slot)
            }
            RpnError::InvalidCRIndex(idx) => write!(f, "RPN invalid CR index: {}", idx),
            RpnError::VqlLimitExceeded => write!(f, "RPN VQL output limit exceeded"),
            RpnError::VqlNoActiveQuery => write!(f, "RPN VQL no active query"),
            RpnError::SkillLimitExceeded => write!(f, "RPN SKLL output limit exceeded"),
            RpnError::SkillNoActiveDef => write!(f, "RPN SKLL no active skill definition"),
            RpnError::SkillNoActiveLlmStep => write!(f, "RPN SKLL no active LLM step"),
            RpnError::InvalidStringIndex(idx) => write!(f, "RPN invalid string index: {}", idx),
        }
    }
}

/// Simple xorshift64 RNG for deterministic random numbers in the VM.
#[derive(Debug, Clone)]
pub struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state as u32
    }

    /// Generate a random u32 in [0, max). Returns 0 if max is 0.
    pub fn next_bounded(&mut self, max: u32) -> u32 {
        if max == 0 {
            return 0;
        }
        self.next_u32() % max
    }
}

/// Bank IDs for typed register banks (matching Tier 1/2 memory layout).
pub const BANK_SCALAR: u32 = 0;
pub const BANK_INT: u32 = 1;
pub const BANK_VEC3: u32 = 2;
pub const BANK_VEC4: u32 = 3;
pub const BANK_ZERO_PAGE: u32 = 4;

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
    // Typed register banks (Tier 1) — persist between execute() calls
    pub scalar_bank: [f32; 16],
    pub int_bank: [i32; 16],
    pub vec3_bank: [[f32; 3]; 16],
    pub vec4_bank: [[f32; 4]; 16],
    // ZeroPage (Tier 2) — 256 bytes for grids/arrays
    pub zero_page: [u8; 256],
    // String table for UiTextStr
    pub string_table: Vec<String>,
    // Event queue
    pub event_queue: VecDeque<VmEvent>,
    // Frame counter
    pub frame_count: u64,
    // RNG
    pub rng: SimpleRng,
    // Control register bank (8 registers, CR0 = output mode FourCC)
    pub cr_bank: [u32; 8],
    // VQL state
    pub vql_outputs: Vec<VqlOutput>,
    vql_active: Option<VqlOutput>,
    // SKLL state
    pub skill_outputs: Vec<SkillDef>,
    skill_active: Option<SkillDef>,
    /// Temporary storage for replaceables being added to the current LLM step
    skill_active_llm_prompt: Option<String>,
    skill_active_llm_replaceables: Vec<SkillReplaceable>,
    skill_active_llm_model: Option<String>,
    skill_active_llm_use_case: LlmUseCase,
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
            scalar_bank: [0.0; 16],
            int_bank: [0; 16],
            vec3_bank: [[0.0; 3]; 16],
            vec4_bank: [[0.0; 4]; 16],
            zero_page: [0; 256],
            string_table: Vec::new(),
            event_queue: VecDeque::new(),
            frame_count: 0,
            rng: SimpleRng::new(0xDEAD_BEEF),
            cr_bank: [FOURCC_MTUI, 0, 0, 0, 0, 0, 0, 0],
            vql_outputs: Vec::new(),
            vql_active: None,
            skill_outputs: Vec::new(),
            skill_active: None,
            skill_active_llm_prompt: None,
            skill_active_llm_replaceables: Vec::new(),
            skill_active_llm_model: None,
            skill_active_llm_use_case: LlmUseCase::General,
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

    fn resolve_str(&self, idx: u32) -> Result<String, RpnError> {
        self.string_table
            .get(idx as usize)
            .cloned()
            .ok_or(RpnError::InvalidStringIndex(idx))
    }

    /// Flush the pending LLM step (prompt + replaceables + model + use_case) into the active skill.
    fn flush_llm_step(&mut self) -> Result<(), RpnError> {
        if let Some(prompt) = self.skill_active_llm_prompt.take() {
            let skill = self.skill_active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
            skill.steps.push(SkillStep::Llm {
                prompt,
                replaceables: std::mem::take(&mut self.skill_active_llm_replaceables),
                model: self.skill_active_llm_model.take(),
                use_case: self.skill_active_llm_use_case,
            });
            self.skill_active_llm_use_case = LlmUseCase::General;
        }
        Ok(())
    }

    fn read_u8(bytecode: &[u8], pos: usize) -> Result<u8, RpnError> {
        bytecode
            .get(pos)
            .copied()
            .ok_or(RpnError::TruncatedBytecode)
    }

    fn read_u32(bytecode: &[u8], pos: usize) -> Result<u32, RpnError> {
        if pos + 4 > bytecode.len() {
            return Err(RpnError::TruncatedBytecode);
        }
        Ok(u32::from_le_bytes(
            bytecode[pos..pos + 4].try_into().unwrap(),
        ))
    }

    fn read_u64(bytecode: &[u8], pos: usize) -> Result<u64, RpnError> {
        if pos + 8 > bytecode.len() {
            return Err(RpnError::TruncatedBytecode);
        }
        Ok(u64::from_le_bytes(
            bytecode[pos..pos + 8].try_into().unwrap(),
        ))
    }

    fn read_u128(bytecode: &[u8], pos: usize) -> Result<u128, RpnError> {
        if pos + 16 > bytecode.len() {
            return Err(RpnError::TruncatedBytecode);
        }
        Ok(u128::from_le_bytes(
            bytecode[pos..pos + 16].try_into().unwrap(),
        ))
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

    /// Load a value from a typed bank.
    fn load_bank(&self, bank: u32, slot: u32) -> Result<u64, RpnError> {
        match bank {
            BANK_SCALAR => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                Ok(f32::to_bits(self.scalar_bank[slot as usize]) as u64)
            }
            BANK_INT => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                Ok(self.int_bank[slot as usize] as u32 as u64)
            }
            BANK_VEC3 => {
                // Return packed: just the first component as a float → u32
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                Ok(f32::to_bits(self.vec3_bank[slot as usize][0]) as u64)
            }
            BANK_VEC4 => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                Ok(f32::to_bits(self.vec4_bank[slot as usize][0]) as u64)
            }
            BANK_ZERO_PAGE => {
                if slot >= 256 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                Ok(self.zero_page[slot as usize] as u64)
            }
            _ => Err(RpnError::InvalidBankId(bank)),
        }
    }

    /// Load a specific component from a vec3/vec4 bank.
    fn load_bank_comp(&self, bank: u32, slot: u32, comp: u32) -> Result<u64, RpnError> {
        match bank {
            BANK_VEC3 => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                if comp >= 3 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                Ok(f32::to_bits(self.vec3_bank[slot as usize][comp as usize]) as u64)
            }
            BANK_VEC4 => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                if comp >= 4 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                Ok(f32::to_bits(self.vec4_bank[slot as usize][comp as usize]) as u64)
            }
            BANK_SCALAR => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                Ok(f32::to_bits(self.scalar_bank[slot as usize]) as u64)
            }
            BANK_INT => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                Ok(self.int_bank[slot as usize] as u32 as u64)
            }
            BANK_ZERO_PAGE => {
                if slot >= 256 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                Ok(self.zero_page[slot as usize] as u64)
            }
            _ => Err(RpnError::InvalidBankId(bank)),
        }
    }

    /// Store a value to a specific component of a vec3/vec4 bank.
    fn store_bank_comp(&mut self, value: u64, bank: u32, slot: u32, comp: u32) -> Result<(), RpnError> {
        match bank {
            BANK_VEC3 => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                if comp >= 3 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                self.vec3_bank[slot as usize][comp as usize] = f32::from_bits(value as u32);
                Ok(())
            }
            BANK_VEC4 => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                if comp >= 4 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                self.vec4_bank[slot as usize][comp as usize] = f32::from_bits(value as u32);
                Ok(())
            }
            BANK_SCALAR => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                self.scalar_bank[slot as usize] = f32::from_bits(value as u32);
                Ok(())
            }
            BANK_INT => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                self.int_bank[slot as usize] = value as i32;
                Ok(())
            }
            BANK_ZERO_PAGE => {
                if slot >= 256 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                self.zero_page[slot as usize] = value as u8;
                Ok(())
            }
            _ => Err(RpnError::InvalidBankId(bank)),
        }
    }

    /// Store a value to a typed bank.
    fn store_bank(&mut self, value: u64, bank: u32, slot: u32) -> Result<(), RpnError> {
        match bank {
            BANK_SCALAR => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                self.scalar_bank[slot as usize] = f32::from_bits(value as u32);
                Ok(())
            }
            BANK_INT => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                self.int_bank[slot as usize] = value as i32;
                Ok(())
            }
            BANK_VEC3 => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                self.vec3_bank[slot as usize][0] = f32::from_bits(value as u32);
                Ok(())
            }
            BANK_VEC4 => {
                if slot >= 16 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                self.vec4_bank[slot as usize][0] = f32::from_bits(value as u32);
                Ok(())
            }
            BANK_ZERO_PAGE => {
                if slot >= 256 {
                    return Err(RpnError::InvalidBankSlot { bank, slot });
                }
                self.zero_page[slot as usize] = value as u8;
                Ok(())
            }
            _ => Err(RpnError::InvalidBankId(bank)),
        }
    }

    /// Execute bytecode against arenas. Returns when bytecode is exhausted or Halt.
    /// Note: typed banks persist between calls — they are NOT cleared.
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
        // Stack is cleared, but banks persist
        self.stack.clear();
        self.call_stack.clear();
        // Reset control registers and extension outputs
        self.cr_bank = [FOURCC_MTUI, 0, 0, 0, 0, 0, 0, 0];
        self.vql_outputs.clear();
        self.vql_active = None;
        self.skill_outputs.clear();
        self.skill_active = None;
        self.skill_active_llm_prompt = None;
        self.skill_active_llm_replaceables.clear();
        self.skill_active_llm_model = None;
        self.skill_active_llm_use_case = LlmUseCase::General;
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
            RpnOp::Jmp => {
                let target = Self::read_u64(bytecode, self.pc + 1)? as usize;
                if target > bytecode.len() {
                    return Err(RpnError::InvalidJumpTarget(target as u64));
                }
                let from_pc = self.pc;
                self.pc = target;
                self.track_jump(from_pc, target)?;
            }
            RpnOp::JmpIf => {
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
                    self.pc += 9;
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
            RpnOp::CmpGe => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(if a >= b { 1 } else { 0 }))?;
                self.pc += 1;
            }
            RpnOp::CmpLe => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(if a <= b { 1 } else { 0 }))?;
                self.pc += 1;
            }
            RpnOp::CmpNe => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(if a != b { 1 } else { 0 }))?;
                self.pc += 1;
            }
            // --- Bitwise opcodes ---
            RpnOp::And => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a & b))?;
                self.pc += 1;
            }
            RpnOp::Or => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a | b))?;
                self.pc += 1;
            }
            RpnOp::Xor => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a ^ b))?;
                self.pc += 1;
            }
            RpnOp::Shl => {
                let n = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a.wrapping_shl(n as u32)))?;
                self.pc += 1;
            }
            RpnOp::Shr => {
                let n = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a.wrapping_shr(n as u32)))?;
                self.pc += 1;
            }
            RpnOp::Not => {
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(if a == 0 { 1 } else { 0 }))?;
                self.pc += 1;
            }
            // --- Typed bank access ---
            RpnOp::LoadBank => {
                let slot = self.pop_u32_coerce()?;
                let bank = self.pop_u32_coerce()?;
                let value = self.load_bank(bank, slot)?;
                self.push(RpnValue::U64(value))?;
                self.pc += 1;
            }
            RpnOp::StoreBank => {
                let slot = self.pop_u32_coerce()?;
                let bank = self.pop_u32_coerce()?;
                let value = self.pop_u64()?;
                self.store_bank(value, bank, slot)?;
                self.pc += 1;
            }
            // --- ZeroPage i32 load/store ---
            RpnOp::LoadZpI32 => {
                let addr = self.pop_u32_coerce()? as usize;
                if addr + 3 >= 256 {
                    return Err(RpnError::InvalidBankSlot { bank: BANK_ZERO_PAGE, slot: addr as u32 });
                }
                let val = i32::from_le_bytes([
                    self.zero_page[addr],
                    self.zero_page[addr + 1],
                    self.zero_page[addr + 2],
                    self.zero_page[addr + 3],
                ]);
                self.push(RpnValue::U64(val as u32 as u64))?;
                self.pc += 1;
            }
            RpnOp::StoreZpI32 => {
                let addr = self.pop_u32_coerce()? as usize;
                let value = self.pop_u32_coerce()? as i32;
                if addr + 3 >= 256 {
                    return Err(RpnError::InvalidBankSlot { bank: BANK_ZERO_PAGE, slot: addr as u32 });
                }
                let bytes = value.to_le_bytes();
                self.zero_page[addr] = bytes[0];
                self.zero_page[addr + 1] = bytes[1];
                self.zero_page[addr + 2] = bytes[2];
                self.zero_page[addr + 3] = bytes[3];
                self.pc += 1;
            }
            // --- Component-aware bank access ---
            RpnOp::LoadBankComp => {
                let comp = self.pop_u32_coerce()?;
                let slot = self.pop_u32_coerce()?;
                let bank = self.pop_u32_coerce()?;
                let value = self.load_bank_comp(bank, slot, comp)?;
                self.push(RpnValue::U64(value))?;
                self.pc += 1;
            }
            RpnOp::StoreBankComp => {
                let comp = self.pop_u32_coerce()?;
                let slot = self.pop_u32_coerce()?;
                let bank = self.pop_u32_coerce()?;
                let value = self.pop_u64()?;
                self.store_bank_comp(value, bank, slot, comp)?;
                self.pc += 1;
            }
            // --- Float arithmetic opcodes ---
            RpnOp::FAdd => {
                let b_bits = self.pop_u64()? as u32;
                let a_bits = self.pop_u64()? as u32;
                let result = f32::from_bits(a_bits) + f32::from_bits(b_bits);
                self.push(RpnValue::U64(f32::to_bits(result) as u64))?;
                self.pc += 1;
            }
            RpnOp::FSub => {
                let b_bits = self.pop_u64()? as u32;
                let a_bits = self.pop_u64()? as u32;
                let result = f32::from_bits(a_bits) - f32::from_bits(b_bits);
                self.push(RpnValue::U64(f32::to_bits(result) as u64))?;
                self.pc += 1;
            }
            RpnOp::FMul => {
                let b_bits = self.pop_u64()? as u32;
                let a_bits = self.pop_u64()? as u32;
                let result = f32::from_bits(a_bits) * f32::from_bits(b_bits);
                self.push(RpnValue::U64(f32::to_bits(result) as u64))?;
                self.pc += 1;
            }
            RpnOp::FDiv => {
                let b_bits = self.pop_u64()? as u32;
                let a_bits = self.pop_u64()? as u32;
                let b = f32::from_bits(b_bits);
                if b == 0.0 {
                    return Err(RpnError::DivisionByZero);
                }
                let result = f32::from_bits(a_bits) / b;
                self.push(RpnValue::U64(f32::to_bits(result) as u64))?;
                self.pc += 1;
            }
            RpnOp::FCmpGt => {
                let b_bits = self.pop_u64()? as u32;
                let a_bits = self.pop_u64()? as u32;
                let result = if f32::from_bits(a_bits) > f32::from_bits(b_bits) { 1u64 } else { 0 };
                self.push(RpnValue::U64(result))?;
                self.pc += 1;
            }
            RpnOp::FCmpLt => {
                let b_bits = self.pop_u64()? as u32;
                let a_bits = self.pop_u64()? as u32;
                let result = if f32::from_bits(a_bits) < f32::from_bits(b_bits) { 1u64 } else { 0 };
                self.push(RpnValue::U64(result))?;
                self.pc += 1;
            }
            RpnOp::FCmpEq => {
                let b_bits = self.pop_u64()? as u32;
                let a_bits = self.pop_u64()? as u32;
                let result = if f32::from_bits(a_bits) == f32::from_bits(b_bits) { 1u64 } else { 0 };
                self.push(RpnValue::U64(result))?;
                self.pc += 1;
            }
            RpnOp::FNeg => {
                let a_bits = self.pop_u64()? as u32;
                let result = -f32::from_bits(a_bits);
                self.push(RpnValue::U64(f32::to_bits(result) as u64))?;
                self.pc += 1;
            }
            RpnOp::FAbs => {
                let a_bits = self.pop_u64()? as u32;
                let result = f32::from_bits(a_bits).abs();
                self.push(RpnValue::U64(f32::to_bits(result) as u64))?;
                self.pc += 1;
            }
            RpnOp::I2F => {
                let a = self.pop_u64()? as i32;
                let result = a as f32;
                self.push(RpnValue::U64(f32::to_bits(result) as u64))?;
                self.pc += 1;
            }
            RpnOp::F2I => {
                let a_bits = self.pop_u64()? as u32;
                let result = f32::from_bits(a_bits) as i32;
                self.push(RpnValue::U64(result as u32 as u64))?;
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
                    x,
                    y,
                    w,
                    h,
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
                    x,
                    y,
                    w,
                    h,
                    radius,
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
                    x,
                    y,
                    r,
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
                    x,
                    y,
                    size,
                    slot,
                    color: self.ui_state.color,
                });
                self.pc += 1;
            }
            RpnOp::UiTextStr => {
                if self.ui_draws.len() >= UI_DRAW_CMD_MAX {
                    return Err(RpnError::UiDrawLimitExceeded);
                }
                let str_idx = self.pop_u32_coerce()?;
                let size = self.pop_u32_coerce()?;
                let y = self.pop_u32_coerce()? as i32 + self.ui_state.offset_y;
                let x = self.pop_u32_coerce()? as i32 + self.ui_state.offset_x;
                self.ui_draws.push(UiDrawCmd::TextStr {
                    x,
                    y,
                    size,
                    str_idx,
                    color: self.ui_state.color,
                });
                self.pc += 1;
            }
            RpnOp::UiAction => {
                if self.ui_draws.len() >= UI_DRAW_CMD_MAX {
                    return Err(RpnError::UiDrawLimitExceeded);
                }
                let str_idx = self.pop_u32_coerce()?;
                let h = self.pop_u32_coerce()?;
                let w = self.pop_u32_coerce()?;
                let y = self.pop_u32_coerce()? as i32 + self.ui_state.offset_y;
                let x = self.pop_u32_coerce()? as i32 + self.ui_state.offset_x;
                self.ui_draws.push(UiDrawCmd::Action {
                    x,
                    y,
                    w,
                    h,
                    str_idx,
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
                self.ui_state = self
                    .ui_state_stack
                    .pop()
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
                    x1,
                    y1,
                    x2,
                    y2,
                    color: self.ui_state.color,
                });
                self.pc += 1;
            }
            // --- Event & runtime opcodes ---
            RpnOp::EvPoll => {
                if let Some(ev) = self.event_queue.pop_front() {
                    self.push(RpnValue::U64(ev.data))?;
                    self.push(RpnValue::U32(ev.etype as u32))?;
                } else {
                    self.push(RpnValue::U64(0))?;
                    self.push(RpnValue::U32(VmEventType::None as u32))?;
                }
                self.pc += 1;
            }
            RpnOp::EvHasEvent => {
                let flag = if self.event_queue.is_empty() { 0u64 } else { 1 };
                self.push(RpnValue::U64(flag))?;
                self.pc += 1;
            }
            RpnOp::FrameCount => {
                self.push(RpnValue::U64(self.frame_count))?;
                self.pc += 1;
            }
            RpnOp::Rand => {
                let max = self.pop_u32_coerce()?;
                let value = self.rng.next_bounded(max);
                self.push(RpnValue::U32(value))?;
                self.pc += 1;
            }
            // --- Control register ---
            RpnOp::SetCR => {
                let value = self.pop_u32_coerce()?;
                let cr_idx = self.pop_u32_coerce()?;
                if cr_idx as usize >= self.cr_bank.len() {
                    return Err(RpnError::InvalidCRIndex(cr_idx));
                }
                self.cr_bank[cr_idx as usize] = value;
                self.pc += 1;
            }
            // --- VQL0 opcodes ---
            RpnOp::VqlBeginQuery => {
                if self.vql_outputs.len() >= VQL_OUTPUT_MAX {
                    return Err(RpnError::VqlLimitExceeded);
                }
                self.vql_active = Some(VqlOutput::new());
                self.pc += 1;
            }
            RpnOp::VqlEndQuery => {
                let query = self.vql_active.take().ok_or(RpnError::VqlNoActiveQuery)?;
                self.vql_outputs.push(query);
                self.pc += 1;
            }
            RpnOp::VqlBind => {
                let val_idx = self.pop_u32_coerce()?;
                let key_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(key_idx)?;
                let value = self.resolve_str(val_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::Bind { name, value });
                self.pc += 1;
            }
            RpnOp::VqlSetField => {
                let value = self.pop_u64()?;
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::FieldValue { name, value });
                self.pc += 1;
            }
            RpnOp::VqlSetFieldStr => {
                let val_idx = self.pop_u32_coerce()?;
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let value = self.resolve_str(val_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::FieldStr { name, value });
                self.pc += 1;
            }
            RpnOp::VqlFilter => {
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::Filter(name));
                self.pc += 1;
            }
            RpnOp::VqlProject => {
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::Project(name));
                self.pc += 1;
            }
            RpnOp::VqlParam => {
                let val_idx = self.pop_u32_coerce()?;
                let key_idx = self.pop_u32_coerce()?;
                let key = self.resolve_str(key_idx)?;
                let value = self.resolve_str(val_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::Param { key, value });
                self.pc += 1;
            }
            // --- SKLL opcodes ---
            RpnOp::SkillBegin => {
                if self.skill_outputs.len() >= SKILL_OUTPUT_MAX {
                    return Err(RpnError::SkillLimitExceeded);
                }
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                self.skill_active = Some(SkillDef::new(name));
                self.pc += 1;
            }
            RpnOp::SkillEnd => {
                // Flush any pending LLM step
                self.flush_llm_step()?;
                let skill = self.skill_active.take().ok_or(RpnError::SkillNoActiveDef)?;
                self.skill_outputs.push(skill);
                self.pc += 1;
            }
            RpnOp::SkillStep => {
                // Flush any pending LLM step first
                self.flush_llm_step()?;
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let skill = self.skill_active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.steps.push(SkillStep::Deterministic { name });
                self.pc += 1;
            }
            RpnOp::SkillLlmStep => {
                // Flush any previous pending LLM step
                self.flush_llm_step()?;
                let prompt_idx = self.pop_u32_coerce()?;
                let prompt = self.resolve_str(prompt_idx)?;
                self.skill_active_llm_prompt = Some(prompt);
                self.pc += 1;
            }
            RpnOp::SkillReplaceable => {
                let default_idx = self.pop_u32_coerce()?;
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let default = self.resolve_str(default_idx)?;
                if self.skill_active_llm_prompt.is_none() {
                    return Err(RpnError::SkillNoActiveLlmStep);
                }
                self.skill_active_llm_replaceables.push(SkillReplaceable { name, default });
                self.pc += 1;
            }
            RpnOp::SkillLlmModel => {
                let model_idx = self.pop_u32_coerce()?;
                let model = self.resolve_str(model_idx)?;
                if self.skill_active_llm_prompt.is_none() {
                    return Err(RpnError::SkillNoActiveLlmStep);
                }
                self.skill_active_llm_model = Some(model);
                self.pc += 1;
            }
            RpnOp::SkillLlmUseCase => {
                let use_case_val = self.pop_u32_coerce()? as u8;
                let use_case = LlmUseCase::from_u8(use_case_val)
                    .unwrap_or(LlmUseCase::General);
                if self.skill_active_llm_prompt.is_none() {
                    return Err(RpnError::SkillNoActiveLlmStep);
                }
                self.skill_active_llm_use_case = use_case;
                self.pc += 1;
            }
            RpnOp::SkillSetShortDesc => {
                let str_idx = self.pop_u32_coerce()?;
                let desc = self.resolve_str(str_idx)?;
                let skill = self.skill_active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.short_description = desc;
                self.pc += 1;
            }
            RpnOp::SkillSetLongDesc => {
                let str_idx = self.pop_u32_coerce()?;
                let desc = self.resolve_str(str_idx)?;
                let skill = self.skill_active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.long_description = desc;
                self.pc += 1;
            }
            RpnOp::SkillCronInterval => {
                let hi = self.pop_u32_coerce()? as u64;
                let lo = self.pop_u32_coerce()? as u64;
                let interval_ms = (hi << 32) | lo;
                let skill = self.skill_active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                match &mut skill.cron {
                    Some(spec) => spec.interval_ms = interval_ms,
                    None => skill.cron = Some(CronSpec { interval_ms, jitter_ms: 0 }),
                }
                self.pc += 1;
            }
            RpnOp::SkillCronJitter => {
                let hi = self.pop_u32_coerce()? as u64;
                let lo = self.pop_u32_coerce()? as u64;
                let jitter_ms = (hi << 32) | lo;
                let skill = self.skill_active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                match &mut skill.cron {
                    Some(spec) => spec.jitter_ms = jitter_ms,
                    None => skill.cron = Some(CronSpec { interval_ms: 0, jitter_ms }),
                }
                self.pc += 1;
            }
            RpnOp::SkillInvoke => {
                // Flush any pending LLM step first
                self.flush_llm_step()?;
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let skill = self.skill_active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.steps.push(SkillStep::InvokeAction { name });
                self.pc += 1;
            }
            RpnOp::SkillInvokeSymbol => {
                // Flush any pending LLM step first
                self.flush_llm_step()?;
                let symbol = self.pop_u32_coerce()?;
                let skill = self.skill_active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.steps.push(SkillStep::InvokeSymbol { symbol });
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
