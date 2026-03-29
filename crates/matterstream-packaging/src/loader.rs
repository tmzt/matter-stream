//! Package loader — decodes .ctab and .stab from archives.
//!
//! Returns a `LoadedPackage` that the caller registers with the VM.
//! This avoids a dependency on the VM crate.

use crate::archive::MtsmArchive;

/// A loaded component descriptor (decoded from .ctab).
#[derive(Clone, Debug)]
pub struct LoadedComponent {
    /// FQA (u128) identifying this component.
    pub fqa: u128,
    /// Byte offset within the package bytecode blob.
    pub offset: u32,
    /// Byte length of this component's bytecode.
    pub length: u32,
    /// String base offset (within the package's string table, before merging).
    pub string_base: u32,
}

/// A fully decoded package ready for VM registration.
#[derive(Clone, Debug)]
pub struct LoadedPackage {
    /// Concatenated bytecode from all .mrbc members.
    pub bytecode: Vec<u8>,
    /// Merged string table from .stab.
    pub strings: Vec<String>,
    /// Component descriptors from .ctab.
    pub components: Vec<LoadedComponent>,
    /// Raw .osym bytes for OID binary search.
    pub osym: Option<Vec<u8>>,
}

#[derive(Debug)]
pub enum LoadError {
    MissingBytecode,
    InvalidCtab(String),
    InvalidStab(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::MissingBytecode => write!(f, "no .mrbc member in archive"),
            LoadError::InvalidCtab(e) => write!(f, "invalid .ctab: {}", e),
            LoadError::InvalidStab(e) => write!(f, "invalid .stab: {}", e),
        }
    }
}

/// Load a package from an archive.
pub fn load_package(archive: &MtsmArchive) -> Result<LoadedPackage, LoadError> {
    // 1. Load bytecode
    let mrbc_members = archive.bincode_members();
    let bytecode = if let Some(mrbc) = mrbc_members.first() {
        mrbc.data.clone()
    } else {
        return Err(LoadError::MissingBytecode);
    };

    // 2. Decode .stab (string table)
    let strings = if let Some(stab) = archive.stab() {
        decode_stab(&stab.data)?
    } else {
        Vec::new()
    };

    // 3. Decode .ctab (component table)
    let components = if let Some(ctab) = archive.ctab() {
        decode_ctab(&ctab.data)?
    } else {
        Vec::new()
    };

    // 4. Raw .osym
    let osym = archive.oid_index().map(|m| m.data.clone());

    Ok(LoadedPackage { bytecode, strings, components, osym })
}

// ── .stab codec ──────────────────────────────────────────────────────────

/// Encode a string table to .stab binary format.
///
/// Format: [count: u32][offsets: u32 × count][data: UTF-8 bytes]
pub fn encode_stab(strings: &[String]) -> Vec<u8> {
    let count = strings.len() as u32;
    let mut data = Vec::new();
    let mut offsets = Vec::with_capacity(strings.len());

    for s in strings {
        offsets.push(data.len() as u32);
        data.extend_from_slice(s.as_bytes());
    }

    let mut buf = Vec::new();
    buf.extend_from_slice(&count.to_le_bytes());
    for off in &offsets {
        buf.extend_from_slice(&off.to_le_bytes());
    }
    buf.extend_from_slice(&data);
    buf
}

/// Decode a .stab binary format to a string table.
pub fn decode_stab(data: &[u8]) -> Result<Vec<String>, LoadError> {
    if data.len() < 4 {
        return Err(LoadError::InvalidStab("too short".into()));
    }
    let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let offsets_end = 4 + count * 4;
    if data.len() < offsets_end {
        return Err(LoadError::InvalidStab("truncated offsets".into()));
    }

    let mut offsets = Vec::with_capacity(count);
    for i in 0..count {
        let start = 4 + i * 4;
        offsets.push(u32::from_le_bytes(data[start..start + 4].try_into().unwrap()) as usize);
    }

    let str_data = &data[offsets_end..];
    let mut strings = Vec::with_capacity(count);
    for i in 0..count {
        let start = offsets[i];
        let end = if i + 1 < count { offsets[i + 1] } else { str_data.len() };
        let s = std::str::from_utf8(&str_data[start..end])
            .map_err(|e| LoadError::InvalidStab(format!("invalid utf8 at string {}: {}", i, e)))?;
        strings.push(s.to_string());
    }

    Ok(strings)
}

