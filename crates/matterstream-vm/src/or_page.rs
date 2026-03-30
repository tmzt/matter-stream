//! OrPageHandler — trait for extensible OR page dispatch.
//!
//! External consumers register handlers by FourCC via `RpnVm::register_or_page`.
//! The VM invokes handlers through a `VmHandle` that exposes only the operations
//! a handler legitimately needs (stack, strings, control registers).

use std::any::Any;
use crate::rpn::RpnError;
use crate::vm_handle::VmHandle;
use matterstream_vm_arena::TripleArena;

/// Trait for OR page handlers registered by FourCC.
///
/// Implementations own their page-specific state and receive a curated `VmHandle`
/// for interacting with the VM stack and string table.
pub trait OrPageHandler: Send {
    /// Dispatch a sub-opcode (0x00–0x7F) on this page.
    fn dispatch(
        &mut self,
        sub_op: u8,
        vm: &mut VmHandle,
        arenas: &mut TripleArena,
    ) -> Result<(), RpnError>;

    /// Gas cost for a given sub-op. Return 0 to use the VM's default OR page cost.
    fn gas_cost(&self, sub_op: u8) -> u64;

    /// Downcast support (consuming).
    fn as_any(self: Box<Self>) -> Box<dyn Any>;

    /// Downcast support (by reference).
    fn as_any_ref(&self) -> &dyn Any;
}
