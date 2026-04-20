//! FQA/OVA/OID addressing, ASLR token tables, and address resolution.

pub mod fqa;
pub mod ova;
pub mod aslr;
pub mod addressing;
pub mod oid;
pub mod oid_index;
pub mod tkv_key;
pub mod tkv_ops;

pub use fqa::{Fqa, Ordinal, FourCC};
pub use ova::{Ova, ArenaId};
pub use oid::{Oid, SecurityMode, ImportKind, InstanceRef};
pub use oid_index::{OidIndex, OidIndexBuilder, OidEntry, OidIndexError};
pub use tkv_key::{TkvKey, TkvType, TkvFixedEntry, StrRefDisc};

#[cfg(feature = "json")]
pub mod json_to_tkv;
#[cfg(feature = "json")]
pub use json_to_tkv::json_to_tkv_entries;
