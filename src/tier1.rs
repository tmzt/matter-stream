//! Tier 1 — Typed Register Banks (CPU Registers analog)
//!
//! Fixed-size typed banks: MAT4, VEC4, VEC3, SCL, INT.

/// Number of registers per bank.
pub const BANK_SIZE: usize = 16;

/// Bank identifier for dirty tracking and RSI resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BankId {
    Mat4 = 0,
    Vec4 = 1,
    Vec3 = 2,
    Scalar = 3,
    Int = 4,
}

impl BankId {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Mat4),
            1 => Some(Self::Vec4),
            2 => Some(Self::Vec3),
            3 => Some(Self::Scalar),
            4 => Some(Self::Int),
            _ => None,
        }
    }
}

/// 4x4 matrix bank — 16 registers of `[f32; 16]`.
#[derive(Debug, Clone)]
pub struct Mat4Bank {
    pub data: [[f32; 16]; BANK_SIZE],
}

impl Mat4Bank {
    pub fn new() -> Self {
        Self {
            data: [Self::identity(); BANK_SIZE],
        }
    }

    pub const fn identity() -> [f32; 16] {
        [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]
    }

    pub fn read(&self, index: usize) -> &[f32; 16] {
        &self.data[index]
    }

    pub fn write(&mut self, index: usize, value: [f32; 16]) {
        self.data[index] = value;
    }

    /// Byte size of a single register.
    pub const fn register_bytes() -> usize {
        64 // 16 * 4
    }
}

impl Default for Mat4Bank {
    fn default() -> Self {
        Self::new()
    }
}

/// Vec4 bank — 16 registers of `[f32; 4]`.
#[derive(Debug, Clone)]
pub struct Vec4Bank {
    pub data: [[f32; 4]; BANK_SIZE],
}

impl Vec4Bank {
    pub fn new() -> Self {
        Self {
            data: [[0.0; 4]; BANK_SIZE],
        }
    }

    pub fn read(&self, index: usize) -> &[f32; 4] {
        &self.data[index]
    }

    pub fn write(&mut self, index: usize, value: [f32; 4]) {
        self.data[index] = value;
    }
}

impl Default for Vec4Bank {
    fn default() -> Self {
        Self::new()
    }
}

/// Vec3 bank — 16 registers of `[f32; 3]`.
#[derive(Debug, Clone)]
pub struct Vec3Bank {
    pub data: [[f32; 3]; BANK_SIZE],
}

impl Vec3Bank {
    pub fn new() -> Self {
        Self {
            data: [[0.0; 3]; BANK_SIZE],
        }
    }

    pub fn read(&self, index: usize) -> &[f32; 3] {
        &self.data[index]
    }

    pub fn write(&mut self, index: usize, value: [f32; 3]) {
        self.data[index] = value;
    }

    /// Byte size of a single register.
    pub const fn register_bytes() -> usize {
        12 // 3 * 4
    }
}

impl Default for Vec3Bank {
    fn default() -> Self {
        Self::new()
    }
}

/// Scalar bank — 16 registers of `f32`.
#[derive(Debug, Clone)]
pub struct ScalarBank {
    pub data: [f32; BANK_SIZE],
}

impl ScalarBank {
    pub fn new() -> Self {
        Self {
            data: [0.0; BANK_SIZE],
        }
    }

    pub fn read(&self, index: usize) -> f32 {
        self.data[index]
    }

    pub fn write(&mut self, index: usize, value: f32) {
        self.data[index] = value;
    }
}

impl Default for ScalarBank {
    fn default() -> Self {
        Self::new()
    }
}

/// Integer bank — 16 registers of `i32`.
#[derive(Debug, Clone)]
pub struct IntBank {
    pub data: [i32; BANK_SIZE],
}

impl IntBank {
    pub fn new() -> Self {
        Self {
            data: [0; BANK_SIZE],
        }
    }

    pub fn read(&self, index: usize) -> i32 {
        self.data[index]
    }

    pub fn write(&mut self, index: usize, value: i32) {
        self.data[index] = value;
    }
}

impl Default for IntBank {
    fn default() -> Self {
        Self::new()
    }
}
