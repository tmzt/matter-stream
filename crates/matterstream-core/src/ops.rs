<<<<<<< HEAD
//! Op definitions, OpsHeader, and RSI pointers.

use matterstream_vm_arena::dmove::DmoveDescriptor;
use matterstream_vm_addressing::fqa::{Fqa, FourCC, Ordinal};
use crate::tier3::ResourceHandle;

/// Register Set Index pointer — locates a value across the tier/bank/index space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsiPointer {
    /// Which memory tier (0–3).
    pub tier: u8,
    /// Which bank within the tier (Tier 1: 0=MAT4, 1=VEC4, 2=VEC3, 3=SCL, 4=INT).
    pub bank: u8,
    /// Register index within the bank.
=======
//! Op definitions for the MatterStream ISA.

use crate::tier3::ResourceHandle;

/// Register State Index pointer — resolves to a specific register in a bank/tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsiPointer {
    pub tier: u8,
    pub bank: u8,
>>>>>>> 3b9a15a (Commit current work)
    pub index: u8,
}

impl RsiPointer {
    pub fn new(tier: u8, bank: u8, index: u8) -> Self {
        Self { tier, bank, index }
    }
}

<<<<<<< HEAD
/// Header preamble for an element's op sequence.
#[derive(Debug, Clone)]
pub struct OpsHeader {
    /// RSI pointers resolved during hydration.
    pub rsi_pointers: Vec<RsiPointer>,
    /// If true, element only translates — use vec3 add instead of mat4 mul.
=======
/// Header for a compiled op sequence — carries RSI pointers and optimization flags.
#[derive(Debug, Clone)]
pub struct OpsHeader {
    pub rsi_pointers: Vec<RsiPointer>,
>>>>>>> 3b9a15a (Commit current work)
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

<<<<<<< HEAD
/// Primitives that can be drawn.
#[derive(Debug, Clone)]
=======
/// Primitive types for draw calls.
#[derive(Debug, Clone, PartialEq)]
>>>>>>> 3b9a15a (Commit current work)
pub enum Primitive {
    Slab,
}

<<<<<<< HEAD
/// The instruction set.
#[derive(Debug, Clone)]
pub enum Op {
    /// Draw a primitive using current register state.
    Draw {
        /// The primitive to draw.
        primitive: Primitive,
        /// RSI pointer index for position data.
        position_rsi: usize,
    },
    /// Set translation (vec3 fast-path, 12 bytes).
    SetTrans([f32; 3]),
    /// Set full transformation matrix (64 bytes).
    SetMatrix([f32; 16]),
    /// Set color (vec4, 16 bytes).
    SetColor([f32; 4]),
    /// Push projection matrix bank (micro-stack).
    PushProj,
    /// Pop projection matrix bank (micro-stack).
    PopProj,
    /// Push full register state (full-stack).
    PushState,
    /// Pop full register state (full-stack).
    PopState,
    /// Bind a zero-page region for the current element.
    BindZeroPage {
        offset: u8,
        len: u8,
    },
    /// Bind a resource handle.
    BindResource(ResourceHandle),
    /// Push a raw byte payload onto the stream.
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

/// Result of a Draw execution — captures how position was resolved.
#[derive(Debug, Clone)]
pub struct Draw {
    /// Position resolved from registers.
    pub position: [f32; 3],
    /// Color resolved from registers.
    pub color: [f32; 4],
    /// Whether the translation fast-path was used.
    pub used_fast_path: bool,
    /// Byte cost of the transform operation.
    pub transform_bytes: usize,
}
=======
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
    /// Push raw bytes to the stream.
    Push(Vec<u8>),
}

/// Result of executing a Draw op.
#[derive(Debug, Clone)]
pub struct Draw {
    pub position: [f32; 3],
    pub color: [f32; 4],
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
>>>>>>> 3b9a15a (Commit current work)
