//! TkvKey — 3-bit segment packed key with type tag and array support.
//!
//! 8 segment slots of 3 bits each in the high 24 bits, with metadata in the low byte.
//! Sorting uses only the segment bits (31..8) so metadata doesn't affect order.
//!
//! ```text
//!   [31..8]  8 segment slots * 3 bits (segment 0 = bits 31..29, most significant)
//!   [7]      is_array flag — if set, slots 6+7 merge into 6-bit array index (0-63)
//!   [6..4]   prefix_len (3 bits, 1-7)
//!   [3..1]   type tag (3 bits)
//!   [0]      pad
//! ```
//!
//! Normal key: 1-7 segments of path, type-tagged value at that position.
//! Array key: 1-6 segments of path + 6-bit array index at the leaf (max 64 elements).
//! Every key has at least one segment (prefix_len >= 1).

use std::fmt;

/// Maximum path segments for a normal key.
pub const MAX_SEGMENTS: usize = 7;
/// Maximum path segments for an array key (slots 6+7 become array index).
pub const MAX_ARRAY_PATH: usize = 6;
/// Maximum array index (6 bits).
pub const MAX_ARRAY_INDEX: usize = 63;

const BITS_PER_SEGMENT: u32 = 3;
const SEGMENT_MASK: u32 = 0b111;

// Segment slot bit positions (slot 0 = most significant).
const fn slot_shift(i: usize) -> u32 {
    29 - (i as u32) * BITS_PER_SEGMENT
}

// Low-byte field positions.
const IS_ARRAY_BIT: u32 = 7;
const PREFIX_LEN_SHIFT: u32 = 4;
const PREFIX_LEN_MASK: u32 = 0b111;
const TYPE_TAG_SHIFT: u32 = 1;
const TYPE_TAG_MASK: u32 = 0b111;

/// Mask for the segment bits only (used for sorting).
const SORT_MASK: u32 = 0xFFFF_FF00;

/// Type tags for TKV values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TkvType {
    String  = 0,
    Integer = 1,
    Boolean = 2,
    Fqa     = 3,
    Table   = 4, // nested TKV (OVA reference)
    Null    = 7,
}

impl TkvType {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::String),
            1 => Some(Self::Integer),
            2 => Some(Self::Boolean),
            3 => Some(Self::Fqa),
            4 => Some(Self::Table),
            7 => Some(Self::Null),
            _ => None,
        }
    }
}

/// 3-bit segment packed key with metadata.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct TkvKey(pub u32);

impl TkvKey {
    /// Build a normal (non-array) key from segments with a type tag.
    pub const fn new(segs: &[u8], typ: TkvType) -> Self {
        assert!(segs.len() >= 1, "TkvKey requires at least 1 segment");
        assert!(segs.len() <= MAX_SEGMENTS, "TkvKey max 7 segments");
        let mut val: u32 = 0;
        let mut i = 0;
        while i < segs.len() {
            assert!(segs[i] <= 7, "segment must be 0-7");
            val |= (segs[i] as u32) << slot_shift(i);
            i += 1;
        }
        val |= (segs.len() as u32) << PREFIX_LEN_SHIFT;
        val |= (typ as u32) << TYPE_TAG_SHIFT;
        TkvKey(val)
    }

    /// Build a key from segments (type defaults to Null, for backward compat).
    pub const fn from_segments(segs: &[u8]) -> Self {
        Self::new(segs, TkvType::Null)
    }

    /// Build an array element key: path segments + 6-bit array index.
    pub const fn array(segs: &[u8], index: u8, typ: TkvType) -> Self {
        assert!(segs.len() >= 1, "array key requires at least 1 path segment");
        assert!(segs.len() <= MAX_ARRAY_PATH, "array key max 6 path segments");
        assert!(index <= MAX_ARRAY_INDEX as u8, "array index max 63");
        let mut val: u32 = 0;
        let mut i = 0;
        while i < segs.len() {
            assert!(segs[i] <= 7, "segment must be 0-7");
            val |= (segs[i] as u32) << slot_shift(i);
            i += 1;
        }
        // Merge index into slots 6+7 (bits 13..8)
        val |= ((index as u32) & 0x3F) << 8;
        val |= 1 << IS_ARRAY_BIT; // set array flag
        val |= (segs.len() as u32) << PREFIX_LEN_SHIFT; // path depth (not counting array idx)
        val |= (typ as u32) << TYPE_TAG_SHIFT;
        TkvKey(val)
    }

