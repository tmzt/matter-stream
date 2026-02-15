//! MatterStream — UI Instruction Set Architecture
//!
//! Treats UI as a stream of immutable instructions (Ops) executed against
//! a 4-tier, register-mapped memory space (Matter).

pub mod builder;
pub mod compiler;
pub mod parser;
pub mod tier0;
pub mod tier1;
pub mod tier2;
pub mod tier3;
pub mod ops;
pub mod registers;
pub mod state_stack;
pub mod stream;

pub use builder::StreamBuilder;
pub use compiler::Compiler;
pub use parser::Parser;
pub use ops::{Op, OpsHeader, RsiPointer, Primitive};
pub use registers::RegisterFile;
pub use state_stack::{StateStack, ProjStack};
pub use stream::MatterStream;
pub use tier0::GlobalUniforms;
pub use tier1::{Mat4Bank, Vec4Bank, Vec3Bank, ScalarBank, IntBank};
pub use tier2::ZeroPage;
pub use tier3::{ResourceHandle, ResourceTable};
