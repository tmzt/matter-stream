//! RegisterFile — aggregates all Tier 1 banks with per-bank dirty tracking.

use crate::ops::RsiPointer;
use crate::tier1::{BankId, IntBank, Mat4Bank, ScalarBank, Vec3Bank, Vec4Bank};
use crate::tier2::ZeroPage;
use crate::tier3::ResourceTable;

/// Dirty flags for each bank — critical for Test C (PUSH_PROJ only dirties MAT4).
#[derive(Debug, Clone, Default)]
pub struct DirtyFlags {
    pub mat4: bool,
    pub vec4: bool,
    pub vec3: bool,
    pub scalar: bool,
    pub int: bool,
}

impl DirtyFlags {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear_all(&mut self) {
        *self = Self::default();
    }

    pub fn set(&mut self, bank: BankId) {
        match bank {
            BankId::Mat4 => self.mat4 = true,
            BankId::Vec4 => self.vec4 = true,
            BankId::Vec3 => self.vec3 = true,
            BankId::Scalar => self.scalar = true,
            BankId::Int => self.int = true,
        }
    }

    pub fn is_dirty(&self, bank: BankId) -> bool {
        match bank {
            BankId::Mat4 => self.mat4,
            BankId::Vec4 => self.vec4,
            BankId::Vec3 => self.vec3,
            BankId::Scalar => self.scalar,
            BankId::Int => self.int,
        }
    }
}

/// The full register file: all Tier 1 banks + dirty tracking.
#[derive(Debug, Clone)]
pub struct RegisterFile {
    pub mat4: Mat4Bank,
    pub vec4: Vec4Bank,
    pub vec3: Vec3Bank,
    pub scalar: ScalarBank,
    pub int: IntBank,
    pub dirty: DirtyFlags,
}

impl RegisterFile {
    pub fn new() -> Self {
        Self {
            mat4: Mat4Bank::new(),
            vec4: Vec4Bank::new(),
            vec3: Vec3Bank::new(),
            scalar: ScalarBank::new(),
            int: IntBank::new(),
            dirty: DirtyFlags::new(),
        }
    }

    /// Hydrate RSI pointers into registers from Tier 2 and Tier 3 sources.
    pub async fn hydrate(
        &mut self,
        rsi_pointers: &[RsiPointer],
        zero_page: &ZeroPage,
        _resources: &ResourceTable,
    ) {
        for rsi in rsi_pointers {
            match rsi.tier {
                // Tier 1: direct register reference (already in-place)
                1 => {
                    // RSI points to a register that's already loaded — no-op.
                    // Just mark the bank dirty so the renderer knows to re-upload.
                    if let Some(bank_id) = BankId::from_u8(rsi.bank) {
                        self.dirty.set(bank_id);
                    }
                }
                // Tier 2: load from zero page into a register
                2 => {
                    let offset = rsi.index;
                    if let Some(bank_id) = BankId::from_u8(rsi.bank) {
                        match bank_id {
                            BankId::Vec3 => {
                                let x = zero_page.read_f32(offset);
                                let y = zero_page.read_f32(offset.wrapping_add(4));
                                let z = zero_page.read_f32(offset.wrapping_add(8));
                                self.vec3.write(0, [x, y, z]);
                                self.dirty.set(BankId::Vec3);
                            }
                            BankId::Scalar => {
                                let v = zero_page.read_f32(offset);
                                self.scalar.write(0, v);
                                self.dirty.set(BankId::Scalar);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Read a vec3 register by direct index — O(1) access for Test A.
    pub fn read_position(&self, bank: BankId, index: usize) -> [f32; 3] {
        match bank {
            BankId::Vec3 => *self.vec3.read(index),
            BankId::Vec4 => {
                let v = self.vec4.read(index);
                [v[0], v[1], v[2]]
            }
            _ => [0.0; 3],
        }
    }

    /// Returns the number of dirty banks (for upload costing).
    pub fn dirty_bank_count(&self) -> usize {
        [
            self.dirty.mat4,
            self.dirty.vec4,
            self.dirty.vec3,
            self.dirty.scalar,
            self.dirty.int,
        ]
        .iter()
        .filter(|&&d| d)
        .count()
    }
}

impl Default for RegisterFile {
    fn default() -> Self {
        Self::new()
    }
}
