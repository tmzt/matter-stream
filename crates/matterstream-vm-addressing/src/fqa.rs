//! FQA (Fully Qualified Address), Base60 Ordinals, and FourCC registry.

use std::fmt;

const BASE62_CHARS: &[u8; 62] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const ORDINAL_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrdinalError {
    InvalidLength(usize),
    InvalidChar(char),
}

impl fmt::Display for OrdinalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrdinalError::InvalidLength(n) => write!(f, "ordinal must be {} chars, got {}", ORDINAL_LEN, n),
            OrdinalError::InvalidChar(c) => write!(f, "invalid ordinal character: '{}'", c),
        }
    }
}

/// 8-character Base60/62 ordinal name (0-9, a-z, A-Z).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ordinal([u8; ORDINAL_LEN]);

impl Ordinal {
    pub fn new(s: &str) -> Result<Self, OrdinalError> {
        if s.len() != ORDINAL_LEN {
            return Err(OrdinalError::InvalidLength(s.len()));
        }
        let mut buf = [0u8; ORDINAL_LEN];
        for (i, c) in s.bytes().enumerate() {
            if !BASE62_CHARS.contains(&c) {
                return Err(OrdinalError::InvalidChar(c as char));
            }
            buf[i] = c;
        }
        Ok(Ordinal(buf))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap()
    }

    /// Extract high-order routing bits (first 2 chars) for SCL subpackage routing.
    pub fn prefix(&self) -> [u8; 2] {
        [self.0[0], self.0[1]]
    }

    /// Zero ordinal used for manifest position.
    pub fn zero() -> Self {
        Ordinal(*b"00000000")
    }

    /// Encode a u64 value to an ordinal using base-62 encoding.
    pub fn from_u64(mut val: u64) -> Self {
        let mut buf = [b'0'; ORDINAL_LEN];
        for i in (0..ORDINAL_LEN).rev() {
            buf[i] = BASE62_CHARS[(val % 62) as usize];
            val /= 62;
        }
        Ordinal(buf)
    }

    /// Decode an ordinal back to a u64.
    pub fn to_u64(&self) -> u64 {
        let mut val = 0u64;
        for &b in &self.0 {
            let digit = BASE62_CHARS.iter().position(|&c| c == b).unwrap() as u64;
            val = val * 62 + digit;
        }
        val
    }
}

impl fmt::Display for Ordinal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// FourCC file extension tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FourCC {
    Meta,
    Caps,
    Mrbc,
    Tsxd,
    Asym,
    Symb,
}

impl FourCC {
    pub fn as_str(&self) -> &'static str {
        match self {
            FourCC::Meta => "meta",
            FourCC::Caps => "caps",
            FourCC::Mrbc => "mrbc",
            FourCC::Tsxd => "tsxd",
            FourCC::Asym => "asym",
            FourCC::Symb => "symb",
        }
    }

    pub fn from_ext(s: &str) -> Option<Self> {
        match s {
            "meta" => Some(FourCC::Meta),
            "caps" => Some(FourCC::Caps),
            "mrbc" => Some(FourCC::Mrbc),
            "tsxd" => Some(FourCC::Tsxd),
            "asym" => Some(FourCC::Asym),
            "symb" => Some(FourCC::Symb),
            _ => None,
        }
    }
}

impl fmt::Display for FourCC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Fully Qualified Address -- sovereign identity anchor (u128).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fqa(pub u128);

impl Fqa {
    pub fn new(val: u128) -> Self {
        Fqa(val)
    }

    /// Convert to an ordinal by encoding the lower 64 bits.
    pub fn to_ordinal(&self) -> Ordinal {
        Ordinal::from_u64(self.0 as u64)
    }

    /// Reconstruct an FQA from an ordinal (lower 64 bits, upper zeroed).
    pub fn from_ordinal(ord: &Ordinal) -> Self {
        Fqa(ord.to_u64() as u128)
    }

    pub fn value(&self) -> u128 {
        self.0
    }
}

impl fmt::Display for Fqa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FQA({:#034x})", self.0)
    }
}
