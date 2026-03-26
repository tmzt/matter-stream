//! Token-based bytecode assembler for the msm1 RPN VM.
//!
//! Two-phase system:
//! 1. Emission: append typed `AsmToken` variants to an IR buffer.
//!    Labels, globals, strings are opaque typed IDs.
//! 2. Resolution (`finish()`): labels → byte offsets, strings → table indices.

use matterstream_vm::hooks::StateSlot;
use matterstream_vm::rpn::RpnOp;
// FourCC constants imported but only used by callers via the sub-op modules
#[allow(unused_imports)]
use matterstream_vm::ui_vm::{FOURCC_MTUI, FOURCC_VQL0, FOURCC_SKLL};
#[allow(unused_imports)]
use matterstream_vm::rpn::{FOURCC_OBJT, FOURCC_CARD};
use std::collections::HashMap;
use std::fmt;

/// Opaque label handle (typed index into label table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LabelId(pub u32);

/// Opaque global-slot handle (typed index into global table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlobalId(pub u32);

/// Opaque string-table handle (typed index into string table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringId(pub u32);

// ── OR page sub-opcodes ────────────────────────────────────────────────

/// MTUI OR page sub-opcodes (emitted as 0x80 + offset).
pub mod mtui {
    pub const SET_COLOR: u8 = 0x80;
    pub const BOX: u8 = 0x81;
    pub const SLAB: u8 = 0x82;
    pub const CIRCLE: u8 = 0x83;
    pub const TEXT: u8 = 0x84;
    pub const PUSH_STATE: u8 = 0x85;
    pub const POP_STATE: u8 = 0x86;
    pub const APPLY_OFFSET: u8 = 0x87;
    pub const LINE: u8 = 0x88;
    pub const TEXT_STR: u8 = 0x89;
    pub const ACTION: u8 = 0x8A;
    pub const APPLY_MATRIX: u8 = 0x8B;
    pub const REPLACE_OFFSET: u8 = 0x8C;
    pub const REPLACE_MATRIX: u8 = 0x8D;
}

pub mod vql {
    pub const BEGIN_QUERY: u8 = 0x80;
    pub const END_QUERY: u8 = 0x81;
    pub const BIND: u8 = 0x82;
    pub const SET_FIELD: u8 = 0x83;
    pub const SET_FIELD_STR: u8 = 0x84;
    pub const FILTER: u8 = 0x85;
    pub const PROJECT: u8 = 0x86;
    pub const PARAM: u8 = 0x87;
}

pub mod skll {
    pub const BEGIN: u8 = 0x80;
    pub const END: u8 = 0x81;
    pub const STEP: u8 = 0x82;
    pub const LLM_STEP: u8 = 0x83;
    pub const REPLACEABLE: u8 = 0x84;
    pub const INVOKE: u8 = 0x85;
    pub const INVOKE_SYMBOL: u8 = 0x86;
    pub const LLM_MODEL: u8 = 0x87;
    pub const LLM_USE_CASE: u8 = 0x88;
    pub const SET_SHORT_DESC: u8 = 0x89;
    pub const SET_LONG_DESC: u8 = 0x8A;
    pub const CRON_INTERVAL: u8 = 0x8B;
    pub const CRON_JITTER: u8 = 0x8C;
    pub const FORWARD_PROMPT: u8 = 0x8D;
    pub const ADD_TO_SYSTEM_PROMPT: u8 = 0x8E;
}

pub mod objt {
    pub const BEGIN: u8 = 0x80;
    pub const END: u8 = 0x81;
    pub const SET_SHORT_DESC: u8 = 0x82;
    pub const SET_LONG_DESC: u8 = 0x83;
    pub const FIELD: u8 = 0x84;
}

pub mod card {
    pub const BEGIN: u8 = 0x80;
    pub const END: u8 = 0x81;
    pub const SET_SHORT_DESC: u8 = 0x82;
    pub const SET_LONG_DESC: u8 = 0x83;
}

// ── UserCall sub-op IDs ────────────────────────────────────────────────

