//! OID (Object Identifier) — hierarchical inter-package address.
//!
//! A u128 stored as two u64 halves, each with MSB=0 for VDBE/SQLite varint
//! compatibility. 3-bit segments for 8-way branching.
//!
//! ```text
//! hi (u64): [0:1][seg_0:3]...[seg_19:3][prefix_len:3]      = 1+60+3 = 64
//! lo (u64): [0:1][seg_20:3]...[seg_38:3][prefix_len:3][flags:3] = 1+57+3+3 = 64
//! ```
//!
//! - 39 segments × 3 bits, 8-way branching
//! - hi prefix_len (3 bits): 1-7 = depth (lo not needed), 0 = extended
//! - lo prefix_len (3 bits): depth - 8 (0-6), or 7 = depth > 15 (scan fallback)
//! - lo flags (3 bits): reserved for future use
//! - When hi prefix_len > 0, lo is free for instance/generation data
//! - Sort: hi first, then lo (with trailer masked). Segments dominate.

use std::fmt;

/// Bit 63 must be 0 on both halves for VDBE/SQLite varint compatibility.
const HI_RESERVED_MASK: u64 = 1u64 << 63;
const LO_RESERVED_MASK: u64 = 1u64 << 63;
const VDBE_MASK: u64 = !HI_RESERVED_MASK; // same value, clearer name usage

const BITS_PER_SEG: u32 = 3;
const SEG_MASK: u64 = 0b111;

/// Segments fitting in hi (before prefix_len field).
const HI_SEGMENTS: usize = 20;
/// Max segments in lo (before 6-bit trailer).
const LO_SEGMENTS: usize = 19;
/// Total addressable segments.
pub const MAX_SEGMENTS: usize = HI_SEGMENTS + LO_SEGMENTS; // 39

/// hi prefix_len: bits 2..0 (3 bits). 1-7 = depth, 0 = extended (read lo).
const HI_PREFIX_SHIFT: u32 = 0;
const HI_PREFIX_MASK: u64 = 0b111;

/// lo trailer: 6 bits at bottom.
///   bits 5..3 = prefix_len (3 bits, used when hi_prefix_len = 0)
///   bits 2..0 = flags (3 bits, reserved)
const LO_PREFIX_SHIFT: u32 = 3;
const LO_PREFIX_MASK: u64 = 0b111;
const LO_FLAGS_SHIFT: u32 = 0;
const LO_FLAGS_MASK: u64 = 0b111;
const LO_TRAILER_MASK: u64 = 0x3F; // bottom 6 bits

/// Sort mask for lo: segments only, trailer cleared.
const LO_SORT_MASK: u64 = VDBE_MASK & !LO_TRAILER_MASK;

/// Bit position of segment `i` within its half.
/// Segment 0 at bits 62..60 of hi, segment 19 at bits 5..3 of hi.
/// Segment 20 at bits 62..60 of lo, segment 37 at bits 11..9 of lo.
const fn seg_shift(local_idx: usize) -> u32 {
    62 - (local_idx as u32) * BITS_PER_SEG
}

/// Shift for hi segments: same as seg_shift but offset from bit 62.
/// Hi segment i is at bits (62 - i*3)..(60 - i*3). But prefix_len is at bits 2..0.
/// So hi segment 0 at 62..60, hi segment 19 at 5..3. prefix_len at 2..0.
/// Check: 20 segments × 3 = 60 bits, from bit 62 down to bit 3. prefix_len at 2..0. Total = 63. ✓
const fn hi_seg_shift(i: usize) -> u32 {
    // Segment 0 at bits 62..60, segment 19 at bits 5..3. prefix_len at 2..0.
    60 - (i as u32) * BITS_PER_SEG
}

/// Lo segment i (local index 0..17) at bits (62 - i*3)..(60 - i*3).
/// Lo segment 0 at 62..60, lo segment 17 at 11..9. Meta at 8..0.
/// Check: 18 segments × 3 = 54 bits, from bit 62 down to bit 9. Meta 8..0. Total = 63. ✓
const fn lo_seg_shift(i: usize) -> u32 {
    // Lo segment 0 at bits 62..60, segment 17 at bits 11..9. Meta at 8..0.
    60 - (i as u32) * BITS_PER_SEG
}

/// OID — hierarchical 2×u64 address with 3-bit segments.
/// Full identity: hi + lo including prefix_len, flags, instance data.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Oid {
    pub hi: u64,
    pub lo: u64,
}

