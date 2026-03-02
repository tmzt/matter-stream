//! TKV (Type, Key-Value) binary metadata format.

use matterstream_vm_addressing::fqa::Fqa;
use std::collections::HashMap;
use std::fmt;

/// TKV type byte tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TkvType {
    String = 0x01,
    Fqa = 0x02,
    Integer = 0x03,
    Table = 0x04,
    Boolean = 0x05,
}

impl TkvType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(TkvType::String),
            0x02 => Some(TkvType::Fqa),
            0x03 => Some(TkvType::Integer),
            0x04 => Some(TkvType::Table),
            0x05 => Some(TkvType::Boolean),
            _ => None,
        }
    }
}

/// TKV value variants.
#[derive(Debug, Clone, PartialEq)]
pub enum TkvValue {
    String(std::string::String),
    Fqa(Fqa),
    Integer(u64),
    Table(Vec<TkvEntry>),
    Boolean(bool),
}

/// A single TKV entry: key string + value.
#[derive(Debug, Clone, PartialEq)]
pub struct TkvEntry {
    pub key: String,
    pub value: TkvValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TkvError {
    InvalidTypeByte(u8),
    TruncatedData,
    InvalidUtf8,
}

impl fmt::Display for TkvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TkvError::InvalidTypeByte(b) => write!(f, "invalid TKV type byte: {:#04x}", b),
            TkvError::TruncatedData => write!(f, "truncated TKV data"),
            TkvError::InvalidUtf8 => write!(f, "invalid UTF-8 in TKV string"),
        }
    }
}

/// A collection of TKV entries (a document).
#[derive(Debug, Clone, PartialEq)]
pub struct TkvDocument {
    pub entries: Vec<TkvEntry>,
}

impl TkvDocument {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn push(&mut self, key: impl Into<String>, value: TkvValue) {
        self.entries.push(TkvEntry {
            key: key.into(),
            value,
        });
    }

    /// Encode to binary wire format.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        for entry in &self.entries {
            encode_entry(&mut buf, entry);
        }
        buf
    }

    /// Decode from binary wire format.
    pub fn decode(data: &[u8]) -> Result<Self, TkvError> {
        let mut entries = Vec::new();
        let mut pos = 0;
        while pos < data.len() {
            let (entry, consumed) = decode_entry(data, pos)?;
            entries.push(entry);
            pos += consumed;
        }
        Ok(Self { entries })
    }

    /// Remove all String entries (production mode -- strip comments).
    pub fn strip_comments(&mut self) {
        self.entries.retain(|e| !matches!(e.value, TkvValue::String(_)));
    }

    /// Extract ordinal->FQA mapping from a manifest document.
    pub fn ordinal_map(&self) -> HashMap<String, Fqa> {
        let mut map = HashMap::new();
        for entry in &self.entries {
            if let TkvValue::Fqa(fqa) = &entry.value {
                map.insert(entry.key.clone(), *fqa);
            }
        }
        map
    }
}

impl Default for TkvDocument {
    fn default() -> Self {
        Self::new()
    }
}