pub mod user_call {
    pub const EV_POLL: u64 = 0x00;
    pub const EV_HAS_EVENT: u64 = 0x01;
    pub const FRAME_COUNT: u64 = 0x02;
    pub const RAND: u64 = 0x03;
    pub const OID_IMPORT: u64 = 0x10;
    pub const OID_CALL: u64 = 0x11;
    pub const OID_COSINE_MATCH: u64 = 0x12;
    pub const READ_USER_ATOMIC: u64 = 0x20;
    pub const SUBMIT_USER_SEMAPHORE: u64 = 0x21;
    pub const SHARED_STRING_GET: u64 = 0x22;
    pub const SHARED_STRING_SET: u64 = 0x23;
}

// ── SystemCall sub-op IDs ──────────────────────────────────────────────

pub mod system_call {
    pub const SYNC: u64 = 0x05;
    pub const SET_OUTPUT_MODE: u64 = 0x07;
}

/// A single token in the assembler IR.
#[derive(Debug, Clone)]
enum AsmToken {
    Op(RpnOp),
    /// Raw byte (for OR page opcodes that aren't in the RpnOp enum).
    RawByte(u8),
    Push32(u32),
    Push64(u64),
    Push128(u128),
    Label(LabelId),
    Jmp(LabelId),
    JmpIf(LabelId),
    LoadBank(StateSlot),
    StoreBank(StateSlot),
    /// Push a string table index as u32 (generic, used by VQL/SKLL/UI ops).
    StrRef(StringId),
    /// SetCR: [opcode][u8 cr_idx][u64 value] — 10 bytes total.
    SetCR(u8, u64),
    /// UserCall: [opcode][u64 action_op][u64 data] — 17 bytes total.
    UserCall(u64, u64),
    /// SystemCall: [opcode][u64 action_op][u64 data] — 17 bytes total.
    SystemCall(u64, u64),
}

impl AsmToken {
    fn byte_size(&self) -> usize {
        match self {
            AsmToken::Op(op) => 1 + op.payload_size(),
            AsmToken::RawByte(_) => 1,
            AsmToken::Push32(_) => 5,
            AsmToken::Push64(_) => 9,
            AsmToken::Push128(_) => 17,
            AsmToken::Label(_) => 0,
            AsmToken::Jmp(_) => 9,
            AsmToken::JmpIf(_) => 9,
            AsmToken::LoadBank(_) => 11,  // Push32(bank) + Push32(index) + LoadBank
            AsmToken::StoreBank(_) => 11,
            AsmToken::StrRef(_) => 5,     // Push32(str_idx)
            AsmToken::SetCR(_, _) => 10,  // opcode + u8 + u64
            AsmToken::UserCall(_, _) => 17,   // opcode + u64 + u64
            AsmToken::SystemCall(_, _) => 17,
        }
    }
}

/// Assembler errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsmError {
    UnresolvedLabel(LabelId),
    DuplicateLabel(LabelId),
}

impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AsmError::UnresolvedLabel(id) => write!(f, "unresolved label: {:?}", id),
            AsmError::DuplicateLabel(id) => write!(f, "duplicate label: {:?}", id),
        }
    }
}

/// Assembled output.
#[derive(Debug)]
pub struct AsmOutput {
    pub bytecode: Vec<u8>,
    pub string_table: Vec<String>,
    pub global_count: u32,
}

/// Token-based bytecode assembler.
pub struct Asm {
    tokens: Vec<AsmToken>,
    label_count: u32,
    global_count: u32,
    string_table: Vec<String>,
}

impl Asm {
    pub fn new() -> Self {
        Self {
            tokens: Vec::new(),
            label_count: 0,
            global_count: 0,
            string_table: Vec::new(),
        }
    }

    // ── Handle allocation ──

    pub fn def_label(&mut self) -> LabelId {
        let id = LabelId(self.label_count);
        self.label_count += 1;
        id
    }

    pub fn def_global(&mut self) -> GlobalId {
        let id = GlobalId(self.global_count);
        self.global_count += 1;
        id
    }

    pub fn def_string(&mut self, content: &str) -> StringId {
        let id = StringId(self.string_table.len() as u32);
        self.string_table.push(content.to_string());
        id
    }

    // ── Label placement & jumps ──

    pub fn mark(&mut self, id: LabelId) -> &mut Self {
        self.tokens.push(AsmToken::Label(id));
        self
    }

    pub fn jmp(&mut self, id: LabelId) -> &mut Self {
        self.tokens.push(AsmToken::Jmp(id));
        self
    }

