//! OidIndex — zero-copy binary search over sorted `.osym` entries.
//!
//! `.osym` binary format:
//! - Header: `[count: u32][reserved: 4 bytes]` = 8 bytes
//! - Entries sorted by OID, 48 bytes each:
//!   `[oid_hi: u64][oid_lo: u64][kind: u8][pad: 7][val_hi: u64][val_lo: u64][val_idx: u64]`
//!
//! Entry `i` is at offset `8 + (i * 48)`.

use crate::fqa::Fqa;
use crate::oid::{ImportKind, Oid, SecurityMode};
use std::fmt;

const HEADER_SIZE: usize = 8;
const ENTRY_SIZE: usize = 48;

/// Error type for OID index operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OidIndexError {
    TruncatedData,
    InvalidEntryCount,
    NotSorted,
    InvalidImportKind(u8),
}

impl fmt::Display for OidIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OidIndexError::TruncatedData => write!(f, "truncated .osym data"),
            OidIndexError::InvalidEntryCount => write!(f, "invalid .osym entry count"),
            OidIndexError::NotSorted => write!(f, ".osym entries not sorted by OID"),
            OidIndexError::InvalidImportKind(k) => write!(f, "invalid import kind: {:#04x}", k),
        }
    }
}

/// A resolved OID entry from the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidEntry {
    pub oid: Oid,
    pub kind: ImportKind,
    pub val_hi: u64,
    pub val_lo: u64,
    pub val_idx: u64,
}

impl OidEntry {
    /// Get the FQA value (val_hi + val_lo as u128).
    pub fn fqa(&self) -> Fqa {
        Fqa::new(((self.val_hi as u128) << 64) | self.val_lo as u128)
    }

    /// Get the dispatch ID (for NativeHook).
    pub fn dispatch_id(&self) -> u32 {
        self.val_idx as u32
    }
}

/// Zero-copy wrapper over raw `.osym` bytes. Provides binary search and
/// prefix range scan directly on the archive member data.
pub struct OidIndex<'a> {
    data: &'a [u8],
    count: u32,
}

impl<'a> OidIndex<'a> {
    /// Wrap raw `.osym` bytes. Validates header and data length.
    pub fn from_bytes(data: &'a [u8]) -> Result<Self, OidIndexError> {
        if data.len() < HEADER_SIZE {
            return Err(OidIndexError::TruncatedData);
        }
        let count = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let expected = HEADER_SIZE + (count as usize) * ENTRY_SIZE;
        if data.len() < expected {
            return Err(OidIndexError::TruncatedData);
        }
        Ok(Self { data, count })
    }