// ── .ctab codec ──────────────────────────────────────────────────────────

const CTAB_HEADER_SIZE: usize = 8;
const CTAB_ENTRY_SIZE: usize = 28; // u128 + u32 + u32 + u32

/// Encode a component table to .ctab binary format.
///
/// Format: [count: u32][reserved: 4][entries: (fqa: u128, offset: u32, length: u32, string_base: u32) × count]
pub fn encode_ctab(components: &[LoadedComponent]) -> Vec<u8> {
    let count = components.len() as u32;
    let mut buf = Vec::with_capacity(CTAB_HEADER_SIZE + components.len() * CTAB_ENTRY_SIZE);
    buf.extend_from_slice(&count.to_le_bytes());
    buf.extend_from_slice(&[0u8; 4]); // reserved

    for c in components {
        buf.extend_from_slice(&c.fqa.to_le_bytes());
        buf.extend_from_slice(&c.offset.to_le_bytes());
        buf.extend_from_slice(&c.length.to_le_bytes());
        buf.extend_from_slice(&c.string_base.to_le_bytes());
    }
    buf
}

/// Decode a .ctab binary format to component descriptors.
pub fn decode_ctab(data: &[u8]) -> Result<Vec<LoadedComponent>, LoadError> {
    if data.len() < CTAB_HEADER_SIZE {
        return Err(LoadError::InvalidCtab("too short".into()));
    }
    let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let expected = CTAB_HEADER_SIZE + count * CTAB_ENTRY_SIZE;
    if data.len() < expected {
        return Err(LoadError::InvalidCtab(format!(
            "expected {} bytes, got {}", expected, data.len()
        )));
    }

    let mut components = Vec::with_capacity(count);
    for i in 0..count {
        let base = CTAB_HEADER_SIZE + i * CTAB_ENTRY_SIZE;
        let fqa = u128::from_le_bytes(data[base..base + 16].try_into().unwrap());
        let offset = u32::from_le_bytes(data[base + 16..base + 20].try_into().unwrap());
        let length = u32::from_le_bytes(data[base + 20..base + 24].try_into().unwrap());
        let string_base = u32::from_le_bytes(data[base + 24..base + 28].try_into().unwrap());
        components.push(LoadedComponent { fqa, offset, length, string_base });
    }

    Ok(components)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stab_roundtrip() {
        let strings = vec!["Hello".to_string(), "World".to_string(), "!".to_string()];
        let encoded = encode_stab(&strings);
        let decoded = decode_stab(&encoded).unwrap();
        assert_eq!(decoded, strings);
    }

    #[test]
    fn stab_empty() {
        let encoded = encode_stab(&[]);
        let decoded = decode_stab(&encoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn ctab_roundtrip() {
        let components = vec![
            LoadedComponent { fqa: 0xDEADBEEF0001, offset: 0, length: 100, string_base: 0 },
            LoadedComponent { fqa: 0xDEADBEEF0002, offset: 100, length: 200, string_base: 3 },
        ];
        let encoded = encode_ctab(&components);
        let decoded = decode_ctab(&encoded).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].fqa, 0xDEADBEEF0001);
        assert_eq!(decoded[0].offset, 0);
        assert_eq!(decoded[0].length, 100);
        assert_eq!(decoded[1].fqa, 0xDEADBEEF0002);
        assert_eq!(decoded[1].offset, 100);
        assert_eq!(decoded[1].string_base, 3);
    }
}