    pub fn jmp_if(&mut self, id: LabelId) -> &mut Self {
        self.tokens.push(AsmToken::JmpIf(id));
        self
    }

    // ── Typed bank access ──

    pub fn load(&mut self, slot: StateSlot) -> &mut Self {
        self.tokens.push(AsmToken::LoadBank(slot));
        self
    }

    pub fn store(&mut self, slot: StateSlot) -> &mut Self {
        self.tokens.push(AsmToken::StoreBank(slot));
        self
    }

    // ── Literal pushes ──

    pub fn push32(&mut self, val: u32) -> &mut Self {
        self.tokens.push(AsmToken::Push32(val));
        self
    }

    pub fn push64(&mut self, val: u64) -> &mut Self {
        self.tokens.push(AsmToken::Push64(val));
        self
    }

    pub fn push128(&mut self, val: u128) -> &mut Self {
        self.tokens.push(AsmToken::Push128(val));
        self
    }

    /// Backward compatibility alias.
    pub fn push_fqa(&mut self, val: u128) -> &mut Self {
        self.push128(val)
    }

    pub fn push_f32(&mut self, val: f32) -> &mut Self {
        self.push32(f32::to_bits(val))
    }

    // ── Raw opcodes ──

    pub fn op(&mut self, op: RpnOp) -> &mut Self {
        self.tokens.push(AsmToken::Op(op));
        self
    }

    fn raw(&mut self, byte: u8) -> &mut Self {
        self.tokens.push(AsmToken::RawByte(byte));
        self
    }

    pub fn add(&mut self) -> &mut Self { self.op(RpnOp::Add) }
    pub fn sub(&mut self) -> &mut Self { self.op(RpnOp::Sub) }
    pub fn mul(&mut self) -> &mut Self { self.op(RpnOp::Mul) }
    pub fn div(&mut self) -> &mut Self { self.op(RpnOp::Div) }
    pub fn mod_(&mut self) -> &mut Self { self.op(RpnOp::Mod) }
    pub fn dup(&mut self) -> &mut Self { self.op(RpnOp::Dup) }
    pub fn drop_(&mut self) -> &mut Self { self.op(RpnOp::Drop) }
    pub fn swap(&mut self) -> &mut Self { self.op(RpnOp::Swap) }
    pub fn cmp_eq(&mut self) -> &mut Self { self.op(RpnOp::CmpEq) }
    pub fn cmp_lt(&mut self) -> &mut Self { self.op(RpnOp::CmpLt) }
    pub fn cmp_gt(&mut self) -> &mut Self { self.op(RpnOp::CmpGt) }
    pub fn cmp_ge(&mut self) -> &mut Self { self.op(RpnOp::CmpGe) }
    pub fn cmp_le(&mut self) -> &mut Self { self.op(RpnOp::CmpLe) }
    pub fn cmp_ne(&mut self) -> &mut Self { self.op(RpnOp::CmpNe) }
    pub fn and(&mut self) -> &mut Self { self.op(RpnOp::And) }
    pub fn or(&mut self) -> &mut Self { self.op(RpnOp::Or) }
    pub fn xor(&mut self) -> &mut Self { self.op(RpnOp::Xor) }
    pub fn shl(&mut self) -> &mut Self { self.op(RpnOp::Shl) }
    pub fn shr(&mut self) -> &mut Self { self.op(RpnOp::Shr) }
    pub fn not(&mut self) -> &mut Self { self.op(RpnOp::Not) }
    pub fn halt(&mut self) -> &mut Self { self.op(RpnOp::Halt) }
    pub fn nop(&mut self) -> &mut Self { self.op(RpnOp::Nop) }
    pub fn ret(&mut self) -> &mut Self { self.op(RpnOp::Ret) }

    pub fn fadd(&mut self) -> &mut Self { self.op(RpnOp::FAdd) }
    pub fn fsub(&mut self) -> &mut Self { self.op(RpnOp::FSub) }
    pub fn fmul(&mut self) -> &mut Self { self.op(RpnOp::FMul) }
    pub fn fdiv(&mut self) -> &mut Self { self.op(RpnOp::FDiv) }
    pub fn fcmp_gt(&mut self) -> &mut Self { self.op(RpnOp::FCmpGt) }
    pub fn fcmp_lt(&mut self) -> &mut Self { self.op(RpnOp::FCmpLt) }
    pub fn fcmp_eq(&mut self) -> &mut Self { self.op(RpnOp::FCmpEq) }
    pub fn fneg(&mut self) -> &mut Self { self.op(RpnOp::FNeg) }
    pub fn fabs(&mut self) -> &mut Self { self.op(RpnOp::FAbs) }
    pub fn i2f(&mut self) -> &mut Self { self.op(RpnOp::I2F) }
    pub fn f2i(&mut self) -> &mut Self { self.op(RpnOp::F2I) }

