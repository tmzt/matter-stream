//! UserCallHandler — trait for externally-registered UserCall action handlers.
//!
//! Allows host code to register handlers for custom UserCall action_op values.
//! The handler receives the data parameter (sub-op) and a VmHandle for stack/string access.

use crate::rpn::RpnError;
use crate::vm_handle::VmHandle;
use matterstream_vm_arena::TripleArena;
use std::any::Any;

/// Handler for a registered UserCall action.
///
/// Receives the `data` parameter from the UserCall instruction (used as sub-opcode)
/// and a `VmHandle` for stack, string table, and control register access.
pub trait UserCallHandler: Send {
    /// Dispatch a sub-operation.
    fn dispatch(
        &mut self,
        sub_op: u64,
        vm: &mut VmHandle,
        arenas: &mut TripleArena,
    ) -> Result<(), RpnError>;

    /// Gas cost for a sub-operation.
    fn gas_cost(&self, sub_op: u64) -> u64 { let _ = sub_op; 100 }

    /// Downcast support for extracting handler state after execution.
    fn as_any(self: Box<Self>) -> Box<dyn Any>;
}