fn encode_entry(buf: &mut Vec<u8>, entry: &TkvEntry) {
    let key_bytes = entry.key.as_bytes();
    match &entry.value {
        TkvValue::String(s) => {
            buf.push(TkvType::String as u8);
            buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(key_bytes);
            let s_bytes = s.as_bytes();
            buf.extend_from_slice(&(s_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(s_bytes);
        }
        TkvValue::Fqa(fqa) => {
            buf.push(TkvType::Fqa as u8);
            buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(key_bytes);
            buf.extend_from_slice(&fqa.0.to_le_bytes());
        }
        TkvValue::Integer(n) => {
            buf.push(TkvType::Integer as u8);
            buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(key_bytes);
            buf.extend_from_slice(&n.to_le_bytes());
        }
        TkvValue::Table(entries) => {
            buf.push(TkvType::Table as u8);
            buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(key_bytes);
            buf.extend_from_slice(&(entries.len() as u32).to_le_bytes());
            for sub_entry in entries {
                encode_entry(buf, sub_entry);
            }
        }
        TkvValue::Boolean(b) => {
            buf.push(TkvType::Boolean as u8);
            buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(key_bytes);
            buf.push(if *b { 1 } else { 0 });
        }
    }
}

fn read_u8(data: &[u8], pos: usize) -> Result<u8, TkvError> {
    data.get(pos).copied().ok_or(TkvError::TruncatedData)
}

fn read_u16_le(data: &[u8], pos: usize) -> Result<u16, TkvError> {
    if pos + 2 > data.len() {
        return Err(TkvError::TruncatedData);
    }
    Ok(u16::from_le_bytes(data[pos..pos + 2].try_into().unwrap()))
}

fn read_u32_le(data: &[u8], pos: usize) -> Result<u32, TkvError> {
    if pos + 4 > data.len() {
        return Err(TkvError::TruncatedData);
    }
    Ok(u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()))
}

fn read_u64_le(data: &[u8], pos: usize) -> Result<u64, TkvError> {
    if pos + 8 > data.len() {
        return Err(TkvError::TruncatedData);
    }
    Ok(u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap()))
}

fn read_u128_le(data: &[u8], pos: usize) -> Result<u128, TkvError> {
    if pos + 16 > data.len() {
        return Err(TkvError::TruncatedData);
    }
    Ok(u128::from_le_bytes(data[pos..pos + 16].try_into().unwrap()))
}

fn read_bytes(data: &[u8], pos: usize, len: usize) -> Result<&[u8], TkvError> {
    if pos + len > data.len() {
        return Err(TkvError::TruncatedData);
    }
    Ok(&data[pos..pos + len])
}

fn decode_entry(data: &[u8], pos: usize) -> Result<(TkvEntry, usize), TkvError> {
    let type_byte = read_u8(data, pos)?;
    let tkv_type = TkvType::from_u8(type_byte).ok_or(TkvError::InvalidTypeByte(type_byte))?;

    let key_len = read_u16_le(data, pos + 1)? as usize;
    let key_bytes = read_bytes(data, pos + 3, key_len)?;
    let key = std::str::from_utf8(key_bytes)
        .map_err(|_| TkvError::InvalidUtf8)?
        .to_owned();

    let val_pos = pos + 3 + key_len;

    match tkv_type {
        TkvType::String => {
            let str_len = read_u32_le(data, val_pos)? as usize;
            let str_bytes = read_bytes(data, val_pos + 4, str_len)?;
            let s = std::str::from_utf8(str_bytes)
                .map_err(|_| TkvError::InvalidUtf8)?
                .to_owned();
            let consumed = 3 + key_len + 4 + str_len;
            Ok((TkvEntry { key, value: TkvValue::String(s) }, consumed))
        }
        TkvType::Fqa => {
            let val = read_u128_le(data, val_pos)?;
            let consumed = 3 + key_len + 16;
            Ok((TkvEntry { key, value: TkvValue::Fqa(Fqa::new(val)) }, consumed))
        }
        TkvType::Integer => {
            let val = read_u64_le(data, val_pos)?;
            let consumed = 3 + key_len + 8;
            Ok((TkvEntry { key, value: TkvValue::Integer(val) }, consumed))
        }
        TkvType::Table => {
            let count = read_u32_le(data, val_pos)? as usize;
            // Cap pre-allocation to remaining data length to avoid OOM from untrusted counts
            let remaining = data.len().saturating_sub(val_pos + 4);
            let mut sub_entries = Vec::with_capacity(count.min(remaining));
            let mut sub_pos = val_pos + 4;
            for _ in 0..count {
                let (entry, consumed) = decode_entry(data, sub_pos)?;
                sub_entries.push(entry);
                sub_pos += consumed;
            }
            let consumed = sub_pos - pos;
            Ok((TkvEntry { key, value: TkvValue::Table(sub_entries) }, consumed))
        }
        TkvType::Boolean => {
            let val = read_u8(data, val_pos)? != 0;
            let consumed = 3 + key_len + 1;
            Ok((TkvEntry { key, value: TkvValue::Boolean(val) }, consumed))
        }
    }
}