    // ── UserCall helpers (0x60) ──

    pub fn ev_poll(&mut self) -> &mut Self {
        self.tokens.push(AsmToken::UserCall(user_call::EV_POLL, 0));
        self
    }
    pub fn ev_has_event(&mut self) -> &mut Self {
        self.tokens.push(AsmToken::UserCall(user_call::EV_HAS_EVENT, 0));
        self
    }
    pub fn frame_count(&mut self) -> &mut Self {
        self.tokens.push(AsmToken::UserCall(user_call::FRAME_COUNT, 0));
        self
    }
    pub fn rand(&mut self) -> &mut Self {
        self.tokens.push(AsmToken::UserCall(user_call::RAND, 0));
        self
    }

    /// Read from UserAtomicReadable[slot] → pushes u32 to stack.
    pub fn read_user_atomic(&mut self, slot: u32) -> &mut Self {
        self.push32(slot);
        self.tokens.push(AsmToken::UserCall(user_call::READ_USER_ATOMIC, 0));
        self
    }

    /// Submit a value to UserAtomicSubmitSemaphore[slot] (fire-and-forget).
    pub fn submit_user_semaphore(&mut self, slot: u32, value: u32) -> &mut Self {
        self.push32(slot);
        self.push32(value);
        self.tokens.push(AsmToken::UserCall(user_call::SUBMIT_USER_SEMAPHORE, 0));
        self
    }

    /// Get shared string[shared_slot] → string_bank[local_slot]. Mutex-protected full copy.
    pub fn shared_string_get(&mut self, shared_slot: u32, local_slot: u32) -> &mut Self {
        self.push32(shared_slot);
        self.push32(local_slot);
        self.tokens.push(AsmToken::UserCall(user_call::SHARED_STRING_GET, 0));
        self
    }

    /// Set shared string[shared_slot] from string_bank[local_slot]. Mutex-protected full copy.
    pub fn shared_string_set(&mut self, local_slot: u32, shared_slot: u32) -> &mut Self {
        self.push32(local_slot);
        self.push32(shared_slot);
        self.tokens.push(AsmToken::UserCall(user_call::SHARED_STRING_SET, 0));
        self
    }

    // ── SystemCall helpers (0x71) ──

    pub fn sync(&mut self) -> &mut Self {
        self.tokens.push(AsmToken::SystemCall(system_call::SYNC, 0));
        self
    }

    // ── SetCR (0x70) ──

    pub fn set_cr(&mut self, cr_index: u8, value: u64) -> &mut Self {
        self.tokens.push(AsmToken::SetCR(cr_index, value));
        self
    }

    /// Set output mode (CR[0]) via SystemCall.
    pub fn set_output_mode(&mut self, fourcc: u32) -> &mut Self {
        self.tokens.push(AsmToken::SystemCall(system_call::SET_OUTPUT_MODE, fourcc as u64));
        self
    }

    // ── ZeroPage i32 helpers ──

    pub fn load_zp_i32(&mut self, addr: u32) -> &mut Self {
        self.push32(addr);
        self.op(RpnOp::LoadZpI32)
    }

    pub fn store_zp_i32(&mut self, addr: u32) -> &mut Self {
        self.push32(addr);
        self.op(RpnOp::StoreZpI32)
    }

    // ── Component-aware bank helpers ──

    pub fn load_bank_comp(&mut self, slot: StateSlot, component: u32) -> &mut Self {
        let bank_id = slot.bank as u32;
        self.push32(bank_id);
        self.push32(slot.index);
        self.push32(component);
        self.op(RpnOp::LoadBankComp)
    }

    pub fn store_bank_comp(&mut self, slot: StateSlot, component: u32) -> &mut Self {
        let bank_id = slot.bank as u32;
        self.push32(bank_id);
        self.push32(slot.index);
        self.push32(component);
        self.op(RpnOp::StoreBankComp)
    }