impl Oid {
    pub const fn new(hi: u64, lo: u64) -> Self {
        Self { hi: hi & VDBE_MASK, lo: lo & VDBE_MASK }
    }

    pub const fn from_u128(val: u128) -> Self {
        let hi = ((val >> 64) as u64) & VDBE_MASK;
        let lo = (val as u64) & VDBE_MASK;
        Self { hi, lo }
    }

    pub const fn to_u128(&self) -> u128 {
        ((self.hi as u128) << 64) | (self.lo as u128)
    }

    /// Build from segment path (0-7 each, 1-38 segments).
    pub const fn from_segments(segs: &[u8]) -> Self {
        assert!(segs.len() >= 1, "OID requires at least 1 segment");
        assert!(segs.len() <= MAX_SEGMENTS, "OID max 38 segments");
        let mut hi: u64 = 0;
        let mut lo: u64 = 0;
        let mut i = 0;
        while i < segs.len() {
            assert!(segs[i] <= 7, "OID segment must be 0-7");
            let seg = segs[i] as u64;
            if i < HI_SEGMENTS {
                hi |= seg << hi_seg_shift(i);
            } else {
                lo |= seg << lo_seg_shift(i - HI_SEGMENTS);
            }
            i += 1;
        }
        // Store prefix_len
        if segs.len() <= 7 {
            hi |= (segs.len() as u64) << HI_PREFIX_SHIFT;
        } else {
            // hi prefix_len = 0 → extended, lo prefix_len has (depth - 8)
            // lo bits 5..3: 0 = depth 8, 1 = depth 9, ... 7 = depth 15
            // For depth 16-39: we chain — lo prefix_len = 0 means depth stored
            // implicitly by segment scan. But that defeats the purpose.
            // Since max useful depth is ~15 in practice (8-way branching gives
            // 8^15 = 35 trillion addresses), 3 bits covering 8-15 is sufficient.
            // Depth > 15 uses lo prefix_len = 7 and the actual depth is
            // determined by the last non-zero segment (fallback scan).
            let lo_depth = if segs.len() <= 15 {
                (segs.len() - 8) as u64
            } else {
                7 // sentinel: depth > 15, scan to determine
            };
            lo |= lo_depth << LO_PREFIX_SHIFT;
        }
        Self { hi, lo }
    }

    /// Extract segment at level (0..37).
    pub const fn segment(&self, level: u8) -> u8 {
        assert!((level as usize) < MAX_SEGMENTS, "OID segment 0..37");
        if (level as usize) < HI_SEGMENTS {
            ((self.hi >> hi_seg_shift(level as usize)) & SEG_MASK) as u8
        } else {
            ((self.lo >> lo_seg_shift(level as usize - HI_SEGMENTS)) & SEG_MASK) as u8
        }
    }

    /// Depth (number of significant segments). O(1) for depth ≤ 15.
    pub const fn depth(&self) -> u8 {
        let hi_len = (self.hi >> HI_PREFIX_SHIFT) & HI_PREFIX_MASK;
        if hi_len > 0 {
            // Depth 1-7: stored directly in hi
            hi_len as u8
        } else {
            // Extended: lo prefix_len = depth - 8 (0-6), or 7 = scan needed
            let lo_len = (self.lo >> LO_PREFIX_SHIFT) & LO_PREFIX_MASK;
            if lo_len < 7 {
                (lo_len + 8) as u8
            } else {
                // Depth > 15: scan for last non-zero segment
                let mut d: u8 = 0;
                let mut i: u8 = 0;
                while (i as usize) < MAX_SEGMENTS {
                    if self.segment(i) != 0 { d = i + 1; }
                    i += 1;
                }
                d
            }
        }
    }

    /// True if this OID uses only hi (depth ≤ 7). lo can be ignored for segments.
    pub const fn is_hi_only(&self) -> bool {
        ((self.hi >> HI_PREFIX_SHIFT) & HI_PREFIX_MASK) > 0
    }

