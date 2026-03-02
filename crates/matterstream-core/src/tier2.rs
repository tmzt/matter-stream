//! Tier 2 — Zero Page (Direct RAM analog)
//!
//! 256-byte direct-addressing storage buffer for instance-local state.
//! 16-byte aligned per spec.

/// 6502-style zero page: 256 bytes, 16-byte aligned.
#[derive(Debug, Clone)]
#[repr(align(16))]
pub struct ZeroPage {
    data: [u8; 256],
}

impl ZeroPage {
    pub fn new() -> Self {
        Self { data: [0u8; 256] }
    }

    /// Read a single byte at `offset`.
    pub fn read_u8(&self, offset: u8) -> u8 {
        self.data[offset as usize]
    }

    /// Write a single byte at `offset`.
    pub fn write_u8(&mut self, offset: u8, value: u8) {
        self.data[offset as usize] = value;
    }

    /// Read `N` bytes starting at `offset`.
    pub fn read_bytes(&self, offset: u8, len: usize) -> &[u8] {
        let start = offset as usize;
        &self.data[start..start + len]
    }

    /// Write a byte slice starting at `offset`.
    pub fn write_bytes(&mut self, offset: u8, bytes: &[u8]) {
        let start = offset as usize;
        self.data[start..start + bytes.len()].copy_from_slice(bytes);
    }

    /// Read an f32 at a 4-byte-aligned offset.
    pub fn read_f32(&self, offset: u8) -> f32 {
        let start = offset as usize;
        let bytes: [u8; 4] = self.data[start..start + 4].try_into().unwrap();
        f32::from_le_bytes(bytes)
    }

    /// Write an f32 at offset.
    pub fn write_f32(&mut self, offset: u8, value: f32) {
        let start = offset as usize;
        self.data[start..start + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Raw access to the full buffer.
    pub fn as_bytes(&self) -> &[u8; 256] {
        &self.data
    }
}

impl Default for ZeroPage {
    fn default() -> Self {
        Self::new()
    }
}