    // ── UI draw helpers (MTUI OR page) ──

    pub fn set_color(&mut self, r: u8, g: u8, b: u8, a: u8) -> &mut Self {
        let rgba = (r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | a as u32;
        self.push32(rgba);
        self.raw(mtui::SET_COLOR)
    }

    pub fn draw_box(&mut self, x: i32, y: i32, w: u32, h: u32) -> &mut Self {
        self.push32(x as u32).push32(y as u32).push32(w).push32(h);
        self.raw(mtui::BOX)
    }

    pub fn draw_slab(&mut self, x: i32, y: i32, w: u32, h: u32, radius: u32) -> &mut Self {
        self.push32(x as u32).push32(y as u32).push32(w).push32(h).push32(radius);
        self.raw(mtui::SLAB)
    }

    pub fn draw_circle(&mut self, cx: i32, cy: i32, r: u32) -> &mut Self {
        self.push32(cx as u32).push32(cy as u32).push32(r);
        self.raw(mtui::CIRCLE)
    }

    /// Draw text from string table (bank_id=0, immutable).
    pub fn draw_text_str(&mut self, x: i32, y: i32, size: u32, id: StringId) -> &mut Self {
        self.push32(x as u32).push32(y as u32).push32(size);
        self.push32(0); // bank_id: 0 = string_table
        self.tokens.push(AsmToken::StrRef(id));
        self.raw(mtui::TEXT_STR)
    }

    /// Draw text from string bank (bank_id=1, mutable runtime slot).
    pub fn draw_text_bank(&mut self, x: i32, y: i32, size: u32, slot: u32) -> &mut Self {
        self.push32(x as u32).push32(y as u32).push32(size);
        self.push32(1); // bank_id: 1 = string_bank
        self.push32(slot);
        self.raw(mtui::TEXT_STR)
    }

    /// Conditional push: reads bank[slot] via packed ref, pushes true_val if != 0, else false_val.
    /// packed_ref = (bank_type as u32) << 16 | (slot as u32)
    pub fn push_if_else(&mut self, packed_ref: u32, true_val: u32, false_val: u32) -> &mut Self {
        self.push32(packed_ref).push32(true_val).push32(false_val);
        self.op(RpnOp::PushIfElse)
    }

    /// Helper: create a packed binding ref from bank type and slot.
    pub fn pack_ref(bank_type: u16, slot: u16) -> u32 {
        (bank_type as u32) << 16 | slot as u32
    }

    pub fn ui_push_state(&mut self) -> &mut Self { self.raw(mtui::PUSH_STATE) }
    pub fn ui_pop_state(&mut self) -> &mut Self { self.raw(mtui::POP_STATE) }

    pub fn ui_apply_offset(&mut self, dx: i32, dy: i32) -> &mut Self {
        self.push32(dx as u32).push32(dy as u32);
        self.raw(mtui::APPLY_OFFSET)
    }

    pub fn ui_apply_matrix(&mut self, m: &[f32; 16]) -> &mut Self {
        for &val in m.iter() { self.push32(f32::to_bits(val)); }
        self.raw(mtui::APPLY_MATRIX)
    }

    pub fn ui_replace_offset(&mut self, dx: i32, dy: i32) -> &mut Self {
        self.push32(dx as u32).push32(dy as u32);
        self.raw(mtui::REPLACE_OFFSET)
    }

    pub fn ui_replace_matrix(&mut self, m: &[f32; 16]) -> &mut Self {
        for &val in m.iter() { self.push32(f32::to_bits(val)); }
        self.raw(mtui::REPLACE_MATRIX)
    }

    pub fn draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> &mut Self {
        self.push32(x1 as u32).push32(y1 as u32).push32(x2 as u32).push32(y2 as u32);
        self.raw(mtui::LINE)
    }

    pub fn draw_action(&mut self, x: i32, y: i32, w: u32, h: u32, id: StringId) -> &mut Self {
        self.push32(x as u32).push32(y as u32).push32(w).push32(h);
        self.tokens.push(AsmToken::StrRef(id));
        self.raw(mtui::ACTION)
    }

    // ── VQL helpers (VQL0 OR page) ──

    pub fn vql_begin_query(&mut self) -> &mut Self { self.raw(vql::BEGIN_QUERY) }
    pub fn vql_end_query(&mut self) -> &mut Self { self.raw(vql::END_QUERY) }

    pub fn vql_bind(&mut self, key: StringId, value: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(key));
        self.tokens.push(AsmToken::StrRef(value));
        self.raw(vql::BIND)
    }

