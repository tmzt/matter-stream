//! Op definitions for the MatterStream ISA.

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
    pub size: [f32; 3], // Add this line
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
