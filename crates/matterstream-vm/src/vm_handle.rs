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
        self.vm.push(val)
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

    /// Pop a u64 from the VM stack.
    pub fn pop_u64(&mut self) -> Result<u64, RpnError> {
        let v = self.pop()?;
        match v {
            RpnValue::U64(x) => Ok(x),
            RpnValue::U32(x) => Ok(x as u64),
            _ => Err(RpnError::TypeMismatch),
        }
    }

    /// Pop an OVA from the VM stack.
    pub fn pop_ova(&mut self) -> Result<matterstream_vm_addressing::ova::Ova, RpnError> {
        let v = self.pop()?;
        match v {
            RpnValue::Ova(o) => Ok(o),
            RpnValue::U32(x) => Ok(matterstream_vm_addressing::ova::Ova(x)),
            _ => Err(RpnError::TypeMismatch),
        }
    }

    /// Access the TKV static templates — index → nursery OVA.
    pub fn tkv_static_templates(&self) -> &[matterstream_vm_addressing::ova::Ova] {
        &self.vm.tkv_static_templates
    }

    /// Read-only access to an OR page handler by FourCC.
    pub fn or_page_handle<T: 'static>(&self, fourcc: u32) -> Option<&T> {
        self.vm.or_page_handle::<T>(fourcc)
    }

    /// Push an SDF draw command.
    pub fn push_sdf_draw(&mut self, cmd: matterstream_common::SdfDrawCmd) {
        if self.vm.sdf_draws.len() < matterstream_common::MAX_DRAW_CMDS {
            self.vm.sdf_draws.push(cmd);
        }
    }

    /// Extend SDF draws from a slice.
    pub fn extend_sdf_draws(&mut self, draws: &[matterstream_common::SdfDrawCmd]) {
        for d in draws {
            self.push_sdf_draw(*d);
        }
    }

    /// Add a string to the string table, returning its index.
    pub fn push_string(&mut self, s: String) -> u32 {
        let idx = self.vm.string_table.len() as u32;
        self.vm.string_table.push(s);
        idx
    }

    /// Execute bytecode against arenas.
    pub fn execute(
        &mut self,
        bytecode: &[u8],
        arenas: &mut matterstream_vm_arena::TripleArena,
    ) -> Result<(), RpnError> {
        self.vm.execute(bytecode, arenas)
    }
}
