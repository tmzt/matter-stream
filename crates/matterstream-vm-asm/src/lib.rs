//! Token-based bytecode assembler for the MTSM RPN VM.
//!
//! Two-phase system:
//! 1. Emission: append typed `AsmToken` variants to an IR buffer.
//!    Labels, globals, strings are opaque typed IDs.
//! 2. Resolution (`finish()`): labels → byte offsets, strings → table indices.

use matterstream_vm::hooks::StateSlot;
use matterstream_vm::rpn::RpnOp;
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

/// A single token in the assembler IR.
#[derive(Debug, Clone)]
enum AsmToken {
    Op(RpnOp),
    Push32(u32),
    Push64(u64),
    PushFqa(u128),
    Label(LabelId),
    Jmp(LabelId),
    JmpIf(LabelId),
    LoadBank(StateSlot),
    StoreBank(StateSlot),
    UiTextStr(StringId),
}

impl AsmToken {
    /// Byte size this token will occupy in the final bytecode.
    fn byte_size(&self) -> usize {
        match self {
            AsmToken::Op(op) => 1 + op.payload_size(),
            AsmToken::Push32(_) => 5,  // opcode + 4 bytes
            AsmToken::Push64(_) => 9,  // opcode + 8 bytes
            AsmToken::PushFqa(_) => 17, // opcode + 16 bytes
            AsmToken::Label(_) => 0,   // labels are zero-width markers
            AsmToken::Jmp(_) => 9,     // opcode + 8-byte target
            AsmToken::JmpIf(_) => 9,   // opcode + 8-byte target
            AsmToken::LoadBank(_) => 5 + 5 + 1, // Push32(bank) + Push32(index) + LoadBank
            AsmToken::StoreBank(_) => 5 + 5 + 1, // Push32(bank) + Push32(index) + StoreBank
            AsmToken::UiTextStr(_) => 5, // Push32(str_idx)
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

    pub fn push_fqa(&mut self, val: u128) -> &mut Self {
        self.tokens.push(AsmToken::PushFqa(val));
        self
    }

    // ── Raw opcodes ──

    pub fn op(&mut self, op: RpnOp) -> &mut Self {
        self.tokens.push(AsmToken::Op(op));
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
    pub fn sync(&mut self) -> &mut Self { self.op(RpnOp::Sync) }
    pub fn ev_poll(&mut self) -> &mut Self { self.op(RpnOp::EvPoll) }
    pub fn ev_has_event(&mut self) -> &mut Self { self.op(RpnOp::EvHasEvent) }
    pub fn frame_count(&mut self) -> &mut Self { self.op(RpnOp::FrameCount) }
    pub fn rand(&mut self) -> &mut Self { self.op(RpnOp::Rand) }

    // ── UI draw helpers ──

    pub fn set_color(&mut self, r: u8, g: u8, b: u8, a: u8) -> &mut Self {
        let rgba = (r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | a as u32;
        self.push32(rgba);
        self.op(RpnOp::UiSetColor)
    }

    pub fn draw_box(&mut self, x: i32, y: i32, w: u32, h: u32) -> &mut Self {
        self.push32(x as u32);
        self.push32(y as u32);
        self.push32(w);
        self.push32(h);
        self.op(RpnOp::UiBox)
    }

    pub fn draw_slab(&mut self, x: i32, y: i32, w: u32, h: u32, radius: u32) -> &mut Self {
        self.push32(x as u32);
        self.push32(y as u32);
        self.push32(w);
        self.push32(h);
        self.push32(radius);
        self.op(RpnOp::UiSlab)
    }

    pub fn draw_circle(&mut self, cx: i32, cy: i32, r: u32) -> &mut Self {
        self.push32(cx as u32);
        self.push32(cy as u32);
        self.push32(r);
        self.op(RpnOp::UiCircle)
    }

    pub fn draw_text_str(&mut self, x: i32, y: i32, size: u32, id: StringId) -> &mut Self {
        self.push32(x as u32);
        self.push32(y as u32);
        self.push32(size);
        self.tokens.push(AsmToken::UiTextStr(id));
        self.op(RpnOp::UiTextStr)
    }

    pub fn ui_push_state(&mut self) -> &mut Self { self.op(RpnOp::UiPushState) }
    pub fn ui_pop_state(&mut self) -> &mut Self { self.op(RpnOp::UiPopState) }

    pub fn ui_set_offset(&mut self, dx: i32, dy: i32) -> &mut Self {
        self.push32(dx as u32);
        self.push32(dy as u32);
        self.op(RpnOp::UiSetOffset)
    }

    pub fn draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> &mut Self {
        self.push32(x1 as u32);
        self.push32(y1 as u32);
        self.push32(x2 as u32);
        self.push32(y2 as u32);
        self.op(RpnOp::UiLine)
    }

    // ── Finalization ──

    pub fn finish(self) -> Result<AsmOutput, AsmError> {
        // Pass 1: compute byte offsets and collect label positions
        let mut label_offsets: HashMap<LabelId, usize> = HashMap::new();
        let mut defined_labels: Vec<LabelId> = Vec::new();
        let mut offset = 0usize;

        for token in &self.tokens {
            if let AsmToken::Label(id) = token {
                if label_offsets.contains_key(id) {
                    return Err(AsmError::DuplicateLabel(*id));
                }
                label_offsets.insert(*id, offset);
                defined_labels.push(*id);
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
                AsmToken::Push32(val) => {
                    bytecode.push(RpnOp::Push32 as u8);
                    bytecode.extend_from_slice(&val.to_le_bytes());
                }
                AsmToken::Push64(val) => {
                    bytecode.push(RpnOp::Push64 as u8);
                    bytecode.extend_from_slice(&val.to_le_bytes());
                }
                AsmToken::PushFqa(val) => {
                    bytecode.push(RpnOp::PushFqa as u8);
                    bytecode.extend_from_slice(&val.to_le_bytes());
                }
                AsmToken::Label(_) => {
                    // Zero-width marker, no bytes emitted
                }
                AsmToken::Jmp(id) => {
                    let target = *label_offsets
                        .get(id)
                        .ok_or(AsmError::UnresolvedLabel(*id))?;
                    bytecode.push(RpnOp::Jmp as u8);
                    bytecode.extend_from_slice(&(target as u64).to_le_bytes());
                }
                AsmToken::JmpIf(id) => {
                    let target = *label_offsets
                        .get(id)
                        .ok_or(AsmError::UnresolvedLabel(*id))?;
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
                AsmToken::UiTextStr(id) => {
                    bytecode.push(RpnOp::Push32 as u8);
                    bytecode.extend_from_slice(&id.0.to_le_bytes());
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
