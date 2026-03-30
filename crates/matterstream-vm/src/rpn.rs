//! msm1 RPN stack language VM.
//!
//! Features:
//! - Per-opcode gas metering with configurable budgets
//! - Backward-jump detection and loop limiting
//! - Execution trace/profiling
//! - Control flow: Jmp, JmpIf, Halt, comparisons
//! - Bitwise ops, typed bank access, event polling
//! - OR page (0x80+) dispatched by CR[0] for UI/VQL/SKLL/OBJT/CARD
//! - Persistent typed register banks (Tier 1/2 memory)
//! - UserCall (0x60) / CoprocessorCall (0x61) escape hatches
//! - SystemCall (0x71) privileged ops, SetCR (0x70)

use std::sync::Arc;

use matterstream_vm_addressing::fqa::Fqa;
use matterstream_vm_addressing::ova::Ova;
use matterstream_vm_addressing::oid::{Oid, SecurityMode, ImportKind};
use matterstream_vm_addressing::oid_index::OidIndex;
use matterstream_vm_arena::TripleArena;

use crate::shared::VmSharedState;

/// Native hook function signature for VM-escape dispatch.
pub type NativeHookFn = fn(vm: &mut RpnVm, arenas: &mut TripleArena) -> Result<(), RpnError>;

/// Descriptor for a loaded component (from a package archive).
#[derive(Clone, Debug)]
pub struct ComponentEntry {
    /// Index into `RpnVm::loaded_bytecodes`.
    pub bytecode_id: u16,
    /// Byte offset within the bytecode blob.
    pub offset: u32,
    /// Length in bytes.
    pub length: u32,
    /// Base offset added to string table indices during execution.
    pub string_base: u32,
}

use crate::event::{VmEvent, VmEventType};
use crate::ui_vm::{
    VqlOutput, VqlField, VQL_OUTPUT_MAX,
    ObjectTypeDef, ObjectFieldDef, OBJECT_TYPE_MAX,
    FOURCC_MTUI, FOURCC_VQL0,
};
use crate::or_page::OrPageHandler;
use crate::vm_handle::VmHandle;
#[cfg(feature = "ui")]
use crate::ui_vm::{
    UiDrawCmd, UiDrawState, UI_DRAW_CMD_MAX, UI_STATE_STACK_MAX,
    CardDef, CARD_DEF_MAX,
    MAT4_IDENTITY, mat4_multiply, apply_transform,
};

/// NOP macro for UI opcodes. With `ui` feature: executes body. Without: discards
/// `pops` values from the stack and advances PC past opcode + payload.
macro_rules! ui_op {
    ($self:expr, pops: $pops:expr, payload: $payload:expr, $body:block) => {{
        #[cfg(feature = "ui")]
        { $body }
        #[cfg(not(feature = "ui"))]
        {
            let len = $self.stack.len();
            $self.stack.truncate(len.saturating_sub($pops));
        }
        $self.pc += 1 + $payload;
    }};
}
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fmt;

// ── FourCC constants for OR page dispatch (OBJT, CARD) ─────────────────
/// FourCC: Object type definitions.
pub const FOURCC_OBJT: u32 = 0x4F424A54;
/// FourCC: Card definitions.
pub const FOURCC_CARD: u32 = 0x43415244;

// ── Security register constants ────────────────────────────────────────
pub const SECURITY_SANDBOXED: u64 = 0x01;
pub const SECURITY_INTERNAL: u64 = 0x02;
pub const SECURITY_SYSTEM: u64 = 0x03;

/// msm1 RPN opcodes (u8 repr).
///
/// Layout:
///   0x00-0x0D  Stack, memory, control
///   0x10-0x1A  Integer arithmetic + bitwise
///   0x20-0x28  Comparison (int + float)
///   0x30-0x37  Float arithmetic
///   0x40-0x4D  Data: banks, dict, destructure
///   0x50-0x55  Blocks + components (stubs)
///   0x60       UserCall [u64 action_op] [u64 data]
///   0x61       CoprocessorCall [u64 action] [u64 length] [u64 data]
///   0x70       SetCR [u8 cr_idx] [u64 value]
///   0x71       SystemCall [u64 action_op] [u64 data]
///   0x80+      OR page — dispatched by CR[0] FourCC
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RpnOp {
    // ── Stack, memory, control (0x00-0x0D) ──
    Nop         = 0x00,
    Push32      = 0x01,
    Push64      = 0x02,
    Push128     = 0x03,
    Dup         = 0x04,
    Drop        = 0x05,
    Swap        = 0x06,
    Load        = 0x07,
    Store       = 0x08,
    Call        = 0x09,
    Ret         = 0x0A,
    Jmp         = 0x0B,
    JmpIf       = 0x0C,
    Halt        = 0x0D,

    // ── Integer arithmetic + bitwise (0x10-0x1A) ──
    Add         = 0x10,
    Sub         = 0x11,
    Mul         = 0x12,
    Div         = 0x13,
    Mod         = 0x14,
    And         = 0x15,
    Or          = 0x16,
    Xor         = 0x17,
    Shl         = 0x18,
    Shr         = 0x19,
    Not         = 0x1A,

    // ── Comparison (0x20-0x28) ──
    CmpEq       = 0x20,
    CmpLt       = 0x21,
    CmpGt       = 0x22,
    CmpGe       = 0x23,
    CmpLe       = 0x24,
    CmpNe       = 0x25,
    FCmpGt      = 0x26,
    FCmpLt      = 0x27,
    FCmpEq      = 0x28,

    // ── Float arithmetic (0x30-0x37) ──
    FAdd        = 0x30,
    FSub        = 0x31,
    FMul        = 0x32,
    FDiv        = 0x33,
    FNeg        = 0x34,
    FAbs        = 0x35,
    I2F         = 0x36,
    F2I         = 0x37,

    // ── Data: banks, dict, destructure (0x40-0x4D) ──
    LoadBank      = 0x40,
    StoreBank     = 0x41,
    LoadZpI32     = 0x42,
    StoreZpI32    = 0x43,
    LoadBankComp  = 0x44,
    StoreBankComp = 0x45,
    DictNew       = 0x48,
    DictSet       = 0x49,
    DictGet       = 0x4A,
    Explode       = 0x4C,
    ExplodeMapped = 0x4D,
    /// PushIfElse: conditional push based on bank value.
    /// Pops [cond_bank, cond_slot, true_val, false_val].
    /// Pushes true_val if banks[cond_bank][cond_slot] != 0, else false_val.
    PushIfElse = 0x4E,

    // ── Blocks + components (0x50-0x55, stubs) ──
    DefineBlock     = 0x50,
    CallBlock       = 0x51,
    LoopOver        = 0x52,
    MapOver         = 0x53,
    DefineComponent = 0x54,
    ExecComponent   = 0x55,

    // ── User escape (0x60-0x6F) ──
    UserCall        = 0x60,
    CoprocessorCall = 0x61,

    // ── System escape (0x70-0x7F) ──
    SetCR       = 0x70,
    SystemCall  = 0x71,
}

// ── OR page enums (0x80+ sub-ops, offset from 0x80) ────────────────────
// Wire byte = 0x80 + variant as u8. Only one page is active based on CR[0].

/// MTUI OR page: UI draw operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MtuiOp {
    SetColor       = 0x00,
    Box            = 0x01,
    Slab           = 0x02,
    Circle         = 0x03,
    Text           = 0x04,
    PushState      = 0x05,
    PopState       = 0x06,
    ApplyOffset    = 0x07,
    Line           = 0x08,
    TextStr        = 0x09,
    Action         = 0x0A,
    ApplyMatrix    = 0x0B,
    ReplaceOffset  = 0x0C,
    ReplaceMatrix  = 0x0D,
    RibbonBegin    = 0x0E,
    RibbonEnd      = 0x0F,
}

impl MtuiOp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::SetColor),
            0x01 => Some(Self::Box),
            0x02 => Some(Self::Slab),
            0x03 => Some(Self::Circle),
            0x04 => Some(Self::Text),
            0x05 => Some(Self::PushState),
            0x06 => Some(Self::PopState),
            0x07 => Some(Self::ApplyOffset),
            0x08 => Some(Self::Line),
            0x09 => Some(Self::TextStr),
            0x0A => Some(Self::Action),
            0x0B => Some(Self::ApplyMatrix),
            0x0C => Some(Self::ReplaceOffset),
            0x0D => Some(Self::ReplaceMatrix),
            0x0E => Some(Self::RibbonBegin),
            0x0F => Some(Self::RibbonEnd),
            _ => None,
        }
    }
    /// Wire byte for bytecode emission.
    pub fn byte(self) -> u8 { 0x80 + self as u8 }
}