    /// Number of entries in the index.
    pub fn len(&self) -> u32 {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Read the entry at position `i`.
    fn entry_at(&self, i: u32) -> OidEntry {
        let off = HEADER_SIZE + (i as usize) * ENTRY_SIZE;
        let d = &self.data[off..off + ENTRY_SIZE];
        let oid_hi = u64::from_le_bytes(d[0..8].try_into().unwrap());
        let oid_lo = u64::from_le_bytes(d[8..16].try_into().unwrap());
        let kind_byte = d[16];
        let val_hi = u64::from_le_bytes(d[24..32].try_into().unwrap());
        let val_lo = u64::from_le_bytes(d[32..40].try_into().unwrap());
        let val_idx = u64::from_le_bytes(d[40..48].try_into().unwrap());
        OidEntry {
            oid: Oid::new(oid_hi, oid_lo),
            kind: ImportKind::from_u8(kind_byte).unwrap_or(ImportKind::Symbol),
            val_hi,
            val_lo,
            val_idx,
        }
    }

    /// Read just the OID key at position `i` (for binary search without full parse).
    fn oid_at(&self, i: u32) -> u128 {
        let off = HEADER_SIZE + (i as usize) * ENTRY_SIZE;
        let d = &self.data[off..];
        let hi = u64::from_le_bytes(d[0..8].try_into().unwrap());
        let lo = u64::from_le_bytes(d[8..16].try_into().unwrap());
        ((hi as u128) << 64) | lo as u128
    }

    /// Binary search for an exact OID match.
    pub fn lookup(&self, oid: Oid) -> Option<OidEntry> {
        let target = oid.to_u128();
        let mut lo = 0u32;
        let mut hi = self.count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let mid_val = self.oid_at(mid);
            if mid_val == target {
                return Some(self.entry_at(mid));
            } else if mid_val < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        None
    }

    /// Find all entries whose OID matches `prefix` in the first `depth` segments.
    /// Returns entries as a contiguous range (relies on sorted order).
    pub fn prefix_range(&self, prefix: Oid, depth: u8) -> Vec<OidEntry> {
        if self.count == 0 || depth == 0 {
            return Vec::new();
        }
        // Find first entry >= prefix
        let start = self.lower_bound(prefix);
        let mut results = Vec::new();
        for i in start..self.count {
            let entry = self.entry_at(i);
            if prefix.is_prefix_of(&entry.oid, depth) {
                results.push(entry);
            } else {
                break; // sorted, so no more matches
            }
        }
        results
    }

    /// Lower bound: first index where oid_at(i) >= target.
    fn lower_bound(&self, target: Oid) -> u32 {
        let target_val = target.to_u128();
        let mut lo = 0u32;
        let mut hi = self.count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.oid_at(mid) < target_val {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// Validate that entries are sorted by OID (ascending). Called by archive validation.
    pub fn validate_sorted(&self) -> Result<(), OidIndexError> {
        for i in 1..self.count {
            if self.oid_at(i) <= self.oid_at(i - 1) {
                return Err(OidIndexError::NotSorted);
            }
        }
        Ok(())
    }

    /// Security mode for an OID — pure bit comparison, no index lookup needed.
    pub fn security_mode(oid: Oid) -> SecurityMode {
        oid.security_mode()
    }
}

// ── Builder: write sorted `.osym` bytes ─────────────────────────────────

/// Builder for creating `.osym` binary data.
pub struct OidIndexBuilder {
    entries: Vec<(Oid, ImportKind, u64, u64, u64)>,
}

impl OidIndexBuilder {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    /// Add an entry. Entries are sorted on `build()`.
    pub fn add(&mut self, oid: Oid, kind: ImportKind, val_hi: u64, val_lo: u64, val_idx: u64) {
        self.entries.push((oid, kind, val_hi, val_lo, val_idx));
    }

    /// Add an FQA-valued entry (Component, Symbol, Hook, Concept).
    pub fn add_fqa(&mut self, oid: Oid, kind: ImportKind, fqa: Fqa) {
        let hi = (fqa.value() >> 64) as u64;
        let lo = fqa.value() as u64;
        self.add(oid, kind, hi, lo, 0);
    }

    /// Add a NativeHook entry.
    pub fn add_native_hook(&mut self, oid: Oid, dispatch_id: u32) {
        self.add(oid, ImportKind::NativeHook, 0, 0, dispatch_id as u64);
    }

    /// Build the sorted `.osym` binary. Returns the raw bytes.
    pub fn build(&mut self) -> Vec<u8> {
        // Sort by OID (u128 ordering)
        self.entries.sort_by_key(|(oid, _, _, _, _)| oid.to_u128());

        let count = self.entries.len() as u32;
        let mut buf = Vec::with_capacity(HEADER_SIZE + self.entries.len() * ENTRY_SIZE);

        // Header
        buf.extend_from_slice(&count.to_le_bytes());
        buf.extend_from_slice(&[0u8; 4]); // reserved

        // Entries
        for (oid, kind, val_hi, val_lo, val_idx) in &self.entries {
            buf.extend_from_slice(&oid.hi.to_le_bytes());
            buf.extend_from_slice(&oid.lo.to_le_bytes());
            buf.push(*kind as u8);
            buf.extend_from_slice(&[0u8; 7]); // pad
            buf.extend_from_slice(&val_hi.to_le_bytes());
            buf.extend_from_slice(&val_lo.to_le_bytes());
            buf.extend_from_slice(&val_idx.to_le_bytes());
        }

        buf
    }
}

impl Default for OidIndexBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_index() -> Vec<u8> {
        let mut builder = OidIndexBuilder::new();
        // Add entries out of order — builder sorts them
        builder.add_fqa(
            Oid::from_segments(&[1, 1, 1, 1, 2]),
            ImportKind::Component,
            Fqa::new(0x1000),
        );
        builder.add_fqa(
            Oid::from_segments(&[1, 1, 1, 1, 1]),
            ImportKind::Symbol,
            Fqa::new(0x2000),
        );
        builder.add_fqa(
            Oid::from_segments(&[1, 1, 1, 1, 3]),
            ImportKind::Concept,
            Fqa::new(0x3000),
        );
        builder.add_native_hook(
            Oid::from_segments(&[1, 1, 1, 3, 1]),
            42,
        );
        builder.build()
    }

    #[test]
    fn builder_roundtrip() {
        let data = make_test_index();
        let idx = OidIndex::from_bytes(&data).unwrap();
        assert_eq!(idx.len(), 4);
        idx.validate_sorted().unwrap();
    }

    #[test]
    fn lookup_found() {
        let data = make_test_index();
        let idx = OidIndex::from_bytes(&data).unwrap();

        let entry = idx.lookup(Oid::from_segments(&[1, 1, 1, 1, 2])).unwrap();
        assert_eq!(entry.kind, ImportKind::Component);
        assert_eq!(entry.fqa().value(), 0x1000);
    }

    #[test]
    fn lookup_not_found() {
        let data = make_test_index();
        let idx = OidIndex::from_bytes(&data).unwrap();

        assert!(idx.lookup(Oid::from_segments(&[1, 1, 1, 1, 0])).is_none());
        assert!(idx.lookup(Oid::from_segments(&[2, 2, 2])).is_none());
    }

    #[test]
    fn lookup_native_hook() {
        let data = make_test_index();
        let idx = OidIndex::from_bytes(&data).unwrap();

        let entry = idx.lookup(Oid::from_segments(&[1, 1, 1, 3, 1])).unwrap();
        assert_eq!(entry.kind, ImportKind::NativeHook);
        assert_eq!(entry.dispatch_id(), 42);
    }

    #[test]
    fn prefix_range_query() {
        let data = make_test_index();
        let idx = OidIndex::from_bytes(&data).unwrap();

        // All entries under 1.1.1.1 (depth=4)
        let results = idx.prefix_range(Oid::PKG_ROOT_CHT_PUBLIC, 4);
        assert_eq!(results.len(), 3); // 1.1.1.1.1, 1.1.1.1.2, 1.1.1.1.3

        // All entries under 1.1.1.3 (depth=4) — system
        let results = idx.prefix_range(Oid::PKG_ROOT_CHT_SYSTEM, 4);
        assert_eq!(results.len(), 1); // 1.1.1.3.1
    }

    #[test]
    fn empty_index() {
        let mut builder = OidIndexBuilder::new();
        let data = builder.build();
        let idx = OidIndex::from_bytes(&data).unwrap();
        assert!(idx.is_empty());
        assert!(idx.lookup(Oid::ROOT).is_none());
        assert!(idx.prefix_range(Oid::ROOT, 1).is_empty());
    }

    #[test]
    fn single_entry() {
        let mut builder = OidIndexBuilder::new();
        builder.add_fqa(Oid::ROOT, ImportKind::Symbol, Fqa::new(99));
        let data = builder.build();
        let idx = OidIndex::from_bytes(&data).unwrap();
        assert_eq!(idx.len(), 1);
        let entry = idx.lookup(Oid::ROOT).unwrap();
        assert_eq!(entry.fqa().value(), 99);
    }

    #[test]
    fn truncated_data_errors() {
        assert!(OidIndex::from_bytes(&[]).is_err());
        assert!(OidIndex::from_bytes(&[1, 0, 0, 0, 0, 0, 0, 0]).is_err()); // count=1 but no entry data
    }

    #[test]
    fn unsorted_detection() {
        // Manually build unsorted data
        let mut buf = Vec::new();
        buf.extend_from_slice(&2u32.to_le_bytes()); // count=2
        buf.extend_from_slice(&[0u8; 4]); // reserved

        // Entry 0: OID = 1.1.2 (larger)
        let oid1 = Oid::from_segments(&[1, 1, 2]);
        buf.extend_from_slice(&oid1.hi.to_le_bytes());
        buf.extend_from_slice(&oid1.lo.to_le_bytes());
        buf.push(ImportKind::Symbol as u8);
        buf.extend_from_slice(&[0u8; 7]);
        buf.extend_from_slice(&[0u8; 24]); // val

        // Entry 1: OID = 1.1.1 (smaller — wrong order)
        let oid2 = Oid::from_segments(&[1, 1, 1]);
        buf.extend_from_slice(&oid2.hi.to_le_bytes());
        buf.extend_from_slice(&oid2.lo.to_le_bytes());
        buf.push(ImportKind::Symbol as u8);
        buf.extend_from_slice(&[0u8; 7]);
        buf.extend_from_slice(&[0u8; 24]); // val

        let idx = OidIndex::from_bytes(&buf).unwrap();
        assert_eq!(idx.validate_sorted(), Err(OidIndexError::NotSorted));
    }

    #[test]
    fn fqa_value_roundtrip() {
        let mut builder = OidIndexBuilder::new();
        let big_fqa = Fqa::new(0xDEAD_BEEF_CAFE_BABE_1234_5678_9ABC_DEF0);
        builder.add_fqa(Oid::ROOT, ImportKind::Component, big_fqa);
        let data = builder.build();
        let idx = OidIndex::from_bytes(&data).unwrap();
        let entry = idx.lookup(Oid::ROOT).unwrap();
        assert_eq!(entry.fqa().value(), big_fqa.value());
    }
}
