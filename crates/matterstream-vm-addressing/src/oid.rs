//! OID (Object Identifier) — hierarchical inter-package address.
//!
//! A u128 stored as two u64 halves, each with MSB reserved (always 0) for
//! VDBE/SQLite varint compatibility. This gives 63 usable bits per half,
//! 126 total, packed as 63 two-bit segments for 4-way branching at each level.
//!
//! Binary search on sorted u128s — no trie needed.

use std::fmt;

/// OID — hierarchical 2×u64 address with VDBE-safe layout.
///
/// ```text
/// hi (u64): [0 reserved][seg_0:2][seg_1:2]...[seg_30:2][1 unused bit]
/// lo (u64): [0 reserved][seg_31:2][seg_32:2]...[seg_61:2][1 unused bit]
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Oid {
    pub hi: u64,
    pub lo: u64,
}

/// Mask to clear the MSB of a u64 (VDBE safety).
const VDBE_MASK: u64 = !(1u64 << 63);

impl Oid {
    /// Create an OID from two u64 halves, clearing MSBs for VDBE safety.
    pub const fn new(hi: u64, lo: u64) -> Self {
        Self {
            hi: hi & VDBE_MASK,
            lo: lo & VDBE_MASK,
        }
    }

    /// Create from a u128, splitting into halves with MSBs cleared.
    pub const fn from_u128(val: u128) -> Self {
        let hi = ((val >> 64) as u64) & VDBE_MASK;
        let lo = (val as u64) & VDBE_MASK;
        Self { hi, lo }
    }

    /// Reconstruct the u128 representation.
    pub const fn to_u128(&self) -> u128 {
        ((self.hi as u128) << 64) | (self.lo as u128)
    }

    /// Build an OID from a dot-separated segment path (e.g., `&[1, 1, 1, 3]`).
    /// Each segment must be 0–3 (2 bits). Max 63 segments.
    pub const fn from_segments(segs: &[u8]) -> Self {
        assert!(segs.len() <= 63, "OID max 63 segments");
        let mut hi: u64 = 0;
        let mut lo: u64 = 0;
        let mut i = 0;
        while i < segs.len() {
            let seg = (segs[i] & 0x03) as u64;
            if i < 31 {
                // hi: bits 62..1, segment i at bit position (62 - i*2)
                let shift = 62 - (i as u32) * 2;
                hi |= seg << shift;
            } else {
                // lo: bits 62..1, segment (i-31) at bit position (62 - (i-31)*2)
                let shift = 62 - ((i - 31) as u32) * 2;
                lo |= seg << shift;
            }
            i += 1;
        }
        Self { hi, lo }
    }

    /// Extract the 2-bit segment at the given level (0..62).
    pub const fn segment(&self, level: u8) -> u8 {
        assert!(level < 63, "OID segment level 0..62");
        if level < 31 {
            let shift = 62 - (level as u32) * 2;
            ((self.hi >> shift) & 0x03) as u8
        } else {
            let shift = 62 - ((level - 31) as u32) * 2;
            ((self.lo >> shift) & 0x03) as u8
        }
    }

    /// Effective depth: the last non-zero segment level + 1.
    pub fn depth(&self) -> u8 {
        for i in (0..63u8).rev() {
            if self.segment(i) != 0 {
                return i + 1;
            }
        }
        0
    }

    /// Check if `self` is a prefix of `other` at the given depth.
    pub fn is_prefix_of(&self, other: &Oid, depth: u8) -> bool {
        for i in 0..depth {
            if self.segment(i) != other.segment(i) {
                return false;
            }
        }
        true
    }

    /// The zero OID.
    pub const ZERO: Oid = Oid { hi: 0, lo: 0 };

    // ── Well-known roots ────────────────────────────────────────────────

