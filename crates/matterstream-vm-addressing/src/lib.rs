//! FQA/OVA/OID addressing, ASLR token tables, and address resolution.

pub mod fqa;
pub mod ova;
pub mod aslr;
pub mod addressing;
pub mod oid;
pub mod oid_index;

pub use fqa::{Fqa, Ordinal, FourCC};
pub use ova::{Ova, ArenaId};
pub use oid::{Oid, SecurityMode, ImportKind, InstanceRef};
pub use oid_index::{OidIndex, OidIndexBuilder, OidEntry, OidIndexError};