    /// Prefix length (number of path segments, not counting array index).
    pub const fn prefix_len(&self) -> usize {
        ((self.0 >> PREFIX_LEN_SHIFT) & PREFIX_LEN_MASK) as usize
    }

    /// Whether this is an array element key.
    pub const fn is_array(&self) -> bool {
        (self.0 >> IS_ARRAY_BIT) & 1 != 0
    }

    /// Type tag.
    pub const fn type_tag(&self) -> u8 {
        ((self.0 >> TYPE_TAG_SHIFT) & TYPE_TAG_MASK) as u8
    }

    /// Extract segment at level `i` (0 = root).
    pub const fn segment(&self, level: usize) -> u8 {
        assert!(level < 8, "TkvKey slot 0..7");
        ((self.0 >> slot_shift(level)) & SEGMENT_MASK) as u8
    }

    /// Array index (valid only when `is_array()` is true). 6 bits from slots 6+7.
    pub const fn array_index(&self) -> u8 {
        ((self.0 >> 8) & 0x3F) as u8
    }

    /// Create a child by appending a segment.
    pub const fn child(self, value: u8, typ: TkvType) -> Self {
        assert!(value <= 7, "segment must be 0-7");
        let depth = self.prefix_len();
        assert!(depth < MAX_SEGMENTS, "TkvKey max depth");
        let new_len = depth + 1;
        // Copy segment bits, clear metadata
        let seg_bits = self.0 & SORT_MASK;
        let mut val = seg_bits | ((value as u32) << slot_shift(depth));
        val |= (new_len as u32) << PREFIX_LEN_SHIFT;
        val |= (typ as u32) << TYPE_TAG_SHIFT;
        TkvKey(val)
    }

    /// Sorting key: only segment bits, metadata stripped.
    pub const fn sort_key(&self) -> u32 {
        self.0 & SORT_MASK
    }

    /// Raw u32 value.
    pub const fn raw(&self) -> u32 {
        self.0
    }

    /// Check if `self` is a descendant of (or equal path to) `ancestor`.
    pub const fn has_prefix(&self, ancestor: TkvKey) -> bool {
        let a_len = ancestor.prefix_len();
        if self.prefix_len() < a_len { return false; }
        let mut i = 0;
        while i < a_len {
            if self.segment(i) != ancestor.segment(i) { return false; }
            i += 1;
        }
        true
    }
}

impl Ord for TkvKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl PartialOrd for TkvKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Debug for TkvKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.prefix_len();
        if self.is_array() {
            write!(f, "TkvKey(")?;
            for i in 0..d {
                if i > 0 { write!(f, ".")?; }
                write!(f, "{}", self.segment(i))?;
            }
            write!(f, "[{}])", self.array_index())
        } else {
            write!(f, "TkvKey(")?;
            for i in 0..d {
                if i > 0 { write!(f, ".")?; }
                write!(f, "{}", self.segment(i))?;
            }
            write!(f, ")")
        }
    }
}

impl fmt::Display for TkvKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.prefix_len();
        for i in 0..d {
            if i > 0 { write!(f, ".")?; }
            write!(f, "{}", self.segment(i))?;
        }
        if self.is_array() {
            write!(f, "[{}]", self.array_index())?;
        }
        Ok(())
    }
}

// ── TkvFixedEntry — 16-byte arena entry ────────────────────────────────

/// String reference discriminant for TKV values and key names.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum StrRefDisc {
    /// Index into compile-time string_table (immutable).
    StringTable = 0x00,
    /// Index into runtime string_values table (mutable, arena-allocated).
    StringValues = 0x01,
}

