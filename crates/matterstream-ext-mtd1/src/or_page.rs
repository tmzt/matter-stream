//! MTD1 OR page handler — collects Command32 instructions during VM execution.

use std::any::Any;
use matterstream_vm::or_page::OrPageHandler;
use matterstream_vm::rpn::RpnError;
use matterstream_vm::vm_handle::VmHandle;
use matterstream_vm_arena::TripleArena;
use matterstream_mtd1_format::{Command32, BankedStyle};

/// OR page handler that collects Command32 instructions.
/// Registered at FOURCC_MTD1. After VM execution, read `instructions`
/// and pass to `mtd1_to_sdf()`.
pub struct Mtd1OrPageHandler {
    pub instructions: Vec<Command32>,
    pub styles: Vec<BankedStyle>,
}

impl Mtd1OrPageHandler {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            styles: Vec::new(),
        }
    }

    /// Add a default style.
    pub fn push_style(&mut self, style: BankedStyle) {
        self.styles.push(style);
    }

    /// Push a raw Command32.
    pub fn push(&mut self, cmd: Command32) {
        self.instructions.push(cmd);
    }
}

impl OrPageHandler for Mtd1OrPageHandler {
    fn dispatch(
        &mut self,
        sub_op: u8,
        vm: &mut VmHandle,
        _arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        match sub_op {
            // 0x00: draw_glyph — pop advance (u12), glyph_id (u16)
            0x00 => {
                let glyph_id = vm.pop_u32()? as u16;
                let advance = vm.pop_u32()? as u16;
                self.instructions.push(Command32::draw_glyph(advance.min(4095), glyph_id));
            }
            // 0x01: set_cursor — pop y (i14), x (i14)
            0x01 => {
                let x = vm.pop_u32()? as i16;
                let y = vm.pop_u32()? as i16;
                self.instructions.push(Command32::set_cursor(y, x));
            }
            // 0x02: set_style — pop bank index (u28)
            0x02 => {
                let idx = vm.pop_u32()?;
                self.instructions.push(Command32::set_style(idx));
            }
            // 0x03: draw_shape — pop height (u14), width (u14)
            0x03 => {
                let width = vm.pop_u32()? as u16;
                let height = vm.pop_u32()? as u16;
                self.instructions.push(Command32::draw_shape(height, width));
            }
            // 0x04: push raw Command32
            0x04 => {
                let raw = vm.pop_u32()?;
                self.instructions.push(Command32(raw));
            }
            _ => {}
        }
        Ok(())
    }

    fn gas_cost(&self, _sub_op: u8) -> u64 { 10 }

    fn as_any(self: Box<Self>) -> Box<dyn Any> { self }
    fn as_any_ref(&self) -> &dyn Any { self }
}