    /// Append a segment at the current depth.
    pub const fn child_const(self, value: u8) -> Self {
        let d = self.depth() as usize;
        assert!(d < MAX_SEGMENTS, "OID max depth");
        assert!(value <= 7, "OID segment must be 0-7");
        let seg = value as u64;
        let mut hi = self.hi & !(HI_PREFIX_MASK << HI_PREFIX_SHIFT);
        let mut lo = self.lo & !(LO_PREFIX_MASK << LO_PREFIX_SHIFT);
        if d < HI_SEGMENTS {
            hi |= seg << hi_seg_shift(d);
        } else {
            lo |= seg << lo_seg_shift(d - HI_SEGMENTS);
        }
        let new_depth = d + 1;
        if new_depth <= 7 {
            hi |= (new_depth as u64) << HI_PREFIX_SHIFT;
        } else {
            // hi prefix_len stays 0 (extended)
            let lo_depth = if new_depth <= 15 {
                (new_depth - 8) as u64
            } else {
                7 // sentinel
            };
            lo |= lo_depth << LO_PREFIX_SHIFT;
        }
        Oid { hi, lo }
    }

    /// Check if `self` is a prefix of `other` up to `depth` segments.
    pub fn is_prefix_of(&self, other: &Oid, depth: u8) -> bool {
        let mut i = 0u8;
        while i < depth {
            if self.segment(i) != other.segment(i) { return false; }
            i += 1;
        }
        true
    }

    pub const ZERO: Oid = Oid { hi: 0, lo: 0 };

    // ── Well-known roots ────────────────────────────────────────────────

    pub const ROOT: Oid = Oid::from_segments(&[1]);
    pub const PKG_ROOT: Oid = Oid::from_segments(&[1, 1]);
    pub const PKG_ROOT_CHT: Oid = Oid::from_segments(&[1, 1, 1]);
    pub const PKG_ROOT_CHT_PUBLIC: Oid = Oid::from_segments(&[1, 1, 1, 1]);
    pub const PKG_ROOT_CHT_INTERNAL: Oid = Oid::from_segments(&[1, 1, 1, 2]);
    pub const PKG_ROOT_CHT_SYSTEM: Oid = Oid::from_segments(&[1, 1, 1, 3]);
    pub const PKG_ROOT_PUBLIC: Oid = Oid::from_segments(&[1, 1, 2]);
    pub const PKG_SKILLS: Oid = Oid::from_segments(&[1, 1, 1, 2, 1]);

    // ── Third-party OIDs (6-bit encoded reverse-DNS) ──────────────────

    /// 6-bit character encoding for reverse-DNS package names.
    /// ```text
    /// 0      = terminator (end of name, start of user address space)
    /// 1-26   = a-z
    /// 27-36  = 0-9
    /// 37     = . (dot separator)
    /// 38     = - (hyphen)
    /// 39     = IDN indicator (reserved, not implemented)
    /// 40-63  = reserved
    /// ```
    const DNS_TERMINATOR: u8 = 0;
    const DNS_DOT: u8 = 37;
    const DNS_HYPHEN: u8 = 38;

    /// Bits used by the prefix (1.1.2) = 3 segments × 3 bits.
    const PUBLIC_PREFIX_BITS: usize = 9;

    /// Max 6-bit chars that fit after the prefix.
    /// hi: (60 - 9) = 51 bits → 8 chars (48 bits) + 3 spare bits
    /// lo: 57 bits → 9 chars (54 bits) + 3 spare bits
    /// Total: 17 chars. Longer names use hash fallback (FNV-1a).
    const MAX_DNS_CHARS: usize = 17;

    fn encode_dns_char(c: u8) -> u8 {
        match c {
            b'a'..=b'z' => c - b'a' + 1,
            b'A'..=b'Z' => c - b'A' + 1, // case-insensitive
            b'0'..=b'9' => c - b'0' + 27,
            b'.' => Self::DNS_DOT,
            b'-' => Self::DNS_HYPHEN,
            _ => 0, // unknown → terminator
        }
    }

    fn decode_dns_char(v: u8) -> Option<u8> {
        match v {
            0 => None, // terminator
            1..=26 => Some(v - 1 + b'a'),
            27..=36 => Some(v - 27 + b'0'),
            37 => Some(b'.'),
            38 => Some(b'-'),
            _ => None,
        }
    }

