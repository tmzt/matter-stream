//! OVA (Object Virtual Address) -- bit-packed arena address.
//!
//! Layout: `[Arena:2][Gen:9][Object:10][Offset:11]` = 32 bits

use std::fmt;

/// Arena identifier (2 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ArenaId {
    Nursery = 0,
    DynamicA = 1,
    DynamicB = 2,
    Reserved = 3,
}

impl ArenaId {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(ArenaId::Nursery),
            1 => Some(ArenaId::DynamicA),
            2 => Some(ArenaId::DynamicB),
            3 => Some(ArenaId::Reserved),
            _ => None,
        }
    }
}

// Bit layout constants
const ARENA_BITS: u32 = 2;
const GEN_BITS: u32 = 9;
const OBJECT_BITS: u32 = 10;
const OFFSET_BITS: u32 = 11;

const OFFSET_MASK: u32 = (1 << OFFSET_BITS) - 1;
const OBJECT_MASK: u32 = (1 << OBJECT_BITS) - 1;
const GEN_MASK: u32 = (1 << GEN_BITS) - 1;
const ARENA_MASK: u32 = (1 << ARENA_BITS) - 1;

const OFFSET_SHIFT: u32 = 0;
const OBJECT_SHIFT: u32 = OFFSET_BITS;
const GEN_SHIFT: u32 = OFFSET_BITS + OBJECT_BITS;
const ARENA_SHIFT: u32 = OFFSET_BITS + OBJECT_BITS + GEN_BITS;

/// Maximum values for each field.
pub const MAX_OFFSET: u32 = (1 << OFFSET_BITS) - 1;   // 2047
pub const MAX_OBJECT: u32 = (1 << OBJECT_BITS) - 1;    // 1023
pub const MAX_GEN: u32 = (1 << GEN_BITS) - 1;          // 511
pub const MAX_ARENA: u32 = (1 << ARENA_BITS) - 1;      // 3

/// Object Virtual Address -- bit-packed `[Arena:2][Gen:9][Object:10][Offset:11]`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ova(pub u32);

impl Ova {
    pub fn new(arena: ArenaId, gen: u32, object: u32, offset: u32) -> Self {
        debug_assert!(gen <= MAX_GEN);
        debug_assert!(object <= MAX_OBJECT);
        debug_assert!(offset <= MAX_OFFSET);
        let bits = ((arena as u32 & ARENA_MASK) << ARENA_SHIFT)
            | ((gen & GEN_MASK) << GEN_SHIFT)
            | ((object & OBJECT_MASK) << OBJECT_SHIFT)
            | ((offset & OFFSET_MASK) << OFFSET_SHIFT);
        Ova(bits)
    }

    pub fn arena(&self) -> ArenaId {
        ArenaId::from_u8(((self.0 >> ARENA_SHIFT) & ARENA_MASK) as u8).unwrap()
    }

    pub fn generation(&self) -> u32 {
        (self.0 >> GEN_SHIFT) & GEN_MASK
    }

    pub fn object(&self) -> u32 {
        (self.0 >> OBJECT_SHIFT) & OBJECT_MASK
    }

    pub fn offset(&self) -> u32 {
        (self.0 >> OFFSET_SHIFT) & OFFSET_MASK
    }

    /// Rewrite the arena bits (used by SYNC).
    pub fn with_arena(&self, arena: ArenaId) -> Self {
        let cleared = self.0 & !(ARENA_MASK << ARENA_SHIFT);
        Ova(cleared | ((arena as u32 & ARENA_MASK) << ARENA_SHIFT))
    }

    /// Rewrite the offset bits.
    pub fn with_offset(&self, offset: u32) -> Self {
        let cleared = self.0 & !(OFFSET_MASK << OFFSET_SHIFT);
        Ova(cleared | ((offset & OFFSET_MASK) << OFFSET_SHIFT))
    }

    /// Bump the generation counter (wraps at MAX_GEN).
    pub fn next_generation(&self) -> Self {
        let gen = (self.generation() + 1) & GEN_MASK;
        let cleared = self.0 & !(GEN_MASK << GEN_SHIFT);
        Ova(cleared | (gen << GEN_SHIFT))
    }
}

impl fmt::Debug for Ova {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Ova(arena={:?}, gen={}, obj={}, off={})",
            self.arena(),
            self.generation(),
            self.object(),
            self.offset()
        )
    }
}

impl fmt::Display for Ova {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#010x}", self.0)
    }
}

/// Extended 64-bit OVA variant for future use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OvaWide(pub u64);

impl OvaWide {
    pub fn from_ova(ova: Ova) -> Self {
        OvaWide(ova.0 as u64)
    }

    pub fn to_ova(&self) -> Ova {
        Ova(self.0 as u32)
    }
}