    pub fn vql_set_field(&mut self, name: StringId, value: u64) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(name));
        self.push64(value);
        self.raw(vql::SET_FIELD)
    }

    pub fn vql_set_field_str(&mut self, name: StringId, value: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(name));
        self.tokens.push(AsmToken::StrRef(value));
        self.raw(vql::SET_FIELD_STR)
    }

    pub fn vql_filter(&mut self, name: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(name));
        self.raw(vql::FILTER)
    }

    pub fn vql_project(&mut self, name: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(name));
        self.raw(vql::PROJECT)
    }

    pub fn vql_param(&mut self, key: StringId, value: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(key));
        self.tokens.push(AsmToken::StrRef(value));
        self.raw(vql::PARAM)
    }

    // ── SKLL helpers (SKLL OR page) ──

    pub fn skill_begin(&mut self, name: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(name));
        self.raw(skll::BEGIN)
    }

    pub fn skill_end(&mut self) -> &mut Self { self.raw(skll::END) }

    pub fn skill_step(&mut self, name: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(name));
        self.raw(skll::STEP)
    }

    pub fn skill_llm_step(&mut self, prompt: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(prompt));
        self.raw(skll::LLM_STEP)
    }

    pub fn skill_replaceable(&mut self, name: StringId, default: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(name));
        self.tokens.push(AsmToken::StrRef(default));
        self.raw(skll::REPLACEABLE)
    }

    pub fn skill_llm_model(&mut self, model: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(model));
        self.raw(skll::LLM_MODEL)
    }

    pub fn skill_llm_use_case(&mut self, use_case: u8) -> &mut Self {
        self.push32(use_case as u32);
        self.raw(skll::LLM_USE_CASE)
    }

    pub fn skill_invoke(&mut self, name: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(name));
        self.raw(skll::INVOKE)
    }

    pub fn skill_invoke_symbol(&mut self, symbol: u32) -> &mut Self {
        self.push32(symbol);
        self.raw(skll::INVOKE_SYMBOL)
    }

    pub fn skill_forward_prompt(&mut self, dest: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(dest));
        self.raw(skll::FORWARD_PROMPT)
    }

    pub fn skill_add_to_system_prompt(&mut self, content: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(content));
        self.raw(skll::ADD_TO_SYSTEM_PROMPT)
    }

    pub fn skill_set_short_desc(&mut self, desc: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(desc));
        self.raw(skll::SET_SHORT_DESC)
    }

    pub fn skill_set_long_desc(&mut self, desc: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(desc));
        self.raw(skll::SET_LONG_DESC)
    }

    pub fn skill_cron_interval(&mut self, interval_ms: u64) -> &mut Self {
        self.push32(interval_ms as u32);
        self.push32((interval_ms >> 32) as u32);
        self.raw(skll::CRON_INTERVAL)
    }

    pub fn skill_cron_jitter(&mut self, jitter_ms: u64) -> &mut Self {
        self.push32(jitter_ms as u32);
        self.push32((jitter_ms >> 32) as u32);
        self.raw(skll::CRON_JITTER)
    }

    // ── Card helpers (CARD OR page) ──

    pub fn card_begin(&mut self, name: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(name));
        self.raw(card::BEGIN)
    }

    pub fn card_end(&mut self) -> &mut Self { self.raw(card::END) }

    pub fn card_set_short_desc(&mut self, desc: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(desc));
        self.raw(card::SET_SHORT_DESC)
    }

    pub fn card_set_long_desc(&mut self, desc: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(desc));
        self.raw(card::SET_LONG_DESC)
    }

    // ── Object type helpers (OBJT OR page) ──

    pub fn objtype_begin(&mut self, name: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(name));
        self.raw(objt::BEGIN)
    }

    pub fn objtype_end(&mut self) -> &mut Self { self.raw(objt::END) }

    pub fn objtype_set_short_desc(&mut self, desc: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(desc));
        self.raw(objt::SET_SHORT_DESC)
    }

    pub fn objtype_set_long_desc(&mut self, desc: StringId) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(desc));
        self.raw(objt::SET_LONG_DESC)
    }

    pub fn objtype_field(&mut self, name: StringId, flags: u32) -> &mut Self {
        self.tokens.push(AsmToken::StrRef(name));
        self.push32(flags);
        self.raw(objt::FIELD)
    }

    // ── Finalization ──

    pub fn finish(self) -> Result<AsmOutput, AsmError> {
        // Pass 1: compute byte offsets and collect label positions
        let mut label_offsets: HashMap<LabelId, usize> = HashMap::new();
        let mut offset = 0usize;

        for token in &self.tokens {
            if let AsmToken::Label(id) = token {
                if label_offsets.contains_key(id) {
                    return Err(AsmError::DuplicateLabel(*id));
                }
                label_offsets.insert(*id, offset);
            }
            offset += token.byte_size();
        }

        // Pass 2: emit bytecode
        let mut bytecode = Vec::with_capacity(offset);

        for token in &self.tokens {
            match token {
                AsmToken::Op(op) => {
                    bytecode.push(*op as u8);
                }
                AsmToken::RawByte(b) => {
                    bytecode.push(*b);
                }
                AsmToken::Push32(val) => {
                    bytecode.push(RpnOp::Push32 as u8);
                    bytecode.extend_from_slice(&val.to_le_bytes());
                }
                AsmToken::Push64(val) => {
                    bytecode.push(RpnOp::Push64 as u8);
                    bytecode.extend_from_slice(&val.to_le_bytes());
                }
                AsmToken::Push128(val) => {
                    bytecode.push(RpnOp::Push128 as u8);
                    bytecode.extend_from_slice(&val.to_le_bytes());
                }
                AsmToken::Label(_) => {}
                AsmToken::Jmp(id) => {
                    let target = *label_offsets.get(id).ok_or(AsmError::UnresolvedLabel(*id))?;
                    bytecode.push(RpnOp::Jmp as u8);
                    bytecode.extend_from_slice(&(target as u64).to_le_bytes());
                }
                AsmToken::JmpIf(id) => {
                    let target = *label_offsets.get(id).ok_or(AsmError::UnresolvedLabel(*id))?;
                    bytecode.push(RpnOp::JmpIf as u8);
                    bytecode.extend_from_slice(&(target as u64).to_le_bytes());
                }
                AsmToken::LoadBank(slot) => {
                    let bank_id = slot.bank as u32;
                    bytecode.push(RpnOp::Push32 as u8);
                    bytecode.extend_from_slice(&bank_id.to_le_bytes());
                    bytecode.push(RpnOp::Push32 as u8);
                    bytecode.extend_from_slice(&slot.index.to_le_bytes());
                    bytecode.push(RpnOp::LoadBank as u8);
                }
                AsmToken::StoreBank(slot) => {
                    let bank_id = slot.bank as u32;
                    bytecode.push(RpnOp::Push32 as u8);
                    bytecode.extend_from_slice(&bank_id.to_le_bytes());
                    bytecode.push(RpnOp::Push32 as u8);
                    bytecode.extend_from_slice(&slot.index.to_le_bytes());
                    bytecode.push(RpnOp::StoreBank as u8);
                }
                AsmToken::StrRef(id) => {
                    bytecode.push(RpnOp::Push32 as u8);
                    bytecode.extend_from_slice(&id.0.to_le_bytes());
                }
                AsmToken::SetCR(cr_idx, value) => {
                    bytecode.push(RpnOp::SetCR as u8);
                    bytecode.push(*cr_idx);
                    bytecode.extend_from_slice(&value.to_le_bytes());
                }
                AsmToken::UserCall(action_op, data) => {
                    bytecode.push(RpnOp::UserCall as u8);
                    bytecode.extend_from_slice(&action_op.to_le_bytes());
                    bytecode.extend_from_slice(&data.to_le_bytes());
                }
                AsmToken::SystemCall(action_op, data) => {
                    bytecode.push(RpnOp::SystemCall as u8);
                    bytecode.extend_from_slice(&action_op.to_le_bytes());
                    bytecode.extend_from_slice(&data.to_le_bytes());
                }
            }
        }

        Ok(AsmOutput {
            bytecode,
            string_table: self.string_table,
            global_count: self.global_count,
        })
    }
}

impl Default for Asm {
    fn default() -> Self {
        Self::new()
    }
}
