//! MatterStream TSX to msm1 bytecode compiler.

pub mod asm_compiler;

pub use asm_compiler::{compile_to_asm, compile_to_asm_with_imports, ImportMap};
