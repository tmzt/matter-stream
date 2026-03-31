//! # matterstream-ext-mtd1
//!
//! The `mtd1` (MatterStream Document v1) rendering engine.
//!
//! Compiles TSX/JSX declarative layouts into a high-density, 32-bit GPU-ready
//! Instruction Set Architecture (ISA), bypassing traditional DOM and HTML
//! layout engines entirely.
//!
//! ## Architecture
//!
//! - **`mtd1_format`** — Binary format: FourCC header, Style Bank, 32-bit ISA
//! - **`pretext_rs`** — Zero-allocation text layout engine (O(N) complexity)
//! - **`tsx_to_mtd1`** — TSX AST → mtd1 bytecode compiler bridge

pub use matterstream_mtd1_format as mtd1_format;
pub use pretext_rs;
pub mod tsx_to_mtd1;
pub mod mtd1_to_sdf;

pub use mtd1_format::{BankedStyle, Command32, Mtd1Document, Mtd1Header, MTD1_MAGIC};
pub use pretext_rs::{FontMetrics, LayoutConfig};
pub use tsx_to_mtd1::{TsxNode, compile_tsx};
pub use mtd1_to_sdf::{mtd1_to_sdf, SdfFrame, FONT_INDEX_PICTOGRAPHIC};