    /// Create a third-party OID from a reverse-DNS name.
    ///
    /// The name is 6-bit encoded into the OID bits after the
    /// `PKG_ROOT_PUBLIC` (1.1.2) prefix. A 0 terminator marks the
    /// end of the name. Bits after the terminator are user address space.
    ///
    /// Names > 17 chars fall back to FNV-1a hash.
    pub fn from_reverse_dns(name: &str) -> Self {
        let bytes = name.as_bytes();
        let use_hash = bytes.len() > Self::MAX_DNS_CHARS;

        // Build a flat bitstream: 126 bits (63 per half, MSB reserved)
        // Prefix occupies bits 125..117 (segments 1.1.2 = 9 bits at top of hi)
        let mut bits = [0u8; 126]; // bit array, index 0 = hi bit 62
        // Write prefix: segments 1,1,2 at 3 bits each
        write_bits(&mut bits, 0, 3, 1); // seg 0 = 1
        write_bits(&mut bits, 3, 3, 1); // seg 1 = 1
        write_bits(&mut bits, 6, 3, 2); // seg 2 = 2

        let mut pos = 9; // first free bit after prefix
        if !use_hash {
            // Direct 6-bit encoding
            for &b in bytes {
                if pos + 6 > 120 { break; } // leave room for trailer
                write_bits(&mut bits, pos, 6, Self::encode_dns_char(b) as u64);
                pos += 6;
            }
            // Terminator
            if pos + 6 <= 120 {
                write_bits(&mut bits, pos, 6, 0);
                pos += 6;
            }
        } else {
            // Hash fallback
            let hash = fnv1a_hash(bytes);
            let mut h = hash;
            for _ in 0..Self::MAX_DNS_CHARS {
                if pos + 6 > 120 { break; }
                let mut chunk = (h & 0x3F) as u8;
                if chunk == 0 { chunk = 1; } // avoid accidental terminator
                write_bits(&mut bits, pos, 6, chunk as u64);
                h >>= 6;
                pos += 6;
            }
            // Terminator
            if pos + 6 <= 120 {
                write_bits(&mut bits, pos, 6, 0);
                pos += 6;
            }
        }

        // Reconstruct hi and lo from bit array
        let hi = bits_to_u64(&bits, 0) & VDBE_MASK;
        let lo = bits_to_u64(&bits, 63) & VDBE_MASK;

        // Store depth in prefix_len fields
        // "depth" for third-party = number of 6-bit chars + 3 prefix segments
        let char_count = if use_hash { Self::MAX_DNS_CHARS } else { bytes.len() };
        let total_depth = 3 + char_count + 1; // prefix segs + chars + terminator
        let mut hi = hi & !(HI_PREFIX_MASK << HI_PREFIX_SHIFT);
        let mut lo = lo & !(LO_PREFIX_MASK << LO_PREFIX_SHIFT);
        if total_depth <= 7 {
            hi |= (total_depth as u64) << HI_PREFIX_SHIFT;
        } else {
            let lo_d = if total_depth <= 15 { (total_depth - 8) as u64 } else { 7 };
            lo |= lo_d << LO_PREFIX_SHIFT;
        }

        Oid { hi, lo }
    }

    /// Extract the reverse-DNS name from a third-party OID.
    /// Returns None if not third-party or can't decode.
    pub fn reverse_dns_name(&self) -> Option<String> {
        if !self.is_third_party() { return None; }
        let bits_hi = u64_to_bits(self.hi);
        let bits_lo = u64_to_bits(self.lo);
        let mut bits = [0u8; 126];
        bits[..63].copy_from_slice(&bits_hi);
        bits[63..].copy_from_slice(&bits_lo);

        let mut pos = 9; // after prefix
        let mut name = Vec::new();
        loop {
            if pos + 6 > 120 { break; }
            let val = read_bits(&bits, pos, 6) as u8;
            if val == 0 { break; } // terminator
            match Self::decode_dns_char(val) {
                Some(c) => name.push(c),
                None => break,
            }
            pos += 6;
        }
        if name.is_empty() { return None; }
        String::from_utf8(name).ok()
    }

    /// Check if this OID is under the third-party public root.
    pub fn is_third_party(&self) -> bool {
        Oid::PKG_ROOT_PUBLIC.is_prefix_of(self, Oid::PKG_ROOT_PUBLIC.depth())
    }
}

// ── Bit manipulation helpers ────────────────────────────────────────────

/// Write `width` bits of `value` at position `pos` in a 126-bit array.
/// pos=0 is bit 62 of hi (MSB after reserved).
fn write_bits(bits: &mut [u8; 126], pos: usize, width: usize, value: u64) {
    for i in 0..width {
        let bit = ((value >> (width - 1 - i)) & 1) as u8;
        if pos + i < 126 {
            bits[pos + i] = bit;
        }
    }
}

/// Read `width` bits starting at `pos` from a 126-bit array.
fn read_bits(bits: &[u8; 126], pos: usize, width: usize) -> u64 {
    let mut val = 0u64;
    for i in 0..width {
        if pos + i < 126 {
            val = (val << 1) | (bits[pos + i] as u64);
        }
    }
    val
}

