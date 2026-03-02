//! FQA/OVA addressing, ASLR token tables, and address resolution.

pub mod fqa;
pub mod ova;
pub mod aslr;
pub mod addressing;

pub use fqa::{Fqa, Ordinal, FourCC};
pub use ova::{Ova, ArenaId};
