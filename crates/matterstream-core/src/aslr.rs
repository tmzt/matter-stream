//! ASLR token table (.asym) -- randomized indirection from tokens to OVAs.

use crate::ova::{ArenaId, Ova};
use std::fmt;

/// Randomized indirection token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AslrToken(pub u32);

impl fmt::Display for AslrToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ASLR({:#010x})", self.0)
    }
}

/// Entry in the ASYM table: token -> OVA mapping.
#[derive(Debug, Clone, Copy)]
struct AsymEntry {
    token: AslrToken,
    ova: Ova,
}

/// ASLR symbol table -- maps tokens to OVAs, sorted for O(log n) lookup.
#[derive(Debug, Clone)]
pub struct AsymTable {
    entries: Vec<AsymEntry>,
    generation: u32,
}

impl AsymTable {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            generation: 0,
        }
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Insert a token -> OVA mapping. Maintains sorted order.
    pub fn insert(&mut self, token: AslrToken, ova: Ova) {
        let idx = self.entries.binary_search_by_key(&token.0, |e| e.token.0);
        match idx {
            Ok(i) => self.entries[i].ova = ova,
            Err(i) => self.entries.insert(i, AsymEntry { token, ova }),
        }
    }

    /// Resolve a token to an OVA via binary search.
    pub fn resolve(&self, token: AslrToken) -> Option<Ova> {
        self.entries
            .binary_search_by_key(&token.0, |e| e.token.0)
            .ok()
            .map(|i| self.entries[i].ova)
    }

    /// Bulk-rewrite all entries during SYNC: swap arena bits.
    pub fn swap_arena(&mut self, from: ArenaId, to: ArenaId) {
        for entry in &mut self.entries {
            if entry.ova.arena() == from {
                entry.ova = entry.ova.with_arena(to);
            }
        }
        self.generation += 1;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serialize to bytes for .asym file storage.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + self.entries.len() * 8);
        buf.extend_from_slice(&self.generation.to_le_bytes());
        buf.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for entry in &self.entries {
            buf.extend_from_slice(&entry.token.0.to_le_bytes());
            buf.extend_from_slice(&entry.ova.0.to_le_bytes());
        }
        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, AsymError> {
        if data.len() < 8 {
            return Err(AsymError::TruncatedData);
        }
        let generation = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let count = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        let expected_len = 8 + count * 8;
        if data.len() < expected_len {
            return Err(AsymError::TruncatedData);
        }
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let base = 8 + i * 8;
            let token = AslrToken(u32::from_le_bytes(data[base..base + 4].try_into().unwrap()));
            let ova = Ova(u32::from_le_bytes(data[base + 4..base + 8].try_into().unwrap()));
            entries.push(AsymEntry { token, ova });
        }
        Ok(Self { entries, generation })
    }
}

impl Default for AsymTable {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsymError {
    TruncatedData,
}

impl fmt::Display for AsymError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AsymError::TruncatedData => write!(f, "truncated .asym data"),
        }
    }
}