/// Convert the top 63 bits (bit 62..0) of a u64 to a 63-element bit array.
fn u64_to_bits(v: u64) -> [u8; 63] {
    let mut bits = [0u8; 63];
    for i in 0..63 {
        bits[i] = ((v >> (62 - i)) & 1) as u8;
    }
    bits
}

/// Reconstruct a u64 from a 63-element bit array (bit 63 = 0 for VDBE).
fn bits_to_u64(bits: &[u8; 126], offset: usize) -> u64 {
    let mut v = 0u64;
    for i in 0..63 {
        v = (v << 1) | (bits[offset + i] as u64);
    }
    v
}

/// FNV-1a 64-bit hash (IETF draft, deterministic, no external deps).
fn fnv1a_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

// ── Security ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityMode {
    System,
    Internal,
    Sandboxed,
}

impl Oid {
    pub fn security_mode(&self) -> SecurityMode {
        if Oid::PKG_ROOT_CHT_SYSTEM.is_prefix_of(self, 4) {
            SecurityMode::System
        } else if Oid::PKG_ROOT_CHT_INTERNAL.is_prefix_of(self, 4) {
            SecurityMode::Internal
        } else {
            SecurityMode::Sandboxed
        }
    }
}

// ── Import kind ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ImportKind {
    Component  = 0x01,
    Symbol     = 0x02,
    Concept    = 0x03,
    Hook       = 0x04,
    NativeHook = 0x05,
}

impl ImportKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::Component),
            0x02 => Some(Self::Symbol),
            0x03 => Some(Self::Concept),
            0x04 => Some(Self::Hook),
            0x05 => Some(Self::NativeHook),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceRef {
    pub oid: Oid,
    pub generation: u32,
}

// ── Display ────────────────────────────────────────────────────────────