    /// `1` — OID root
    pub const ROOT: Oid = Oid::from_segments(&[1]);
    /// `1.1` — Package root
    pub const PKG_ROOT: Oid = Oid::from_segments(&[1, 1]);
    /// `1.1.1` — Chitin ecosystem root (`@chitin/`)
    pub const PKG_ROOT_CHT: Oid = Oid::from_segments(&[1, 1, 1]);
    /// `1.1.1.1` — Chitin public packages (`@chitin/[pkgpath]`)
    pub const PKG_ROOT_CHT_PUBLIC: Oid = Oid::from_segments(&[1, 1, 1, 1]);
    /// `1.1.1.2` — Chitin internal (`@chitin/internal`)
    pub const PKG_ROOT_CHT_INTERNAL: Oid = Oid::from_segments(&[1, 1, 1, 2]);
    /// `1.1.1.3` — Chitin system (`@chitin/system`)
    pub const PKG_ROOT_CHT_SYSTEM: Oid = Oid::from_segments(&[1, 1, 1, 3]);
    /// `1.1.2` — Public package tree (third-party)
    pub const PKG_ROOT_PUBLIC: Oid = Oid::from_segments(&[1, 1, 2]);

    // ── @chitin/skills package OIDs (1.1.1.2.1-2) ─────────────────────
    /// `1.1.1.2.1` — `<Param>` element (skill parameter definition)
    pub const SKILLS_PARAM: Oid = Oid::from_segments(&[1, 1, 1, 2, 1]);
    /// `1.1.1.2.2` — `<Trigger>` element (heuristic routing pattern)
    pub const SKILLS_TRIGGER: Oid = Oid::from_segments(&[1, 1, 1, 2, 2]);

    /// Package name for `@chitin/skills` imports.
    pub const SKILLS_PACKAGE_NAME: &'static str = "@chitin/skills";
}

impl fmt::Debug for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Print as dot-separated segments up to depth
        let d = self.depth();
        if d == 0 {
            return write!(f, "Oid(0)");
        }
        write!(f, "Oid(")?;
        for i in 0..d {
            if i > 0 {
                write!(f, ".")?;
            }
            write!(f, "{}", self.segment(i))?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.depth();
        if d == 0 {
            return write!(f, "0");
        }
        for i in 0..d {
            if i > 0 {
                write!(f, ".")?;
            }
            write!(f, "{}", self.segment(i))?;
        }
        Ok(())
    }
}

// ── Security modes ──────────────────────────────────────────────────────

/// Security mode derived from OID prefix. Determines what the caller is
/// allowed to do when resolving this OID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityMode {
    /// `1.1.1.3` — VM-escape + full CR access.
    System,
    /// `1.1.1.2` — VM-escape only, no CR switching.
    Internal,
    /// Everything else — no VM-escape, no CR modification.
    Sandboxed,
}

impl Oid {
    /// Derive the security mode from this OID's position in the tree.
    /// Pure bit comparison against well-known prefixes — no index lookup needed.
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

// ── Import kind ─────────────────────────────────────────────────────────

/// What kind of import an OID entry represents. Carried alongside the OID
/// as a type tag, not encoded in the OID bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ImportKind {
    Component = 0x01,
    Symbol = 0x02,
    Concept = 0x03,
    Hook = 0x04,
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

// ── InstanceRef ─────────────────────────────────────────────────────────

/// 3×u64 address: Oid (2×u64) + ordinal index (1×u64).
/// Same VDBE rule: MSB=0 on each u64 component → 3×63 = 189 usable bits.
/// Used for addressing into object memory (array index, property ordinal, rowid).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceRef {
    pub oid: Oid,
    pub index: u64,
}