/// 16-byte fixed-size TKV arena entry.
///
/// ```text
/// [0..3]   key_path      TkvKey (u32 LE)
/// [4]      value_type    TkvType tag
/// [5..12]  value         8 bytes payload (format per type)
/// [13]     key_str_disc  StrRefDisc for key name
/// [14..15] key_str_idx   u16 LE index for key name
/// ```
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(C, packed)]
pub struct TkvFixedEntry {
    pub key_path: u32,
    pub value_type: u8,
    pub value: [u8; 8],
    pub key_str_disc: u8,
    pub key_str_idx: u16,
}

const _ASSERT_SIZE: () = assert!(std::mem::size_of::<TkvFixedEntry>() == 16);

impl TkvFixedEntry {
    /// Create an entry with a string value (string table ref).
    pub const fn string(key: TkvKey, str_idx: u32, key_name_disc: u8, key_name_idx: u16) -> Self {
        let mut value = [0u8; 8];
        value[0] = StrRefDisc::StringTable as u8;
        let bytes = str_idx.to_le_bytes();
        value[1] = bytes[0];
        value[2] = bytes[1];
        value[3] = bytes[2];
        value[4] = bytes[3];
        Self {
            key_path: key.raw(),
            value_type: TkvType::String as u8,
            value,
            key_str_disc: key_name_disc,
            key_str_idx: key_name_idx,
        }
    }

    /// Create an entry with an integer value.
    pub const fn integer(key: TkvKey, val: u64, key_name_disc: u8, key_name_idx: u16) -> Self {
        Self {
            key_path: key.raw(),
            value_type: TkvType::Integer as u8,
            value: val.to_le_bytes(),
            key_str_disc: key_name_disc,
            key_str_idx: key_name_idx,
        }
    }

    /// Create an entry with a boolean value.
    pub const fn boolean(key: TkvKey, val: bool, key_name_disc: u8, key_name_idx: u16) -> Self {
        let mut value = [0u8; 8];
        value[0] = if val { 1 } else { 0 };
        Self {
            key_path: key.raw(),
            value_type: TkvType::Boolean as u8,
            value,
            key_str_disc: key_name_disc,
            key_str_idx: key_name_idx,
        }
    }

    /// Create a null entry (placeholder).
    pub const fn null(key: TkvKey, key_name_disc: u8, key_name_idx: u16) -> Self {
        Self {
            key_path: key.raw(),
            value_type: TkvType::Null as u8,
            value: [0u8; 8],
            key_str_disc: key_name_disc,
            key_str_idx: key_name_idx,
        }
    }

    /// The TkvKey for this entry.
    pub const fn key(&self) -> TkvKey {
        TkvKey(self.key_path)
    }

    /// Sort key (segment bits only, for ordering).
    pub const fn sort_key(&self) -> u32 {
        self.key_path & SORT_MASK
    }