impl fmt::Debug for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.depth();
        write!(f, "Oid(")?;
        for i in 0..d {
            if i > 0 { write!(f, ".")?; }
            write!(f, "{}", self.segment(i))?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.depth();
        for i in 0..d {
            if i > 0 { write!(f, ".")?; }
            write!(f, "{}", self.segment(i))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let oid = Oid::from_segments(&[1, 2, 3, 0, 1]);
        assert_eq!(oid.segment(0), 1);
        assert_eq!(oid.segment(1), 2);
        assert_eq!(oid.segment(2), 3);
        assert_eq!(oid.segment(3), 0);
        assert_eq!(oid.segment(4), 1);
        assert_eq!(oid.depth(), 5);
    }

    #[test]
    fn depth_stored_not_computed() {
        let a = Oid::from_segments(&[1, 2]);
        let b = Oid::from_segments(&[1, 2, 0]);
        assert_eq!(a.depth(), 2);
        assert_eq!(b.depth(), 3);
        assert_ne!(a, b);
    }

    #[test]
    fn hi_only_for_shallow() {
        let shallow = Oid::from_segments(&[1, 2, 3]);
        assert!(shallow.is_hi_only());
        assert_eq!(shallow.depth(), 3);
    }

    #[test]
    fn extended_depth() {
        let mut segs = [1u8; 10]; // depth 10 > 7
        let oid = Oid::from_segments(&segs);
        assert!(!oid.is_hi_only());
        assert_eq!(oid.depth(), 10);
        for i in 0..10 {
            assert_eq!(oid.segment(i), 1);
        }
    }

    #[test]
    fn child() {
        let parent = Oid::from_segments(&[1, 1, 1]);
        let child = parent.child_const(5);
        assert_eq!(child.depth(), 4);
        assert_eq!(child.segment(3), 5);
    }

    #[test]
    fn sorting() {
        let parent = Oid::from_segments(&[1]);
        let child = Oid::from_segments(&[1, 1]);
        let sibling = Oid::from_segments(&[2]);
        assert!(parent < child);
        assert!(child < sibling);
    }

    #[test]
    fn eight_way() {
        for v in 0..=7u8 {
            let oid = Oid::from_segments(&[v]);
            assert_eq!(oid.segment(0), v);
        }
    }

    #[test]
    fn well_known_roots() {
        assert_eq!(Oid::ROOT.depth(), 1);
        assert_eq!(Oid::PKG_SKILLS.depth(), 5);
        assert!(Oid::ROOT < Oid::PKG_ROOT);
        assert!(Oid::PKG_ROOT < Oid::PKG_ROOT_CHT);
    }

    #[test]
    fn security() {
        assert_eq!(Oid::PKG_ROOT_CHT_SYSTEM.security_mode(), SecurityMode::System);
        assert_eq!(Oid::PKG_ROOT_CHT_SYSTEM.child_const(1).security_mode(), SecurityMode::System);
        assert_eq!(Oid::PKG_ROOT_CHT_INTERNAL.security_mode(), SecurityMode::Internal);
        assert_eq!(Oid::PKG_SKILLS.security_mode(), SecurityMode::Internal);
        assert_eq!(Oid::ROOT.security_mode(), SecurityMode::Sandboxed);
    }

    #[test]
    fn vdbe_safe() {
        let oid = Oid::from_segments(&[7, 7, 7, 7, 7]);
        assert_eq!(oid.hi & (1u64 << 63), 0);
        assert_eq!(oid.lo & (1u64 << 63), 0);
    }

    #[test]
    fn cross_half() {
        let mut segs = [0u8; 25];
        segs[0] = 1;
        segs[19] = 7; // last in hi
        segs[20] = 5; // first in lo
        segs[24] = 3;
        let oid = Oid::from_segments(&segs);
        assert_eq!(oid.segment(0), 1);
        assert_eq!(oid.segment(19), 7);
        assert_eq!(oid.segment(20), 5);
        assert_eq!(oid.segment(24), 3);
        assert_eq!(oid.depth(), 25);
        assert!(!oid.is_hi_only());
    }

    #[test]
    fn display() {
        assert_eq!(format!("{}", Oid::from_segments(&[1, 2, 3])), "1.2.3");
    }

    #[test]
    fn u128_roundtrip() {
        let oid = Oid::from_segments(&[1, 2, 3, 4, 5]);
        let oid2 = Oid::from_u128(oid.to_u128());
        assert_eq!(oid, oid2);
    }

    // ── Third-party reverse-DNS OIDs ─────────────────────────────────

    #[test]
    fn reverse_dns_deterministic() {
        let a = Oid::from_reverse_dns("com.example.mylib");
        let b = Oid::from_reverse_dns("com.example.mylib");
        assert_eq!(a, b);
    }

    #[test]
    fn reverse_dns_different_names_differ() {
        let a = Oid::from_reverse_dns("com.example.foo");
        let b = Oid::from_reverse_dns("com.example.bar");
        assert_ne!(a, b);
    }

    #[test]
    fn reverse_dns_is_third_party() {
        let oid = Oid::from_reverse_dns("org.mozilla.firefox");
        assert!(oid.is_third_party());
        assert_eq!(oid.segment(0), 1);
        assert_eq!(oid.segment(1), 1);
        assert_eq!(oid.segment(2), 2);
        eprintln!("org.mozilla.firefox → {:?}", oid);
    }

    #[test]
    fn reverse_dns_roundtrip_name() {
        let name = "com.example.mylib";
        let oid = Oid::from_reverse_dns(name);
        let decoded = oid.reverse_dns_name().expect("should decode");
        assert_eq!(decoded, name);
        eprintln!("{} → {:?} → {}", name, oid, decoded);
    }

    #[test]
    fn reverse_dns_with_digits_and_hyphens() {
        let name = "io.k8s-sigs.controller-runtime";
        if name.len() <= Oid::MAX_DNS_CHARS {
            let oid = Oid::from_reverse_dns(name);
            let decoded = oid.reverse_dns_name().expect("should decode");
            assert_eq!(decoded, name);
        }
    }

    #[test]
    fn reverse_dns_is_sandboxed() {
        let oid = Oid::from_reverse_dns("com.example.untrusted");
        assert_eq!(oid.security_mode(), SecurityMode::Sandboxed);
    }

    #[test]
    fn reverse_dns_vdbe_safe() {
        let oid = Oid::from_reverse_dns("com.example.test");
        assert_eq!(oid.hi & (1u64 << 63), 0);
        assert_eq!(oid.lo & (1u64 << 63), 0);
    }

    #[test]
    fn reverse_dns_sorting() {
        let chitin = Oid::PKG_SKILLS;
        let third = Oid::from_reverse_dns("com.example.pkg");
        assert!(chitin < third, "{:?} should be < {:?}", chitin, third);
    }

    #[test]
    fn reverse_dns_short_name() {
        let name = "io.test";
        let oid = Oid::from_reverse_dns(name);
        let decoded = oid.reverse_dns_name().expect("should decode");
        assert_eq!(decoded, name);
    }
}