impl InstanceRef {
    pub const fn new(oid: Oid, index: u64) -> Self {
        Self {
            oid,
            index: index & VDBE_MASK,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oid_zero() {
        assert_eq!(Oid::ZERO.to_u128(), 0);
        assert_eq!(Oid::ZERO.depth(), 0);
    }

    #[test]
    fn oid_from_segments_roundtrip() {
        let oid = Oid::from_segments(&[1, 2, 3, 0, 1]);
        assert_eq!(oid.segment(0), 1);
        assert_eq!(oid.segment(1), 2);
        assert_eq!(oid.segment(2), 3);
        assert_eq!(oid.segment(3), 0);
        assert_eq!(oid.segment(4), 1);
        assert_eq!(oid.depth(), 5);
    }

    #[test]
    fn oid_u128_roundtrip() {
        let oid = Oid::from_segments(&[1, 1, 1, 3]);
        let val = oid.to_u128();
        let oid2 = Oid::from_u128(val);
        assert_eq!(oid, oid2);
    }

    #[test]
    fn oid_vdbe_invariant() {
        // MSB of each u64 must always be 0
        let oid = Oid::new(u64::MAX, u64::MAX);
        assert_eq!(oid.hi & (1u64 << 63), 0);
        assert_eq!(oid.lo & (1u64 << 63), 0);
    }

    #[test]
    fn oid_well_known_roots() {
        assert_eq!(Oid::ROOT.depth(), 1);
        assert_eq!(Oid::ROOT.segment(0), 1);

        assert_eq!(Oid::PKG_ROOT.depth(), 2);
        assert_eq!(Oid::PKG_ROOT.segment(0), 1);
        assert_eq!(Oid::PKG_ROOT.segment(1), 1);

        assert_eq!(Oid::PKG_ROOT_CHT_SYSTEM.depth(), 4);
        assert_eq!(Oid::PKG_ROOT_CHT_SYSTEM.segment(3), 3);

        assert_eq!(Oid::PKG_ROOT_CHT_INTERNAL.segment(3), 2);
        assert_eq!(Oid::PKG_ROOT_CHT_PUBLIC.segment(3), 1);

        assert_eq!(Oid::PKG_ROOT_PUBLIC.depth(), 3);
        assert_eq!(Oid::PKG_ROOT_PUBLIC.segment(2), 2);
    }

    #[test]
    fn oid_security_mode() {
        // System: 1.1.1.3.*
        assert_eq!(Oid::PKG_ROOT_CHT_SYSTEM.security_mode(), SecurityMode::System);
        let sys_child = Oid::from_segments(&[1, 1, 1, 3, 2, 1]);
        assert_eq!(sys_child.security_mode(), SecurityMode::System);

        // Internal: 1.1.1.2.*
        assert_eq!(Oid::PKG_ROOT_CHT_INTERNAL.security_mode(), SecurityMode::Internal);
        let int_child = Oid::from_segments(&[1, 1, 1, 2, 3]);
        assert_eq!(int_child.security_mode(), SecurityMode::Internal);

        // Sandboxed: everything else
        assert_eq!(Oid::PKG_ROOT_CHT_PUBLIC.security_mode(), SecurityMode::Sandboxed);
        assert_eq!(Oid::PKG_ROOT_PUBLIC.security_mode(), SecurityMode::Sandboxed);
        assert_eq!(Oid::ROOT.security_mode(), SecurityMode::Sandboxed);
        let third_party = Oid::from_segments(&[1, 1, 2, 1, 3, 2]);
        assert_eq!(third_party.security_mode(), SecurityMode::Sandboxed);
    }

    #[test]
    fn oid_prefix_check() {
        let parent = Oid::from_segments(&[1, 1, 1]);
        let child = Oid::from_segments(&[1, 1, 1, 3, 2]);
        let other = Oid::from_segments(&[1, 1, 2, 1]);

        assert!(parent.is_prefix_of(&child, 3));
        assert!(parent.is_prefix_of(&other, 2)); // first 2 match
        assert!(!parent.is_prefix_of(&other, 3)); // third differs
    }

    #[test]
    fn oid_display() {
        assert_eq!(format!("{}", Oid::ROOT), "1");
        assert_eq!(format!("{}", Oid::PKG_ROOT_CHT_SYSTEM), "1.1.1.3");
        assert_eq!(format!("{}", Oid::ZERO), "0");
    }

    #[test]
    fn oid_ordering() {
        // OIDs should sort by u128 value
        let a = Oid::from_segments(&[1, 1, 1]);
        let b = Oid::from_segments(&[1, 1, 2]);
        let c = Oid::from_segments(&[1, 2, 1]);
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn oid_segments_cross_halves() {
        // Segment 30 is the last in hi, 31 is first in lo
        let mut segs = [0u8; 63];
        segs[30] = 3;
        segs[31] = 2;
        segs[62] = 1;
        let oid = Oid::from_segments(&segs);
        assert_eq!(oid.segment(30), 3);
        assert_eq!(oid.segment(31), 2);
        assert_eq!(oid.segment(62), 1);
        assert_eq!(oid.depth(), 63);
    }

    #[test]
    fn instance_ref_vdbe_safety() {
        let iref = InstanceRef::new(Oid::ROOT, u64::MAX);
        assert_eq!(iref.index & (1u64 << 63), 0);
    }
}
