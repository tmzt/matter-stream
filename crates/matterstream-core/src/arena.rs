//! Triple-Arena memory: Nursery (immortal) + Dynamic A/B ping-pong + SYNC.

use crate::ova::{ArenaId, Ova, MAX_GEN, MAX_OFFSET};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArenaError {
    OutOfSpace,
    InvalidOva,
    GenerationMismatch { expected: u32, got: u32 },
    NurseryWriteViolation,
    ObjectTooLarge(usize),
}

impl fmt::Display for ArenaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArenaError::OutOfSpace => write!(f, "arena out of space"),
            ArenaError::InvalidOva => write!(f, "invalid OVA"),
            ArenaError::GenerationMismatch { expected, got } => {
                write!(f, "generation mismatch: expected {}, got {}", expected, got)
            }
            ArenaError::NurseryWriteViolation => write!(f, "nursery is write-once after initial allocation"),
            ArenaError::ObjectTooLarge(sz) => write!(f, "object too large: {} bytes (max {})", sz, MAX_OFFSET + 1),
        }
    }
}

/// A single object slot within an arena.
#[derive(Debug, Clone)]
struct ArenaObject {
    data: Vec<u8>,
    alive: bool,
    generation: u32,
}

/// A single arena with fixed-capacity object slots.
#[derive(Debug, Clone)]
struct Arena {
    objects: Vec<ArenaObject>,
    arena_id: ArenaId,
}

