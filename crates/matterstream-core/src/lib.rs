//! MatterStream Core — UI Instruction Set Architecture
//!
//! Core types, executor, builder, and parser for the MatterStream ISA.
//! Treats UI as a stream of immutable instructions (Ops) executed against
//! a 4-tier, register-mapped memory space (Matter).


pub mod ast_hook;
pub mod ast_tsx;
pub mod builder;
pub mod ops;
pub mod parser;
pub mod registers;
pub mod state_stack;
pub mod stream;
pub mod tier0;
pub mod tier1;
pub mod tier2;
pub mod tier3;

pub use builder::StreamBuilder;
pub use ops::{CompiledOps, Draw, Op, OpsHeader, Primitive, RsiPointer};
pub use parser::Parser;
pub use registers::RegisterFile;
pub use state_stack::{ProjStack, StateStack};
pub use stream::MatterStream;
pub use tier0::GlobalUniforms;
pub use tier1::{IntBank, Mat4Bank, ScalarBank, Vec3Bank, Vec4Bank};
pub use tier2::ZeroPage;
pub use tier3::{ResourceHandle, ResourceTable};
pub use ast_hook::{MtsmObject, MtsmVariant, AnyMtsmBinding, AnyMtsmHook, MtsmBinding, MtsmHook, MtsmActionGetter, MtsmActionSetter, MtsmSlotGetter, MtsmSlotSetter, MtsmPrimitive, TsShaderTransmissible, MtsmTsxFunctionalComponent, TsxElementContext, MtsmExecFunctionalComponent};
pub use ast_tsx::{TsxElement, TsxAttributes, TsxFragment, TsxKind};
