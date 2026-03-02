//! MatterStream Core — UI Instruction Set Architecture
//!
//! Ops, registers, tiers, stream executor, RPN VM, and UI draw commands.
//! VM_SPEC subsystems (addressing, arena, packaging, SCL) live in their own crates.

pub mod ast_hook;
pub mod ast_tsx;
pub mod builder;
#[cfg(feature = "compiler")]
pub mod compiler;
pub mod ops;
pub mod parser;
pub mod registers;
pub mod rpn;
pub mod state_stack;
pub mod stream;
pub mod tier0;
pub mod tier1;
pub mod tier2;
pub mod tier3;
pub mod ui_vm;

pub use builder::StreamBuilder;
#[cfg(feature = "compiler")]
pub use compiler::Compiler;
pub use ops::{CompiledOps, Draw, Op, OpsHeader, Primitive, RsiPointer};
pub use parser::Parser;
pub use registers::RegisterFile;
pub use rpn::RpnVm;
pub use state_stack::{ProjStack, StateStack};
pub use stream::MatterStream;
pub use tier0::GlobalUniforms;
pub use tier1::{IntBank, Mat4Bank, ScalarBank, Vec3Bank, Vec4Bank};
pub use tier2::ZeroPage;
pub use tier3::{ResourceHandle, ResourceTable};
pub use ast_hook::{MtsmObject, MtsmVariant, AnyMtsmBinding, AnyMtsmHook, MtsmBinding, MtsmHook, MtsmActionGetter, MtsmActionSetter, MtsmSlotGetter, MtsmSlotSetter, MtsmPrimitive, TsShaderTransmissible, MtsmTsxFunctionalComponent, TsxElementContext, MtsmExecFunctionalComponent, Binder, BinderEntry, MtsmSecureSourceSymbol, MtsmBindHandle, MtsmPackageHandle, MtsmPackageRegistry};
pub use ast_tsx::{TsxElement, TsxAttributes, TsxFragment, TsxKind, TsTypeValue, TsTypeDef, TypeValueBinder, SourceLoc};
pub use ui_vm::{render_ui_draws, rgba, UiDrawCmd, UiDrawState};