impl Arena {
    fn new(capacity: usize, arena_id: ArenaId) -> Self {
        let mut objects = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            objects.push(ArenaObject {
                data: Vec::new(),
                alive: false,
                generation: 0,
            });
        }
        Self {
            objects,
            arena_id,
        }
    }

    fn alloc(&mut self, size: usize) -> Result<Ova, ArenaError> {
        if size > (MAX_OFFSET as usize + 1) {
            return Err(ArenaError::ObjectTooLarge(size));
        }
        for (i, obj) in self.objects.iter_mut().enumerate() {
            if !obj.alive {
                obj.alive = true;
                obj.data = vec![0u8; size];
                let ova = Ova::new(self.arena_id, obj.generation, i as u32, 0);
                return Ok(ova);
            }
        }
        Err(ArenaError::OutOfSpace)
    }

    fn read(&self, ova: Ova) -> Result<&[u8], ArenaError> {
        let idx = ova.object() as usize;
        let obj = self.objects.get(idx).ok_or(ArenaError::InvalidOva)?;
        if !obj.alive {
            return Err(ArenaError::InvalidOva);
        }
        if obj.generation != ova.generation() {
            return Err(ArenaError::GenerationMismatch {
                expected: obj.generation,
                got: ova.generation(),
            });
        }
        let off = ova.offset() as usize;
        if off > obj.data.len() {
            return Err(ArenaError::InvalidOva);
        }
        Ok(&obj.data[off..])
    }

    fn write(&mut self, ova: Ova, data: &[u8]) -> Result<(), ArenaError> {
        let idx = ova.object() as usize;
        let obj = self.objects.get_mut(idx).ok_or(ArenaError::InvalidOva)?;
        if !obj.alive {
            return Err(ArenaError::InvalidOva);
        }
        if obj.generation != ova.generation() {
            return Err(ArenaError::GenerationMismatch {
                expected: obj.generation,
                got: ova.generation(),
            });
        }
        let off = ova.offset() as usize;
        if off + data.len() > obj.data.len() {
            return Err(ArenaError::InvalidOva);
        }
        obj.data[off..off + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn free(&mut self, ova: Ova) -> Result<(), ArenaError> {
        let idx = ova.object() as usize;
        let obj = self.objects.get_mut(idx).ok_or(ArenaError::InvalidOva)?;
        if !obj.alive {
            return Err(ArenaError::InvalidOva);
        }
        obj.alive = false;
        obj.data.clear();
        obj.generation = (obj.generation + 1) & MAX_GEN;
        Ok(())
    }

    fn clear(&mut self) {
        for obj in &mut self.objects {
            obj.alive = false;
            obj.data.clear();
            obj.generation = (obj.generation + 1) & MAX_GEN;
        }
    }
}

/// Result of a SYNC operation.
#[derive(Debug)]
pub struct SyncResult {
    pub old_active: ArenaId,
    pub new_active: ArenaId,
}

/// Triple-Arena: Nursery (immortal, 256 slots) + DynamicA/B (1024 slots each).
pub struct TripleArena {
    nursery: Arena,
    dynamic_a: Arena,
    dynamic_b: Arena,
    /// Which dynamic arena is currently active (DynamicA or DynamicB).
    active: ArenaId,
}

impl TripleArena {
    pub fn new() -> Self {
        Self {
            nursery: Arena::new(256, ArenaId::Nursery),
            dynamic_a: Arena::new(1024, ArenaId::DynamicA),
            dynamic_b: Arena::new(1024, ArenaId::DynamicB),
            active: ArenaId::DynamicA,
        }
    }

    pub fn active_arena(&self) -> ArenaId {
        self.active
    }

    fn staging_arena(&self) -> ArenaId {
        match self.active {
            ArenaId::DynamicA => ArenaId::DynamicB,
            _ => ArenaId::DynamicA,
        }
    }

    /// Allocate in the nursery (immortal objects).
    pub fn alloc_nursery(&mut self, size: usize) -> Result<Ova, ArenaError> {
        self.nursery.alloc(size)
    }

    /// Allocate in the staging (inactive) dynamic arena.
    pub fn alloc_staging(&mut self, size: usize) -> Result<Ova, ArenaError> {
        match self.staging_arena() {
            ArenaId::DynamicA => self.dynamic_a.alloc(size),
            ArenaId::DynamicB => self.dynamic_b.alloc(size),
            _ => Err(ArenaError::InvalidOva),
        }
    }

    /// Read from any arena, dispatched by OVA's arena bits.
    pub fn read(&self, ova: Ova) -> Result<&[u8], ArenaError> {
        match ova.arena() {
            ArenaId::Nursery => self.nursery.read(ova),
            ArenaId::DynamicA => self.dynamic_a.read(ova),
            ArenaId::DynamicB => self.dynamic_b.read(ova),
            ArenaId::Reserved => Err(ArenaError::InvalidOva),
        }
    }

    /// Write to any arena (nursery write after initial alloc succeeds for data fill).
    pub fn write(&mut self, ova: Ova, data: &[u8]) -> Result<(), ArenaError> {
        match ova.arena() {
            ArenaId::Nursery => self.nursery.write(ova, data),
            ArenaId::DynamicA => self.dynamic_a.write(ova, data),
            ArenaId::DynamicB => self.dynamic_b.write(ova, data),
            ArenaId::Reserved => Err(ArenaError::InvalidOva),
        }
    }

    /// Free an object in a dynamic arena.
    pub fn free(&mut self, ova: Ova) -> Result<(), ArenaError> {
        match ova.arena() {
            ArenaId::Nursery => Err(ArenaError::NurseryWriteViolation),
            ArenaId::DynamicA => self.dynamic_a.free(ova),
            ArenaId::DynamicB => self.dynamic_b.free(ova),
            ArenaId::Reserved => Err(ArenaError::InvalidOva),
        }
    }

    /// Atomic SYNC: swap active/staging, clear old staging.
    pub fn sync(&mut self) -> SyncResult {
        let old_active = self.active;
        let new_active = self.staging_arena();

        // Clear the old active (it becomes the new staging)
        match old_active {
            ArenaId::DynamicA => self.dynamic_a.clear(),
            ArenaId::DynamicB => self.dynamic_b.clear(),
            _ => {}
        }

        self.active = new_active;

        SyncResult {
            old_active,
            new_active,
        }
    }
}

impl Default for TripleArena {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for TripleArena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TripleArena(active={:?})", self.active)
    }
}
