<<<<<<< HEAD
//! MatterStream — UI Instruction Set Architecture
//!
//! Facade crate re-exporting all subsystem crates.

pub use matterstream_core::*;

// Root-level re-exports matching the old monolith API
pub use matterstream_vm_addressing::fqa::{Fqa, Ordinal, FourCC};
pub use matterstream_vm_addressing::ova::{Ova, ArenaId};
pub use matterstream_vm_arena::TripleArena;
pub use matterstream_vm_scl::Scl;

// Re-export subsystem crates as modules
pub mod fqa {
    pub use matterstream_vm_addressing::fqa::*;
}
pub mod ova {
    pub use matterstream_vm_addressing::ova::*;
}
pub mod aslr {
    pub use matterstream_vm_addressing::aslr::*;
}
pub mod addressing {
    pub use matterstream_vm_addressing::addressing::*;
}

pub mod arena {
    pub use matterstream_vm_arena::arena::*;
}
pub mod dmove {
    pub use matterstream_vm_arena::dmove::*;
}

pub mod tkv {
    pub use matterstream_packaging::tkv::*;
}
pub mod archive {
    pub use matterstream_packaging::archive::*;
}

pub mod scl {
    pub use matterstream_vm_scl::scl::*;
}
pub mod keyless {
    pub use matterstream_vm_scl::keyless::*;
}
=======
pub use matterstream_compiler::{Compiler, CompilerError};
pub use matterstream_core::{CompiledOps, Op, OpsHeader, Primitive};
pub use matterstream_loader::{Loader, LoaderError};
pub use matterstream_renderer::{Renderer, RendererError};

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn it_works() {
//         let result = add(2, 2);
//         assert_eq!(result, 4);
//     }
// }
>>>>>>> 3b9a15a (Commit current work)
