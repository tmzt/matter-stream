//! MatterStream — UI Instruction Set Architecture
//!
//! Treats UI as a stream of immutable instructions (Ops) executed against
//! a 4-tier, register-mapped memory space (Matter).
//!
//! VM_SPEC v0.1.0 subsystems: FQA/OVA addressing, triple-arena memory,
//! TKV metadata, AR archives, SCL entropy guard, RPN bytecode execution,
//! and Keyless invariant enforcement.

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

// VM_SPEC v0.1.0 modules
pub mod fqa;
pub mod ova;
pub mod aslr;
pub mod addressing;
pub mod arena;
pub mod dmove;
pub mod tkv;
pub mod archive;
pub mod scl;
pub mod keyless;
pub mod rpn;
pub mod ui_vm;

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

// VM_SPEC v0.1.0 re-exports
pub use fqa::{Fqa, Ordinal, FourCC};
pub use ova::{Ova, ArenaId};
pub use arena::TripleArena;
pub use scl::Scl;
pub use rpn::RpnVm;
pub use ui_vm::{UiDrawCmd, UiDrawState, render_ui_draws, rgba};
