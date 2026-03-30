//! TKV arena operation constants and types.
//!
//! Defines the sub-operation enum and format constants for TKV arena objects.
//! The handler implementation lives in matterstream-vm-asm.

/// UserCall action_op for TKV arena operations.
pub const TKV_ACTION_OP: u64 = 0x30;

/// TKV arena sub-operations (passed as `data` to UserCall).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum TkvOp {
    /// Clone template from tkv_static_templates → dynamic arena.
    /// Stack: [oid_u128] → [ova]
    Clone       = 0x00,
    /// Allocate empty TKV in dynamic arena.
    /// Stack: [] → [ova]
    New         = 0x01,
    /// Write value at ordinal (type from existing entry).
    /// Stack: [ova, ordinal, value] → [ova]
    Set         = 0x02,
    /// Read value at ordinal.
    /// Stack: [ova, ordinal] → [value]
    Get         = 0x03,
    /// Find entry by TkvKey path (binary search on sort_key).
    /// Stack: [ova, tkv_key_u32] → [ordinal | u32::MAX]
    FindKey     = 0x04,
    /// Find entry by string key name (linear scan on key_str_idx).
    /// Stack: [ova, str_id] → [ordinal | u32::MAX]
    FindStrKey  = 0x05,
    /// Insert new entry by TkvKey path. O(n) re-sort.
    /// Stack: [ova, tkv_key_u32, key_name_disc, key_name_idx, value] → [ova]
    AddKey      = 0x06,
    /// Insert new entry by string key name. Assigns next available TkvKey. O(n).
    /// Stack: [ova, parent_key_u32, str_id, value_type, value] → [ova]
    AddStrKey   = 0x07,
    /// Copy from dynamic arena to nursery (seal as immutable).
    /// Stack: [ova] → [nursery_ova]
    Seal        = 0x08,
    /// Push entry count.
    /// Stack: [ova] → [count]
    Count       = 0x09,
}

impl TkvOp {
    pub fn from_u64(v: u64) -> Option<Self> {
        match v {
            0x00 => Some(Self::Clone),
            0x01 => Some(Self::New),
            0x02 => Some(Self::Set),
            0x03 => Some(Self::Get),
            0x04 => Some(Self::FindKey),
            0x05 => Some(Self::FindStrKey),
            0x06 => Some(Self::AddKey),
            0x07 => Some(Self::AddStrKey),
            0x08 => Some(Self::Seal),
            0x09 => Some(Self::Count),
            _ => None,
        }
    }
}

/// Header size in bytes (u32 entry count).
pub const TKV_HEADER_SIZE: usize = 4;
/// Fixed entry size in bytes.
pub const TKV_ENTRY_SIZE: usize = 16;
/// Default slot size for new TKV objects (64 entries max).
pub const TKV_SLOT_SIZE: usize = TKV_HEADER_SIZE + 64 * TKV_ENTRY_SIZE;