/// VQL0 OR page: Vesicle Query Language operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum VqlOp {
    BeginQuery   = 0x00,
    EndQuery     = 0x01,
    Bind         = 0x02,
    SetField     = 0x03,
    SetFieldStr  = 0x04,
    Filter       = 0x05,
    Project      = 0x06,
    Param        = 0x07,
}

impl VqlOp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::BeginQuery),
            0x01 => Some(Self::EndQuery),
            0x02 => Some(Self::Bind),
            0x03 => Some(Self::SetField),
            0x04 => Some(Self::SetFieldStr),
            0x05 => Some(Self::Filter),
            0x06 => Some(Self::Project),
            0x07 => Some(Self::Param),
            _ => None,
        }
    }
    pub fn byte(self) -> u8 { 0x80 + self as u8 }
}

/// SKLL OR page: Skill definition operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SkllOp {
    Begin              = 0x00,
    End                = 0x01,
    Step               = 0x02,
    LlmStep            = 0x03,
    Replaceable        = 0x04,
    Invoke             = 0x05,
    InvokeSymbol       = 0x06,
    LlmModel           = 0x07,
    LlmUseCase         = 0x08,
    SetShortDesc       = 0x09,
    SetLongDesc        = 0x0A,
    CronInterval       = 0x0B,
    CronJitter         = 0x0C,
    ForwardPrompt      = 0x0D,
    AddToSystemPrompt  = 0x0E,
}

impl SkllOp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::Begin),
            0x01 => Some(Self::End),
            0x02 => Some(Self::Step),
            0x03 => Some(Self::LlmStep),
            0x04 => Some(Self::Replaceable),
            0x05 => Some(Self::Invoke),
            0x06 => Some(Self::InvokeSymbol),
            0x07 => Some(Self::LlmModel),
            0x08 => Some(Self::LlmUseCase),
            0x09 => Some(Self::SetShortDesc),
            0x0A => Some(Self::SetLongDesc),
            0x0B => Some(Self::CronInterval),
            0x0C => Some(Self::CronJitter),
            0x0D => Some(Self::ForwardPrompt),
            0x0E => Some(Self::AddToSystemPrompt),
            _ => None,
        }
    }
    pub fn byte(self) -> u8 { 0x80 + self as u8 }
}

/// SKLS OR page: Skill execution operations (host callbacks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SklsOp {
    SetModel          = 0x00,
    AppendToPrompt    = 0x01,
    ExecutePrompt     = 0x02,
    ForwardToModel    = 0x03,
    QueueSkill        = 0x04,
    QueueAction       = 0x05,
    ExecuteSkill      = 0x06,
    ExecuteAction     = 0x07,
}

impl SklsOp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::SetModel),
            0x01 => Some(Self::AppendToPrompt),
            0x02 => Some(Self::ExecutePrompt),
            0x03 => Some(Self::ForwardToModel),
            0x04 => Some(Self::QueueSkill),
            0x05 => Some(Self::QueueAction),
            0x06 => Some(Self::ExecuteSkill),
            0x07 => Some(Self::ExecuteAction),
            _ => None,
        }
    }
    pub fn byte(self) -> u8 { 0x80 + self as u8 }
}

/// OBJT OR page: Object type definition operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ObjtOp {
    Begin        = 0x00,
    End          = 0x01,
    SetShortDesc = 0x02,
    SetLongDesc  = 0x03,
    Field        = 0x04,
}

impl ObjtOp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::Begin),
            0x01 => Some(Self::End),
            0x02 => Some(Self::SetShortDesc),
            0x03 => Some(Self::SetLongDesc),
            0x04 => Some(Self::Field),
            _ => None,
        }
    }
    pub fn byte(self) -> u8 { 0x80 + self as u8 }
}

/// CARD OR page: Card definition operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CardOp {
    Begin        = 0x00,
    End          = 0x01,
    SetShortDesc = 0x02,
    SetLongDesc  = 0x03,
}

impl CardOp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::Begin),
            0x01 => Some(Self::End),
            0x02 => Some(Self::SetShortDesc),
            0x03 => Some(Self::SetLongDesc),
            _ => None,
        }
    }
    pub fn byte(self) -> u8 { 0x80 + self as u8 }
}

/// UserCall sub-op identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u64)]
pub enum UserCallOp {
    EvPoll         = 0x00,
    EvHasEvent     = 0x01,
    FrameCount     = 0x02,
    Rand           = 0x03,
    OidImport      = 0x10,
    OidCall        = 0x11,
    OidCosineMatch = 0x12,
    /// Read from UserAtomicReadable[slot] → pushes u32.
    ReadUserAtomic       = 0x20,
    /// Write value to UserAtomicSubmitSemaphore[slot] (fire-and-forget).
    SubmitUserSemaphore  = 0x21,
    /// Get shared string[slot] → copy to string_bank[local_slot]. Mutex-protected.
    SharedStringGet      = 0x22,
    /// Set shared string[slot] from string_bank[local_slot]. Mutex-protected.
    SharedStringSet      = 0x23,
}

/// SystemCall sub-op identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u64)]
pub enum SystemCallOp {
    AtomicRead     = 0x00,
    AtomicWrite    = 0x01,
    AtomicRmw      = 0x02,
    NativeHook     = 0x03,
    CopyList       = 0x04,
    Sync           = 0x05,
    DefineBlock    = 0x06,
    SetOutputMode  = 0x07,
    OidExec        = 0x10,
}