    /// Encode to 16 bytes (little-endian).
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0..4].copy_from_slice(&self.key_path.to_le_bytes());
        buf[4] = self.value_type;
        buf[5..13].copy_from_slice(&self.value);
        buf[13] = self.key_str_disc;
        buf[14..16].copy_from_slice(&self.key_str_idx.to_le_bytes());
        buf
    }

    /// Decode from 16 bytes (little-endian).
    pub fn from_bytes(b: &[u8; 16]) -> Self {
        Self {
            key_path: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            value_type: b[4],
            value: [b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12]],
            key_str_disc: b[13],
            key_str_idx: u16::from_le_bytes([b[14], b[15]]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_segments_roundtrip() {
        let k = TkvKey::from_segments(&[1, 2, 3]);
        assert_eq!(k.prefix_len(), 3);
        assert_eq!(k.segment(0), 1);
        assert_eq!(k.segment(1), 2);
        assert_eq!(k.segment(2), 3);
        assert!(!k.is_array());
    }

    #[test]
    fn typed_key() {
        let k = TkvKey::new(&[1, 2], TkvType::String);
        assert_eq!(k.prefix_len(), 2);
        assert_eq!(k.type_tag(), TkvType::String as u8);
        assert_eq!(k.segment(0), 1);
        assert_eq!(k.segment(1), 2);
    }

    #[test]
    fn depth_disambiguates() {
        let a = TkvKey::from_segments(&[1, 2]);
        let b = TkvKey::from_segments(&[1, 2, 0]);
        // Same segment bits for slots 0-1, but slot 2 differs (0 vs has value)
        // Actually [1,2,0] has segment 2 = 0, same bits as [1,2] which also has 0 in slot 2.
        // Disambiguation comes from sort_key being same → they compare equal in sort.
        // But prefix_len differs, so they're != as values.
        assert_ne!(a.raw(), b.raw()); // different due to prefix_len
    }

    #[test]
    fn child() {
        let parent = TkvKey::new(&[1], TkvType::Table);
        let ch = parent.child(3, TkvType::String);
        assert_eq!(ch.prefix_len(), 2);
        assert_eq!(ch.segment(0), 1);
        assert_eq!(ch.segment(1), 3);
        assert_eq!(ch.type_tag(), TkvType::String as u8);
    }

    #[test]
    fn sorting_depth_first() {
        let parent = TkvKey::from_segments(&[1]);
        let child_a = TkvKey::from_segments(&[1, 1]);
        let child_b = TkvKey::from_segments(&[1, 2]);
        let sibling = TkvKey::from_segments(&[2]);

        // Parent before children (segment 1 = 0 in parent vs nonzero in children)
        assert!(parent < child_a);
        assert!(parent < child_b);
        // Children of 1 before sibling 2 (segment 0: 1 < 2)
        assert!(child_a < sibling);
        assert!(child_b < sibling);
        // Children ordered by segment
        assert!(child_a < child_b);
    }

    #[test]
    fn array_key() {
        let k = TkvKey::array(&[1, 2], 42, TkvType::Integer);
        assert!(k.is_array());
        assert_eq!(k.prefix_len(), 2);
        assert_eq!(k.segment(0), 1);
        assert_eq!(k.segment(1), 2);
        assert_eq!(k.array_index(), 42);
        assert_eq!(k.type_tag(), TkvType::Integer as u8);
    }

    #[test]
    fn array_sorting() {
        let elem_0 = TkvKey::array(&[1], 0, TkvType::String);
        let elem_1 = TkvKey::array(&[1], 1, TkvType::String);
        let elem_63 = TkvKey::array(&[1], 63, TkvType::String);
        let sibling = TkvKey::from_segments(&[2]);

        // Array elements of [1] sort before sibling [2]
        assert!(elem_0 < sibling);
        assert!(elem_63 < sibling);
        // Array elements sort by index
        assert!(elem_0 < elem_1);
        assert!(elem_1 < elem_63);
    }

    #[test]
    fn prefix_check() {
        let parent = TkvKey::from_segments(&[1, 2]);
        let child = TkvKey::from_segments(&[1, 2, 5]);
        let other = TkvKey::from_segments(&[1, 3, 5]);

        assert!(child.has_prefix(parent));
        assert!(!other.has_prefix(parent));
        assert!(parent.has_prefix(parent));
    }

    #[test]
    fn max_segments() {
        let k = TkvKey::from_segments(&[7, 7, 7, 7, 7, 7, 7]);
        assert_eq!(k.prefix_len(), 7);
        for i in 0..7 {
            assert_eq!(k.segment(i), 7);
        }
    }

    #[test]
    fn display() {
        let k = TkvKey::from_segments(&[1, 2, 3]);
        assert_eq!(format!("{}", k), "1.2.3");
        let a = TkvKey::array(&[1, 2], 5, TkvType::String);
        assert_eq!(format!("{}", a), "1.2[5]");
    }

    #[test]
    fn type_tag_doesnt_affect_sort() {
        let a = TkvKey::new(&[1, 2], TkvType::String);
        let b = TkvKey::new(&[1, 2], TkvType::Integer);
        // Different type tags but same sort order
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
        // But not equal as values
        assert_ne!(a.raw(), b.raw());
    }
}
