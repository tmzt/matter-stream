//! VmHandle — curated API surface for OR page handlers.
//!
//! Wraps `&mut RpnVm` and exposes only the operations that external handlers
//! legitimately need. Handlers cannot touch the PC, gas state, page registry,
//! or arenas through this handle.

use crate::rpn::{RpnVm, RpnValue, RpnError};

/// Curated API surface for OR page handlers.
pub struct VmHandle<'a> {
    pub(crate) vm: &'a mut RpnVm,
}

impl<'a> VmHandle<'a> {
    /// Push a value onto the VM stack.
    pub fn push(&mut self, val: RpnValue) -> Result<(), RpnError> {
        self.vm.push_value(val)
    }

    /// Pop a value from the VM stack.
    pub fn pop(&mut self) -> Result<RpnValue, RpnError> {
        self.vm.stack.pop().ok_or(RpnError::StackUnderflow)
    }

    /// Pop a u32 from the VM stack (coerces u64 → u32).
    pub fn pop_u32(&mut self) -> Result<u32, RpnError> {
        let v = self.pop()?;
        match v {
            RpnValue::U32(x) => Ok(x),
            RpnValue::U64(x) => Ok(x as u32),
            _ => Err(RpnError::TypeMismatch),
        }
    }

    /// Resolve a string table index to a string.
    pub fn resolve_str(&self, idx: u32) -> Result<String, RpnError> {
        let effective_idx = idx + self.vm.string_base_offset;
        self.vm
            .string_table
            .get(effective_idx as usize)
            .cloned()
            .ok_or(RpnError::InvalidStringIndex(effective_idx))
    }

    /// Read a control register.
    pub fn cr(&self, idx: usize) -> u32 {
        self.vm.cr_bank[idx]
    }

    /// Access the string table (read-only).
    pub fn string_table(&self) -> &[String] {
        &self.vm.string_table
    }

    /// Access the mutable string bank (read-only view).
    pub fn string_bank(&self) -> &[Option<String>] {
        &self.vm.string_bank
    }

    /// Write a value to the mutable string bank.
    pub fn set_string_bank(&mut self, idx: usize, val: Option<String>) {
        if idx < self.vm.string_bank.len() {
            self.vm.string_bank[idx] = val;
        }
    }
}