impl RpnOp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(RpnOp::Nop),
            0x01 => Some(RpnOp::Push32),
            0x02 => Some(RpnOp::Push64),
            0x03 => Some(RpnOp::Push128),
            0x04 => Some(RpnOp::Dup),
            0x05 => Some(RpnOp::Drop),
            0x06 => Some(RpnOp::Swap),
            0x07 => Some(RpnOp::Load),
            0x08 => Some(RpnOp::Store),
            0x09 => Some(RpnOp::Call),
            0x0A => Some(RpnOp::Ret),
            0x0B => Some(RpnOp::Jmp),
            0x0C => Some(RpnOp::JmpIf),
            0x0D => Some(RpnOp::Halt),
            0x10 => Some(RpnOp::Add),
            0x11 => Some(RpnOp::Sub),
            0x12 => Some(RpnOp::Mul),
            0x13 => Some(RpnOp::Div),
            0x14 => Some(RpnOp::Mod),
            0x15 => Some(RpnOp::And),
            0x16 => Some(RpnOp::Or),
            0x17 => Some(RpnOp::Xor),
            0x18 => Some(RpnOp::Shl),
            0x19 => Some(RpnOp::Shr),
            0x1A => Some(RpnOp::Not),
            0x20 => Some(RpnOp::CmpEq),
            0x21 => Some(RpnOp::CmpLt),
            0x22 => Some(RpnOp::CmpGt),
            0x23 => Some(RpnOp::CmpGe),
            0x24 => Some(RpnOp::CmpLe),
            0x25 => Some(RpnOp::CmpNe),
            0x26 => Some(RpnOp::FCmpGt),
            0x27 => Some(RpnOp::FCmpLt),
            0x28 => Some(RpnOp::FCmpEq),
            0x30 => Some(RpnOp::FAdd),
            0x31 => Some(RpnOp::FSub),
            0x32 => Some(RpnOp::FMul),
            0x33 => Some(RpnOp::FDiv),
            0x34 => Some(RpnOp::FNeg),
            0x35 => Some(RpnOp::FAbs),
            0x36 => Some(RpnOp::I2F),
            0x37 => Some(RpnOp::F2I),
            0x40 => Some(RpnOp::LoadBank),
            0x41 => Some(RpnOp::StoreBank),
            0x42 => Some(RpnOp::LoadZpI32),
            0x43 => Some(RpnOp::StoreZpI32),
            0x44 => Some(RpnOp::LoadBankComp),
            0x45 => Some(RpnOp::StoreBankComp),
            0x48 => Some(RpnOp::DictNew),
            0x49 => Some(RpnOp::DictSet),
            0x4A => Some(RpnOp::DictGet),
            0x4C => Some(RpnOp::Explode),
            0x4D => Some(RpnOp::ExplodeMapped),
            0x4E => Some(RpnOp::PushIfElse),
            0x50 => Some(RpnOp::DefineBlock),
            0x51 => Some(RpnOp::CallBlock),
            0x52 => Some(RpnOp::LoopOver),
            0x53 => Some(RpnOp::MapOver),
            0x54 => Some(RpnOp::DefineComponent),
            0x55 => Some(RpnOp::ExecComponent),
            0x60 => Some(RpnOp::UserCall),
            0x61 => Some(RpnOp::CoprocessorCall),
            0x70 => Some(RpnOp::SetCR),
            0x71 => Some(RpnOp::SystemCall),
            _ => None,
        }
    }

    /// Payload size in bytes for this opcode.
    pub fn payload_size(&self) -> usize {
        match self {
            RpnOp::Push32 => 4,
            RpnOp::Push64 | RpnOp::Jmp | RpnOp::JmpIf => 8,
            RpnOp::Push128 => 16,
            RpnOp::UserCall => 16,        // [u64 action_op] [u64 data]
            RpnOp::CoprocessorCall => 24,  // [u64 action] [u64 length] [u64 data]
            RpnOp::SetCR => 9,            // [u8 cr_idx] [u64 value]
            RpnOp::SystemCall => 16,       // [u64 action_op] [u64 data]
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
    pub cost_dict: u64,
    pub cost_jump: u64,
    pub cost_compare: u64,
    pub cost_ui: u64,
    pub cost_bitwise: u64,
    pub cost_bank: u64,
    pub cost_event: u64,
    pub cost_cr: u64,
    pub cost_vql: u64,
    pub cost_skill: u64,
    pub cost_skls: u64,
    pub cost_user_call: u64,
    pub cost_system_call: u64,
    pub cost_block: u64,
    pub cost_or_page_default: u64,
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
            cost_dict: 5,
            cost_jump: 2,
            cost_compare: 2,
            cost_ui: 5,
            cost_bitwise: 2,
            cost_bank: 3,
            cost_event: 5,
            cost_cr: 2,
            cost_vql: 100,
            cost_skill: 100,
            cost_skls: 100,
            cost_user_call: 50,

            cost_system_call: 20,
            cost_block: 5,
            cost_or_page_default: 100,
        }
    }

    /// Gas cost for a given opcode.
    pub fn cost_of(&self, op: RpnOp) -> u64 {
        match op {
            RpnOp::Nop | RpnOp::Halt => self.cost_nop,
            RpnOp::Push32 | RpnOp::Push64 | RpnOp::Push128 => self.cost_push,
            RpnOp::Dup | RpnOp::Drop | RpnOp::Swap => self.cost_stack_op,
            RpnOp::Add | RpnOp::Sub | RpnOp::Mul | RpnOp::Div | RpnOp::Mod
            | RpnOp::FAdd | RpnOp::FSub | RpnOp::FMul | RpnOp::FDiv
            | RpnOp::FNeg | RpnOp::FAbs | RpnOp::I2F | RpnOp::F2I => {
                self.cost_arithmetic
            }
            RpnOp::CmpEq | RpnOp::CmpLt | RpnOp::CmpGt | RpnOp::CmpGe | RpnOp::CmpLe
            | RpnOp::CmpNe | RpnOp::FCmpGt | RpnOp::FCmpLt | RpnOp::FCmpEq => {
                self.cost_compare
            }
            RpnOp::Load | RpnOp::Store => self.cost_memory,
            RpnOp::Call | RpnOp::Ret => self.cost_call,
            RpnOp::Jmp | RpnOp::JmpIf => self.cost_jump,
            RpnOp::And | RpnOp::Or | RpnOp::Xor | RpnOp::Shl | RpnOp::Shr | RpnOp::Not => {
                self.cost_bitwise
            }
            RpnOp::LoadBank | RpnOp::StoreBank
            | RpnOp::LoadZpI32 | RpnOp::StoreZpI32
            | RpnOp::LoadBankComp | RpnOp::StoreBankComp => self.cost_bank,
            RpnOp::DictNew | RpnOp::DictSet | RpnOp::DictGet => self.cost_dict,
            RpnOp::Explode | RpnOp::ExplodeMapped => self.cost_dict,
            RpnOp::PushIfElse => self.cost_compare,
            RpnOp::DefineBlock | RpnOp::CallBlock | RpnOp::LoopOver | RpnOp::MapOver
            | RpnOp::DefineComponent | RpnOp::ExecComponent => self.cost_block,
            RpnOp::UserCall | RpnOp::CoprocessorCall => self.cost_user_call,
            RpnOp::SetCR => self.cost_cr,
            RpnOp::SystemCall => self.cost_system_call,
        }
    }

    /// Gas cost for a built-in OR page opcode. Returns None for unregistered FourCCs.
    pub fn cost_of_or_page_builtin(&self, cr0: u32) -> Option<u64> {
        match cr0 {
            FOURCC_MTUI => Some(self.cost_ui),
            FOURCC_VQL0 => Some(self.cost_vql),
            FOURCC_OBJT => Some(self.cost_skill),
            FOURCC_CARD => Some(self.cost_ui),
            _ => None,
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
    ObjTypeLimitExceeded,
    ObjTypeNoActiveDef,
    CardLimitExceeded,
    CardNoActiveDef,
    ComponentNotFound(u128),
    ComponentDepthExceeded,
    OidNotFound { hi: u64, lo: u64 },
    OidSecurityViolation { hi: u64, lo: u64, required: &'static str, actual: &'static str },
    OidNoIndicesLoaded,
    OidInvalidNativeHook(u32),
    SecurityViolation(&'static str),
    InvalidOrPage(u32),
    InvalidUserCallAction(u64),
    InvalidSystemCallAction(u64),
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
            RpnError::ObjTypeLimitExceeded => write!(f, "RPN object type limit exceeded"),
            RpnError::ObjTypeNoActiveDef => write!(f, "RPN no active object type definition"),
            RpnError::CardLimitExceeded => write!(f, "RPN card definition limit exceeded"),
            RpnError::CardNoActiveDef => write!(f, "RPN no active card definition"),
            RpnError::ComponentNotFound(fqa) => write!(f, "RPN component not found: {:#034x}", fqa),
            RpnError::ComponentDepthExceeded => write!(f, "RPN component nesting depth exceeded (max 16)"),
            RpnError::OidNotFound { hi, lo } => {
                write!(f, "RPN OID not found: {:#018x}:{:#018x}", hi, lo)
            }
            RpnError::OidSecurityViolation { hi, lo, required, actual } => {
                write!(f, "RPN OID security violation: {:#018x}:{:#018x} requires {} but caller is {}", hi, lo, required, actual)
            }
            RpnError::OidNoIndicesLoaded => write!(f, "RPN no OID indices loaded"),
            RpnError::OidInvalidNativeHook(id) => write!(f, "RPN invalid native hook dispatch_id: {}", id),
            RpnError::SecurityViolation(msg) => write!(f, "RPN security violation: {}", msg),
            RpnError::InvalidOrPage(fourcc) => write!(f, "RPN invalid OR page FourCC: {:#010x}", fourcc),
            RpnError::InvalidUserCallAction(action) => write!(f, "RPN invalid UserCall action: {:#x}", action),
            RpnError::InvalidSystemCallAction(action) => write!(f, "RPN invalid SystemCall action: {:#x}", action),
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
    // ── Core VM state ───────────────────────────────────────────────────
    pub stack: Vec<RpnValue>,
    pub call_stack: Vec<usize>,
    pub pc: usize,
    pub max_stack_depth: usize,
    pub max_cycles: usize,
    pub synced: bool,
    pub gas: GasConfig,
    pub trace: ExecTrace,
    // Typed register banks (Tier 1) — persist between execute() calls
    pub scalar_bank: [f32; 16],
    pub int_bank: [i32; 16],
    pub vec3_bank: [[f32; 3]; 16],
    pub vec4_bank: [[f32; 4]; 16],
    // ZeroPage (Tier 2) — 256 bytes for grids/arrays
    pub zero_page: [u8; 256],
    // String table for UiTextStr
    pub string_table: Vec<String>,
    /// Mutable string bank (runtime-writable, 256 nullable slots).
    pub string_bank: Vec<Option<String>>,

    // ── Shared state (external Arc or local fallback) ──────────────────
    /// External shared state provider. When set, ReadUserAtomic / SubmitUserSemaphore /
    /// SharedStringGet/Set dispatch through this trait. When None, falls back to local arrays.
    pub shared_state: Option<Arc<dyn VmSharedState>>,
    /// Local fallback: readable atomics (256 slots). Used when shared_state is None.
    pub user_atomics_readable: Vec<std::sync::atomic::AtomicU32>,
    /// Local fallback: submit semaphores (256 slots). Used when shared_state is None.
    pub user_atomics_submit: Vec<std::sync::atomic::AtomicU32>,
    /// Local fallback: shared strings (256 slots). Used when shared_state is None.
    pub user_shared_strings: Vec<std::sync::Mutex<Option<String>>>,
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
    // Object type state
    pub objtype_outputs: Vec<ObjectTypeDef>,
    objtype_active: Option<ObjectTypeDef>,

    // ── SDF draw output ──────────────────────────────────────────────────
    /// SDF draw commands produced by MTUI ops — unified GPU-uploadable format.
    pub sdf_draws: Vec<matterstream_common::SdfDrawCmd>,

    // ── OID import state ────────────────────────────────────────────────
    /// Loaded .osym indices — raw bytes, binary searched directly.
    pub oid_indices: Vec<Vec<u8>>,
    /// Native hook dispatch table (system/internal packages only).
    pub native_hooks: Vec<NativeHookFn>,

    // ── Component import state ─────────────────────────────────────────
    /// Component registry: FQA (u128) → entry descriptor.
    pub component_table: std::collections::HashMap<u128, ComponentEntry>,
    /// Loaded bytecode blobs from packages (indexed by bytecode_id).
    pub loaded_bytecodes: Vec<Vec<u8>>,
    /// String base offset — added to string table indices during component execution.
    pub string_base_offset: u32,
    /// Component nesting depth guard (max 16).
    pub component_depth: u8,

    /// External OR page handlers — (fourcc, handler) pairs, linear scanned.
    or_pages: Vec<(u32, Box<dyn OrPageHandler>)>,

    // ── UI state (requires "ui" feature) ────────────────────────────────
    #[cfg(feature = "ui")]
    pub ui_draws: Vec<UiDrawCmd>,
    #[cfg(feature = "ui")]
    pub ui_state: UiDrawState,
    #[cfg(feature = "ui")]
    pub ui_state_stack: Vec<UiDrawState>,
    #[cfg(feature = "ui")]
    pub transform_stack: Vec<[f32; 16]>,
    #[cfg(feature = "ui")]
    pub card_outputs: Vec<CardDef>,
    #[cfg(feature = "ui")]
    card_active: Option<CardDef>,
    #[cfg(feature = "ui")]
    card_capturing: bool,
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
            scalar_bank: [0.0; 16],
            int_bank: [0; 16],
            vec3_bank: [[0.0; 3]; 16],
            vec4_bank: [[0.0; 4]; 16],
            zero_page: [0; 256],
            string_table: Vec::new(),
            string_bank: vec![None; 256],
            shared_state: None,
            user_atomics_readable: (0..256).map(|_| std::sync::atomic::AtomicU32::new(0)).collect(),
            user_atomics_submit: (0..256).map(|_| std::sync::atomic::AtomicU32::new(0)).collect(),
            user_shared_strings: (0..256).map(|_| std::sync::Mutex::new(None)).collect(),
            event_queue: VecDeque::new(),
            frame_count: 0,
            rng: SimpleRng::new(0xDEAD_BEEF),
            cr_bank: [FOURCC_MTUI, 0, 0, 0, 0, 0, 0, 0],
            vql_outputs: Vec::new(),
            vql_active: None,
            objtype_outputs: Vec::new(),
            objtype_active: None,
            sdf_draws: Vec::new(),
            oid_indices: Vec::new(),
            native_hooks: Vec::new(),
            component_table: std::collections::HashMap::new(),
            loaded_bytecodes: Vec::new(),
            string_base_offset: 0,
            component_depth: 0,
            or_pages: Vec::new(),
            #[cfg(feature = "ui")]
            ui_draws: Vec::new(),
            #[cfg(feature = "ui")]
            ui_state: UiDrawState::default(),
            #[cfg(feature = "ui")]
            ui_state_stack: Vec::new(),
            #[cfg(feature = "ui")]
            transform_stack: vec![MAT4_IDENTITY],
            #[cfg(feature = "ui")]
            card_outputs: Vec::new(),
            #[cfg(feature = "ui")]
            card_active: None,
            #[cfg(feature = "ui")]
            card_capturing: false,
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

    /// Register an external OR page handler by FourCC.
    pub fn register_or_page(&mut self, fourcc: u32, handler: Box<dyn OrPageHandler>) {
        if let Some(entry) = self.or_pages.iter_mut().find(|(f, _)| *f == fourcc) {
            entry.1 = handler;
        } else {
            self.or_pages.push((fourcc, handler));
        }
    }

    /// Remove and return an external OR page handler by FourCC, downcasting to a concrete type.
    pub fn take_or_page<T: 'static>(&mut self, fourcc: u32) -> Option<Box<T>> {
        let idx = self.or_pages.iter().position(|(f, _)| *f == fourcc)?;
        let (_, handler) = self.or_pages.swap_remove(idx);
        handler.as_any().downcast::<T>().ok()
    }

    /// Public push for native hooks (VM-escape).
    pub fn push_value(&mut self, val: RpnValue) -> Result<(), RpnError> {
        self.push(val)
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
        let effective_idx = idx + self.string_base_offset;
        self.string_table
            .get(effective_idx as usize)
            .cloned()
            .ok_or(RpnError::InvalidStringIndex(effective_idx))
    }

    #[cfg(feature = "ui")]
    fn current_transform(&self) -> &[f32; 16] {
        self.transform_stack.last().unwrap_or(&MAT4_IDENTITY)
    }

    #[cfg(feature = "ui")]
    fn transform_point(&self, x: i32, y: i32) -> (i32, i32) {
        apply_transform(self.current_transform(), x, y)
    }

    #[cfg(feature = "ui")]
    fn push_draw(&mut self, cmd: UiDrawCmd) -> Result<(), RpnError> {
        if self.card_capturing {
            let card = self.card_active.as_mut().ok_or(RpnError::CardNoActiveDef)?;
            card.draws.push(cmd);
        } else {
            if self.ui_draws.len() >= UI_DRAW_CMD_MAX {
                return Err(RpnError::UiDrawLimitExceeded);
            }
            self.ui_draws.push(cmd);
        }
        Ok(())
    }

    /// Push an SdfDrawCmd to the draw list (always available, not feature-gated).
    fn push_sdf_draw(&mut self, cmd: matterstream_common::SdfDrawCmd) {
        if self.sdf_draws.len() < matterstream_common::MAX_DRAW_CMDS {
            self.sdf_draws.push(cmd);
        }
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

    /// Resolve an OID by binary searching across all loaded .osym indices.
    fn resolve_oid(&self, oid: Oid) -> Result<matterstream_vm_addressing::oid_index::OidEntry, RpnError> {
        for raw in &self.oid_indices {
            if let Ok(idx) = OidIndex::from_bytes(raw) {
                if let Some(entry) = idx.lookup(oid) {
                    return Ok(entry);
                }
            }
        }
        Err(RpnError::OidNotFound { hi: oid.hi, lo: oid.lo })
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

    /// Consume gas for an OR page opcode (cost depends on CR[0]).
    fn consume_gas_or_page(&mut self, sub_op: u8) -> Result<(), RpnError> {
        let cr0 = self.cr_bank[0];
        let cost = self.gas.cost_of_or_page_builtin(cr0).unwrap_or_else(|| {
            self.or_pages
                .iter()
                .find(|(f, _)| *f == cr0)
                .map(|(_, h)| {
                    let c = h.gas_cost(sub_op);
                    if c == 0 { self.gas.cost_or_page_default } else { c }
                })
                .unwrap_or(self.gas.cost_or_page_default)
        });
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
        // Stack is cleared, but banks persist
        self.stack.clear();
        self.call_stack.clear();
        // Reset control registers — preserve CR[1] (security, set by loader)
        let security = self.cr_bank[1];
        self.cr_bank = [FOURCC_MTUI, security, 0, 0, 0, 0, 0, 0];
        self.vql_outputs.clear();
        self.vql_active = None;
        self.objtype_outputs.clear();
        self.objtype_active = None;
        self.sdf_draws.clear();
        // UI state reset
        #[cfg(feature = "ui")]
        {
            self.ui_draws.clear();
            self.ui_state = UiDrawState::default();
            self.ui_state_stack.clear();
            self.card_outputs.clear();
            self.card_active = None;
            self.card_capturing = false;
        }
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

    /// Execute bytecode without resetting VM state.
    /// Used by ExecComponent for recursive component calls.
    fn execute_inner(
        &mut self,
        bytecode: &[u8],
        arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        let saved_halted = self.trace.halted;
        self.trace.halted = false;
        self.pc = 0;
        let mut cycles = 0usize;

        while self.pc < bytecode.len() {
            cycles += 1;
            if cycles > self.max_cycles {
                return Err(RpnError::CycleLimitExceeded);
            }
            self.step(bytecode, arenas)?;
            if self.trace.halted {
                // Halt inside component = return from component, not kill VM
                self.trace.halted = saved_halted;
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

        // ── OR page dispatch (0x80+) ─────────────────────────────────
        if op_byte >= 0x80 {
            self.consume_gas_or_page(op_byte - 0x80)?;
            self.trace.opcodes_executed += 1;
            return self.dispatch_or_page(op_byte, bytecode, arenas);
        }

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
            RpnOp::Push128 => {
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
            // ── Integer arithmetic ──
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
            // ── Bitwise ──
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
            // ── Comparison ──
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
            // ── Float arithmetic ──
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
            // ── Typed bank access ──
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
            // ── Dict (renamed from Map) ──
            RpnOp::DictNew => {
                self.push(RpnValue::Map(HashMap::new()))?;
                self.pc += 1;
            }
            RpnOp::DictSet => {
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
            RpnOp::DictGet => {
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
            // ── Explode/ExplodeMapped (stubs) ──
            RpnOp::Explode => {
                // Stub: pop count, push nothing
                let _count = self.pop_u32_coerce()?;
                self.pc += 1;
            }
            RpnOp::ExplodeMapped => {
                // Stub: pop count, push nothing
                let _count = self.pop_u32_coerce()?;
                self.pc += 1;
            }
            RpnOp::PushIfElse => {
                // Pops [packed_ref, true_val, false_val]
                // packed_ref = u16 bank_type << 16 | u16 slot
                let false_val = self.pop_u32_coerce()?;
                let true_val = self.pop_u32_coerce()?;
                let packed_ref = self.pop_u32_coerce()?;
                let bank_type = (packed_ref >> 16) as u16;
                let slot = (packed_ref & 0xFFFF) as u16;

                let condition = match bank_type as u32 {
                    BANK_SCALAR => {
                        (slot as usize) < self.scalar_bank.len() && self.scalar_bank[slot as usize] != 0.0
                    }
                    BANK_INT => {
                        (slot as usize) < self.int_bank.len() && self.int_bank[slot as usize] != 0
                    }
                    _ => false,
                };

                let result = if condition { true_val } else { false_val };
                self.push(RpnValue::U32(result))?;
                self.pc += 1;
            }
            // ── Blocks + components (stubs) ──
            RpnOp::DefineBlock => {
                // Stub: register callable block at PC offset
                self.pc += 1;
            }
            RpnOp::CallBlock => {
                // Stub: call a defined block
                self.pc += 1;
            }
            RpnOp::LoopOver => {
                // Stub: [n, block_idx] — call block for each
                self.pc += 1;
            }
            RpnOp::MapOver => {
                // Stub: [n, block_idx] — call block for each, push results
                self.pc += 1;
            }
            RpnOp::DefineComponent => {
                // Stub: creates subpackage
                self.pc += 1;
            }
            RpnOp::ExecComponent => {
                // Pop FQA, look up component, execute with state isolation
                let fqa_val = self.stack.pop().ok_or(RpnError::StackUnderflow)?;
                let fqa_key = match fqa_val {
                    RpnValue::Fqa(f) => f.0,
                    RpnValue::U64(v) => v as u128,
                    RpnValue::U32(v) => v as u128,
                    _ => return Err(RpnError::TypeMismatch),
                };
                let entry = self.component_table.get(&fqa_key)
                    .ok_or(RpnError::ComponentNotFound(fqa_key))?
                    .clone();
                if self.component_depth >= 16 {
                    return Err(RpnError::ComponentDepthExceeded);
                }

                // Save context
                let saved_pc = self.pc;
                let saved_string_base = self.string_base_offset;
                let saved_depth = self.component_depth;

                // UI state isolation
                #[cfg(feature = "ui")]
                {
                    self.ui_state_stack.push(self.ui_state);
                    self.transform_stack.push(MAT4_IDENTITY);
                }

                // Enter component
                self.string_base_offset = entry.string_base;
                self.component_depth += 1;

                let bc = self.loaded_bytecodes[entry.bytecode_id as usize].clone();
                let component_bc = &bc[entry.offset as usize..(entry.offset + entry.length) as usize];
                let result = self.execute_inner(component_bc, arenas);

                // Restore context
                self.pc = saved_pc + 1;
                self.string_base_offset = saved_string_base;
                self.component_depth = saved_depth;

                #[cfg(feature = "ui")]
                {
                    self.transform_stack.pop();
                    if let Some(state) = self.ui_state_stack.pop() {
                        self.ui_state = state;
                    }
                }

                result?;
                return Ok(()); // pc already advanced
            }
            // ── UserCall (0x60): unprivileged escape ──
            RpnOp::UserCall => {
                let action_op = Self::read_u64(bytecode, self.pc + 1)?;
                let data = Self::read_u64(bytecode, self.pc + 9)?;
                self.dispatch_user_call(action_op, data, bytecode, arenas)?;
                self.pc += 17; // 1 + 8 + 8
            }
            // ── CoprocessorCall (0x61): reserved ──
            RpnOp::CoprocessorCall => {
                // Reserved — coprocessor system TBD
                // Format: [u64 action] [u64 length] [u64 data]
                // For now, just advance past the payload
                self.pc += 25; // 1 + 8 + 8 + 8
            }
            // ── SetCR (0x70): privileged control register write ──
            RpnOp::SetCR => {
                let cr_idx = Self::read_u8(bytecode, self.pc + 1)?;
                let value = Self::read_u64(bytecode, self.pc + 2)?;
                if cr_idx as usize >= self.cr_bank.len() {
                    return Err(RpnError::InvalidCRIndex(cr_idx as u32));
                }
                // CR[1] write is security-sensitive (would check $EXEC_PKG in production)
                self.cr_bank[cr_idx as usize] = value as u32;
                self.pc += 10; // 1 + 1 + 8
            }
            // ── SystemCall (0x71): privileged escape ──
            RpnOp::SystemCall => {
                // Requires CR[1] >= INTERNAL
                if (self.cr_bank[1] as u64) < SECURITY_INTERNAL {
                    return Err(RpnError::SecurityViolation("SystemCall requires CR[1] >= INTERNAL"));
                }
                let action_op = Self::read_u64(bytecode, self.pc + 1)?;
                let data = Self::read_u64(bytecode, self.pc + 9)?;
                self.dispatch_system_call(action_op, data, bytecode, arenas)?;
                self.pc += 17; // 1 + 8 + 8
            }
        }

        Ok(())
    }

    /// Dispatch UserCall sub-operations (0x60).
    fn dispatch_user_call(
        &mut self,
        action_op: u64,
        _data: u64,
        _bytecode: &[u8],
        arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        match action_op {
            // 0x00 EvPoll
            0x00 => {
                if let Some(ev) = self.event_queue.pop_front() {
                    self.push(RpnValue::U64(ev.data))?;
                    self.push(RpnValue::U32(ev.etype as u32))?;
                } else {
                    self.push(RpnValue::U64(0))?;
                    self.push(RpnValue::U32(VmEventType::None as u32))?;
                }
            }
            // 0x01 EvHasEvent
            0x01 => {
                let flag = if self.event_queue.is_empty() { 0u64 } else { 1 };
                self.push(RpnValue::U64(flag))?;
            }
            // 0x02 FrameCount
            0x02 => {
                self.push(RpnValue::U64(self.frame_count))?;
            }
            // 0x03 Rand
            0x03 => {
                let max = self.pop_u32_coerce()?;
                let value = self.rng.next_bounded(max);
                self.push(RpnValue::U32(value))?;
            }
            // 0x10 OidImport — pops a u128 (Push128/Fqa) or two u64s (hi, lo)
            0x10 => {
                let val = self.pop()?;
                let (oid, hi, lo) = match val {
                    RpnValue::Fqa(fqa) => {
                        let v = fqa.value();
                        let hi = (v >> 64) as u64;
                        let lo = v as u64;
                        (Oid::new(hi, lo), hi, lo)
                    }
                    RpnValue::U64(lo_val) => {
                        let hi = self.pop_u64()?;
                        (Oid::new(hi, lo_val), hi, lo_val)
                    }
                    _ => return Err(RpnError::TypeMismatch),
                };
                let entry = self.resolve_oid(oid)?;

                let mode = oid.security_mode();
                if entry.kind == ImportKind::NativeHook && mode == SecurityMode::Sandboxed {
                    return Err(RpnError::OidSecurityViolation {
                        hi, lo,
                        required: "System or Internal",
                        actual: "Sandboxed",
                    });
                }

                let fqa = entry.fqa();
                self.push(RpnValue::Fqa(fqa))?;
            }
            // 0x11 OidCall — pops a u128 (Push128/Fqa) or two u64s (hi, lo)
            0x11 => {
                let val = self.pop()?;
                let (oid, hi, lo) = match val {
                    RpnValue::Fqa(fqa) => {
                        let v = fqa.value();
                        let hi = (v >> 64) as u64;
                        let lo = v as u64;
                        (Oid::new(hi, lo), hi, lo)
                    }
                    RpnValue::U64(lo_val) => {
                        let hi = self.pop_u64()?;
                        (Oid::new(hi, lo_val), hi, lo_val)
                    }
                    _ => return Err(RpnError::TypeMismatch),
                };
                let entry = self.resolve_oid(oid)?;

                let mode = oid.security_mode();
                match entry.kind {
                    ImportKind::NativeHook => {
                        if mode == SecurityMode::Sandboxed {
                            return Err(RpnError::OidSecurityViolation {
                                hi, lo,
                                required: "System or Internal",
                                actual: "Sandboxed",
                            });
                        }
                        let dispatch_id = entry.dispatch_id();
                        if dispatch_id as usize >= self.native_hooks.len() {
                            return Err(RpnError::OidInvalidNativeHook(dispatch_id));
                        }
                        let hook = self.native_hooks[dispatch_id as usize];
                        hook(self, arenas)?;
                    }
                    _ => {
                        let fqa = entry.fqa();
                        self.push(RpnValue::Fqa(fqa))?;
                    }
                }
            }
            // 0x12 OidCosineMatch (stub)
            0x12 => {
                // Stub: push 0 as match score
                self.push(RpnValue::U64(0))?;
            }
            // 0x20 ReadUserAtomic — read from shared_state or local fallback
            0x20 => {
                let slot = self.pop_u32_coerce()? as usize;
                let val = if let Some(ref shared) = self.shared_state {
                    shared.read_atomic(slot)
                } else if slot < self.user_atomics_readable.len() {
                    self.user_atomics_readable[slot].load(std::sync::atomic::Ordering::Relaxed)
                } else {
                    0
                };
                self.push(RpnValue::U32(val))?;
            }
            // 0x21 SubmitUserSemaphore — write to shared_state or local fallback
            0x21 => {
                let val = self.pop_u32_coerce()?;
                let slot = self.pop_u32_coerce()? as usize;
                if let Some(ref shared) = self.shared_state {
                    shared.write_semaphore(slot, val);
                } else if slot < self.user_atomics_submit.len() {
                    self.user_atomics_submit[slot].store(val, std::sync::atomic::Ordering::Relaxed);
                }
            }
            // 0x22 SharedStringGet — read from shared_state or local fallback → string_bank
            0x22 => {
                let local_slot = self.pop_u32_coerce()? as usize;
                let shared_slot = self.pop_u32_coerce()? as usize;
                let val = if let Some(ref shared) = self.shared_state {
                    shared.read_string(shared_slot)
                } else if shared_slot < self.user_shared_strings.len() {
                    self.user_shared_strings[shared_slot].lock().unwrap().clone()
                } else {
                    None
                };
                if local_slot < self.string_bank.len() {
                    self.string_bank[local_slot] = val;
                }
            }
            // 0x23 SharedStringSet — write string_bank → shared_state or local fallback
            0x23 => {
                let shared_slot = self.pop_u32_coerce()? as usize;
                let local_slot = self.pop_u32_coerce()? as usize;
                let val = if local_slot < self.string_bank.len() {
                    self.string_bank[local_slot].clone()
                } else {
                    None
                };
                if let Some(ref shared) = self.shared_state {
                    shared.write_string(shared_slot, val);
                } else if shared_slot < self.user_shared_strings.len() {
                    *self.user_shared_strings[shared_slot].lock().unwrap() = val;
                }
            }
            _ => {
                return Err(RpnError::InvalidUserCallAction(action_op));
            }
        }
        Ok(())
    }

    /// Dispatch SystemCall sub-operations (0x71).
    fn dispatch_system_call(
        &mut self,
        action_op: u64,
        _data: u64,
        _bytecode: &[u8],
        arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        match action_op {
            // 0x00 AtomicRead (stub)
            0x00 => {
                self.push(RpnValue::U64(0))?;
            }
            // 0x01 AtomicWrite (stub)
            0x01 => {
                let _val = self.pop_u64()?;
            }
            // 0x02 AtomicRmw (stub)
            0x02 => {
                self.push(RpnValue::U64(0))?;
            }
            // 0x03 NativeHook
            0x03 => {
                let dispatch_id = self.pop_u32_coerce()?;
                if dispatch_id as usize >= self.native_hooks.len() {
                    return Err(RpnError::OidInvalidNativeHook(dispatch_id));
                }
                let hook = self.native_hooks[dispatch_id as usize];
                hook(self, arenas)?;
            }
            // 0x04 CopyList (stub)
            0x04 => {}
            // 0x05 Sync
            0x05 => {
                arenas.sync();
                self.synced = true;
                self.trace.syncs += 1;
            }
            // 0x06 DefineBlock (privileged variant, stub)
            0x06 => {}
            // 0x07 SetOutputMode — set CR[0] to the FourCC in data
            0x07 => {
                self.cr_bank[0] = _data as u32;
            }
            // 0x10 OidExec (stub)
            0x10 => {
                self.push(RpnValue::U64(0))?;
            }
            _ => {
                return Err(RpnError::InvalidSystemCallAction(action_op));
            }
        }
        Ok(())
    }

    /// Dispatch OR page opcodes (0x80+). The sub-op byte is the raw opcode.
    /// Routing depends on CR[0] FourCC.
    fn dispatch_or_page(
        &mut self,
        op_byte: u8,
        _bytecode: &[u8],
        arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        let sub_op = op_byte - 0x80;
        let cr0 = self.cr_bank[0];

        match cr0 {
            FOURCC_MTUI => self.dispatch_or_mtui(sub_op),
            FOURCC_VQL0 => self.dispatch_or_vql(sub_op),
            FOURCC_OBJT => self.dispatch_or_objt(sub_op),
            FOURCC_CARD => self.dispatch_or_card(sub_op),
            _ => {
                let idx = self.or_pages.iter().position(|(f, _)| *f == cr0);
                if let Some(i) = idx {
                    let (_, mut handler) = self.or_pages.swap_remove(i);
                    let mut handle = VmHandle { vm: self };
                    let result = handler.dispatch(sub_op, &mut handle, arenas);
                    self.or_pages.push((cr0, handler));
                    // Advance PC — external handlers don't manage PC
                    self.pc += 1;
                    result
                } else {
                    Err(RpnError::InvalidOrPage(cr0))
                }
            }
        }
    }

    /// OR page: MTUI sub-ops
    /// 0x00 SetColor, 0x01 Box, 0x02 Slab, 0x03 Circle,
    /// 0x04 Text, 0x05 PushState, 0x06 PopState,
    /// 0x07 ApplyOffset, 0x08 Line, 0x09 TextStr,
    /// 0x0A Action, 0x0B ApplyMatrix, 0x0C ReplaceOffset,
    /// 0x0D ReplaceMatrix
    fn dispatch_or_mtui(&mut self, sub_op: u8) -> Result<(), RpnError> {
        match sub_op {
            // 0x00 SetColor
            0x00 => ui_op!(self, pops: 1, payload: 0, {
                let rgba = self.pop_u32_coerce()?;
                self.ui_state.color = rgba;
            }),
            // 0x01 Box
            0x01 => ui_op!(self, pops: 4, payload: 0, {
                let h = self.pop_u32_coerce()?;
                let w = self.pop_u32_coerce()?;
                let raw_y = self.pop_u32_coerce()? as i32;
                let raw_x = self.pop_u32_coerce()? as i32;
                let (x, y) = self.transform_point(raw_x, raw_y);
                let color = self.ui_state.color;
                self.push_draw(UiDrawCmd::Box { x, y, w, h, color })?;
                self.push_sdf_draw(matterstream_common::SdfDrawCmd {
                    pos: [x as f32, y as f32], size: [w as f32, h as f32],
                    color: matterstream_common::color_u32_to_f32(color),
                    params: [matterstream_common::DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
                });
            }),
            // 0x02 Slab
            0x02 => ui_op!(self, pops: 5, payload: 0, {
                let radius = self.pop_u32_coerce()?;
                let h = self.pop_u32_coerce()?;
                let w = self.pop_u32_coerce()?;
                let raw_y = self.pop_u32_coerce()? as i32;
                let raw_x = self.pop_u32_coerce()? as i32;
                let (x, y) = self.transform_point(raw_x, raw_y);
                let color = self.ui_state.color;
                self.push_draw(UiDrawCmd::Slab { x, y, w, h, radius, color })?;
                self.push_sdf_draw(matterstream_common::SdfDrawCmd {
                    pos: [x as f32, y as f32], size: [w as f32, h as f32],
                    color: matterstream_common::color_u32_to_f32(color),
                    params: [matterstream_common::DRAW_TYPE_SLAB, radius as f32, 0.0, 0.0],
                });
            }),
            // 0x03 Circle
            0x03 => ui_op!(self, pops: 3, payload: 0, {
                let r = self.pop_u32_coerce()?;
                let raw_y = self.pop_u32_coerce()? as i32;
                let raw_x = self.pop_u32_coerce()? as i32;
                let (x, y) = self.transform_point(raw_x, raw_y);
                let color = self.ui_state.color;
                self.push_draw(UiDrawCmd::Circle { x, y, r, color })?;
                let d = (r * 2) as f32;
                self.push_sdf_draw(matterstream_common::SdfDrawCmd {
                    pos: [(x - r as i32) as f32, (y - r as i32) as f32], size: [d, d],
                    color: matterstream_common::color_u32_to_f32(color),
                    params: [matterstream_common::DRAW_TYPE_CIRCLE, r as f32, 0.0, 0.0],
                });
            }),
            // 0x04 Text
            0x04 => ui_op!(self, pops: 4, payload: 0, {
                let slot = self.pop_u32_coerce()?;
                let size = self.pop_u32_coerce()?;
                let raw_y = self.pop_u32_coerce()? as i32;
                let raw_x = self.pop_u32_coerce()? as i32;
                let (x, y) = self.transform_point(raw_x, raw_y);
                let color = self.ui_state.color;
                self.push_draw(UiDrawCmd::Text { x, y, size, slot, color })?;
                self.push_sdf_draw(matterstream_common::SdfDrawCmd {
                    pos: [x as f32, y as f32], size: [(size * 4) as f32, size as f32],
                    color: matterstream_common::color_u32_to_f32(color),
                    params: [matterstream_common::DRAW_TYPE_TEXT, 0.0, 0.0, slot as f32],
                });
            }),
            // 0x05 PushState
            0x05 => ui_op!(self, pops: 0, payload: 0, {
                if self.ui_state_stack.len() >= UI_STATE_STACK_MAX {
                    return Err(RpnError::UiStateStackOverflow);
                }
                self.ui_state_stack.push(self.ui_state);
                let current = *self.current_transform();
                self.transform_stack.push(current);
            }),
            // 0x06 PopState
            0x06 => ui_op!(self, pops: 0, payload: 0, {
                self.ui_state = self.ui_state_stack.pop()
                    .ok_or(RpnError::UiStateStackUnderflow)?;
                if self.transform_stack.len() > 1 {
                    self.transform_stack.pop();
                }
            }),
            // 0x07 ApplyOffset
            0x07 => ui_op!(self, pops: 2, payload: 0, {
                let dy = self.pop_u32_coerce()? as i32;
                let dx = self.pop_u32_coerce()? as i32;
                if let Some(top) = self.transform_stack.last_mut() {
                    top[12] += dx as f32;
                    top[13] += dy as f32;
                }
            }),
            // 0x08 Line
            0x08 => ui_op!(self, pops: 4, payload: 0, {
                let raw_y2 = self.pop_u32_coerce()? as i32;
                let raw_x2 = self.pop_u32_coerce()? as i32;
                let raw_y1 = self.pop_u32_coerce()? as i32;
                let raw_x1 = self.pop_u32_coerce()? as i32;
                let (x1, y1) = self.transform_point(raw_x1, raw_y1);
                let (x2, y2) = self.transform_point(raw_x2, raw_y2);
                let color = self.ui_state.color;
                self.push_draw(UiDrawCmd::Line { x1, y1, x2, y2, color })?;
                self.push_sdf_draw(matterstream_common::SdfDrawCmd {
                    pos: [x1 as f32, y1 as f32], size: [x2 as f32, y2 as f32],
                    color: matterstream_common::color_u32_to_f32(color),
                    params: [matterstream_common::DRAW_TYPE_LINE, 0.0, 0.0, 0.0],
                });
            }),
            // 0x09 TextStr — pops [x, y, size, bank_id, str_idx]
            // bank_id: 0 = string_table (immutable), 1 = string_bank (mutable)
            0x09 => ui_op!(self, pops: 5, payload: 0, {
                let str_idx = self.pop_u32_coerce()?;
                let bank_id = self.pop_u32_coerce()?;
                let size = self.pop_u32_coerce()?;
                let raw_y = self.pop_u32_coerce()? as i32;
                let raw_x = self.pop_u32_coerce()? as i32;
                let (x, y) = self.transform_point(raw_x, raw_y);
                let color = self.ui_state.color;
                // Resolve the string for UiDrawCmd (legacy path uses str_idx into combined view)
                self.push_draw(UiDrawCmd::TextStr { x, y, size, str_idx, color })?;
                self.push_sdf_draw(matterstream_common::SdfDrawCmd {
                    pos: [x as f32, y as f32], size: [(size * 4) as f32, size as f32],
                    color: matterstream_common::color_u32_to_f32(color),
                    params: [matterstream_common::DRAW_TYPE_TEXT, bank_id as f32, 0.0, str_idx as f32],
                });
            }),
            // 0x0A Action
            0x0A => ui_op!(self, pops: 5, payload: 0, {
                let str_idx = self.pop_u32_coerce()?;
                let h = self.pop_u32_coerce()?;
                let w = self.pop_u32_coerce()?;
                let raw_y = self.pop_u32_coerce()? as i32;
                let raw_x = self.pop_u32_coerce()? as i32;
                let (x, y) = self.transform_point(raw_x, raw_y);
                self.push_draw(UiDrawCmd::Action { x, y, w, h, str_idx })?;
            }),
            // 0x0B ApplyMatrix
            0x0B => ui_op!(self, pops: 16, payload: 0, {
                let mut m = [0.0f32; 16];
                for i in (0..16).rev() {
                    m[i] = f32::from_bits(self.pop_u32_coerce()?);
                }
                if let Some(top) = self.transform_stack.last_mut() {
                    *top = mat4_multiply(top, &m);
                }
            }),
            // 0x0C ReplaceOffset
            0x0C => ui_op!(self, pops: 2, payload: 0, {
                let dy = self.pop_u32_coerce()? as i32;
                let dx = self.pop_u32_coerce()? as i32;
                if let Some(top) = self.transform_stack.last_mut() {
                    *top = MAT4_IDENTITY;
                    top[12] = dx as f32;
                    top[13] = dy as f32;
                }
            }),
            // 0x0D ReplaceMatrix
            0x0D => ui_op!(self, pops: 16, payload: 0, {
                let mut m = [0.0f32; 16];
                for i in (0..16).rev() {
                    m[i] = f32::from_bits(self.pop_u32_coerce()?);
                }
                if let Some(top) = self.transform_stack.last_mut() {
                    *top = m;
                }
            }),
            // 0x0E RibbonBegin — pops 7: x, y, w, h, scroll_slot, dir, card_width
            0x0E => ui_op!(self, pops: 7, payload: 0, {
                let card_width = self.pop_u32_coerce()? as f32;
                let scroll_dir = self.pop_u32_coerce()? as f32;
                let scroll_slot = self.pop_u32_coerce()? as f32;
                let h = self.pop_u32_coerce()? as f32;
                let w = self.pop_u32_coerce()? as f32;
                let raw_y = self.pop_u32_coerce()? as i32;
                let raw_x = self.pop_u32_coerce()? as i32;
                let (x, y) = self.transform_point(raw_x, raw_y);
                self.push_sdf_draw(matterstream_common::SdfDrawCmd {
                    pos: [x as f32, y as f32],
                    size: [w, h],
                    color: [0.0; 4],
                    params: [matterstream_common::DRAW_TYPE_RIBBON_BEGIN, scroll_slot, scroll_dir, card_width],
                });
            }),
            // 0x0F RibbonEnd — pops 0
            0x0F => ui_op!(self, pops: 0, payload: 0, {
                self.push_sdf_draw(matterstream_common::SdfDrawCmd {
                    pos: [0.0; 2],
                    size: [0.0; 2],
                    color: [0.0; 4],
                    params: [matterstream_common::DRAW_TYPE_RIBBON_END, 0.0, 0.0, 0.0],
                });
            }),
            _ => {
                // Unknown MTUI sub-op: skip
                self.pc += 1;
            }
        }
        Ok(())
    }

    /// OR page: VQL0 sub-ops
    /// 0x00 BeginQuery, 0x01 EndQuery, 0x02 Bind,
    /// 0x03 SetField, 0x04 SetFieldStr, 0x05 Filter,
    /// 0x06 Project, 0x07 Param
    fn dispatch_or_vql(&mut self, sub_op: u8) -> Result<(), RpnError> {
        match sub_op {
            // 0x00 BeginQuery
            0x00 => {
                if self.vql_outputs.len() >= VQL_OUTPUT_MAX {
                    return Err(RpnError::VqlLimitExceeded);
                }
                self.vql_active = Some(VqlOutput::new());
                self.pc += 1;
            }
            // 0x01 EndQuery
            0x01 => {
                let query = self.vql_active.take().ok_or(RpnError::VqlNoActiveQuery)?;
                self.vql_outputs.push(query);
                self.pc += 1;
            }
            // 0x02 Bind
            0x02 => {
                let val_idx = self.pop_u32_coerce()?;
                let key_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(key_idx)?;
                let value = self.resolve_str(val_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::Bind { name, value });
                self.pc += 1;
            }
            // 0x03 SetField
            0x03 => {
                let value = self.pop_u64()?;
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::FieldValue { name, value });
                self.pc += 1;
            }
            // 0x04 SetFieldStr
            0x04 => {
                let val_idx = self.pop_u32_coerce()?;
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let value = self.resolve_str(val_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::FieldStr { name, value });
                self.pc += 1;
            }
            // 0x05 Filter
            0x05 => {
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::Filter(name));
                self.pc += 1;
            }
            // 0x06 Project
            0x06 => {
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::Project(name));
                self.pc += 1;
            }
            // 0x07 Param
            0x07 => {
                let val_idx = self.pop_u32_coerce()?;
                let key_idx = self.pop_u32_coerce()?;
                let key = self.resolve_str(key_idx)?;
                let value = self.resolve_str(val_idx)?;
                let query = self.vql_active.as_mut().ok_or(RpnError::VqlNoActiveQuery)?;
                query.fields.push(VqlField::Param { key, value });
                self.pc += 1;
            }
            _ => {
                self.pc += 1;
            }
        }
        Ok(())
    }

    /// OR page: SKLL sub-ops
    /// OR page: OBJT sub-ops
    /// 0x00 ObjTypeBegin, 0x01 ObjTypeEnd, 0x02 ObjTypeSetShortDesc,
    /// 0x03 ObjTypeSetLongDesc, 0x04 ObjTypeField
    fn dispatch_or_objt(&mut self, sub_op: u8) -> Result<(), RpnError> {
        match sub_op {
            // 0x00 ObjTypeBegin
            0x00 => {
                if self.objtype_outputs.len() >= OBJECT_TYPE_MAX {
                    return Err(RpnError::ObjTypeLimitExceeded);
                }
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                self.objtype_active = Some(ObjectTypeDef::new(name));
                self.pc += 1;
            }
            // 0x01 ObjTypeEnd
            0x01 => {
                let def = self.objtype_active.take().ok_or(RpnError::ObjTypeNoActiveDef)?;
                self.objtype_outputs.push(def);
                self.pc += 1;
            }
            // 0x02 ObjTypeSetShortDesc
            0x02 => {
                let str_idx = self.pop_u32_coerce()?;
                let desc = self.resolve_str(str_idx)?;
                let def = self.objtype_active.as_mut().ok_or(RpnError::ObjTypeNoActiveDef)?;
                def.short_description = desc;
                self.pc += 1;
            }
            // 0x03 ObjTypeSetLongDesc
            0x03 => {
                let str_idx = self.pop_u32_coerce()?;
                let desc = self.resolve_str(str_idx)?;
                let def = self.objtype_active.as_mut().ok_or(RpnError::ObjTypeNoActiveDef)?;
                def.long_description = desc;
                self.pc += 1;
            }
            // 0x04 ObjTypeField
            0x04 => {
                let flags = self.pop_u32_coerce()?;
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                let def = self.objtype_active.as_mut().ok_or(RpnError::ObjTypeNoActiveDef)?;
                def.fields.push(ObjectFieldDef {
                    name,
                    fts: flags & 1 != 0,
                    vec: flags & 2 != 0,
                });
                self.pc += 1;
            }
            _ => {
                self.pc += 1;
            }
        }
        Ok(())
    }

    /// OR page: CARD sub-ops
    /// 0x00 CardBegin, 0x01 CardEnd, 0x02 CardSetShortDesc,
    /// 0x03 CardSetLongDesc
    fn dispatch_or_card(&mut self, sub_op: u8) -> Result<(), RpnError> {
        match sub_op {
            // 0x00 CardBegin
            0x00 => ui_op!(self, pops: 1, payload: 0, {
                if self.card_outputs.len() >= CARD_DEF_MAX {
                    return Err(RpnError::CardLimitExceeded);
                }
                let name_idx = self.pop_u32_coerce()?;
                let name = self.resolve_str(name_idx)?;
                self.card_active = Some(CardDef::new(name));
                self.card_capturing = true;
            }),
            // 0x01 CardEnd
            0x01 => ui_op!(self, pops: 0, payload: 0, {
                self.card_capturing = false;
                let mut card = self.card_active.take().ok_or(RpnError::CardNoActiveDef)?;
                card.string_table = self.string_table.clone();
                self.card_outputs.push(card);
            }),
            // 0x02 CardSetShortDesc
            0x02 => ui_op!(self, pops: 1, payload: 0, {
                let str_idx = self.pop_u32_coerce()?;
                let desc = self.resolve_str(str_idx)?;
                let card = self.card_active.as_mut().ok_or(RpnError::CardNoActiveDef)?;
                card.short_description = desc;
            }),
            // 0x03 CardSetLongDesc
            0x03 => ui_op!(self, pops: 1, payload: 0, {
                let str_idx = self.pop_u32_coerce()?;
                let desc = self.resolve_str(str_idx)?;
                let card = self.card_active.as_mut().ok_or(RpnError::CardNoActiveDef)?;
                card.long_description = desc;
            }),
            _ => {
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
    /// Note: OR page opcodes (0x80+) are not decoded as RpnOp variants.
    pub fn decode(bytecode: &[u8]) -> Result<Vec<(RpnOp, Vec<u8>)>, RpnError> {
        let mut result = Vec::new();
        let mut pc = 0;

        while pc < bytecode.len() {
            let op_byte = bytecode[pc];
            if op_byte >= 0x80 {
                // OR page opcodes have no payload at the bytecode level
                result.push((RpnOp::Nop, vec![op_byte]));
                pc += 1;
                continue;
            }
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
