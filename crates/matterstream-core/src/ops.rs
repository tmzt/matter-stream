//! Op definitions, OpsHeader, and RSI pointers.

use matterstream_vm_arena::dmove::DmoveDescriptor;
use matterstream_vm_addressing::fqa::{Fqa, FourCC, Ordinal};
use crate::tier3::ResourceHandle;

/// Register State Index pointer — resolves to a specific register in a bank/tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsiPointer {
    pub tier: u8,
    pub bank: u8,
    pub index: u8,
}

impl RsiPointer {
    pub fn new(tier: u8, bank: u8, index: u8) -> Self {
        Self { tier, bank, index }
    }
}

/// Header for a compiled op sequence — carries RSI pointers and optimization flags.
#[derive(Debug, Clone)]
pub struct OpsHeader {
    pub rsi_pointers: Vec<RsiPointer>,
    pub translation_only: bool,
}

impl OpsHeader {
    pub fn new(rsi_pointers: Vec<RsiPointer>, translation_only: bool) -> Self {
        Self {
            rsi_pointers,
            translation_only,
        }
    }
}

/// Primitive types for draw calls.
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    Slab,
    Text,
}

/// An individual instruction in the MatterStream ISA.
#[derive(Debug, Clone)]
pub enum Op {
    /// Draw a primitive, resolving position via RSI pointer index.
    Draw { primitive: Primitive, position_rsi: usize },
    /// Set translation (vec3) — fast path (12 bytes).
    SetTrans([f32; 3]),
    /// Set full matrix (mat4) — slow path (64 bytes).
    SetMatrix([f32; 16]),
    /// Set color (vec4).
    SetColor([f32; 4]),
    /// Set size (vec2) — width and height in NDC units.
    SetSize([f32; 2]),
    /// Push projection stack (saves Mat4 bank only).
    PushProj,
    /// Pop projection stack (restores Mat4 bank only).
    PopProj,
    /// Push full state stack (saves all register banks).
    PushState,
    /// Pop full state stack (restores all register banks).
    PopState,
    /// Bind a zero page region.
    BindZeroPage { offset: u8, len: u8 },
    /// Bind a resource handle.
    BindResource(ResourceHandle),
    /// Set label text for the next draw call.
    SetLabel(String),
    /// Set pixel padding [top, right, bottom, left] for the next draw call.
    SetPadding([f32; 4]),
    /// Set text color (RGBA) for nested text within a slab.
    SetTextColor([f32; 4]),
    /// Push raw bytes to the stream.
    Push(Vec<u8>),

    // -- VM_SPEC v0.1.0 ops --

    /// Resolve an FQA through the addressing pipeline.
    ResolveFqa(Fqa),
    /// Trigger arena SYNC (swap active/staging).
    Sync,
    /// Execute RPN bytecode.
    ExecRpn(Vec<u8>),
    /// Execute a batch of DMOVE descriptors.
    Dmove(Vec<DmoveDescriptor>),
    /// Load an archive member by ordinal + FourCC.
    LoadArchiveMember { ordinal: Ordinal, fourcc: FourCC },
}

/// Result of executing a Draw op.
#[derive(Debug, Clone)]
pub struct Draw {
    pub primitive: Primitive,
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub size: [f32; 2],
    pub label: Option<String>,
    pub padding: [f32; 4],
    pub text_color: Option<[f32; 4]>,
    pub used_fast_path: bool,
    pub transform_bytes: usize,
}

/// A compiled op sequence with header metadata.
#[derive(Debug, Clone)]
pub struct CompiledOps {
    pub header: OpsHeader,
    pub ops: Vec<Op>,
}

impl CompiledOps {
    pub fn new(header: OpsHeader, ops: Vec<Op>) -> Self {
        Self { header, ops }
    }
}
