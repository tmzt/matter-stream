//! AR archive container: standard Unix ar format with Base60 ordinals + FourCC.

use matterstream_vm_addressing::fqa::{FourCC, Ordinal, OrdinalError};
use crate::tkv::{TkvDocument, TkvError};
use std::fmt;

const AR_MAGIC: &[u8; 8] = b"!<arch>\n";
const HEADER_SIZE: usize = 60;
const HEADER_END: &[u8; 2] = b"`\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveError {
    InvalidMagic,
    InvalidHeader,
    TruncatedData,
    InvalidOrdinal(String),
    InvalidFourCC(String),
    MissingMeta,
    MissingAsym,
    Tkv(TkvError),
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArchiveError::InvalidMagic => write!(f, "invalid AR magic"),
            ArchiveError::InvalidHeader => write!(f, "invalid AR header"),
            ArchiveError::TruncatedData => write!(f, "truncated AR data"),
            ArchiveError::InvalidOrdinal(s) => write!(f, "invalid ordinal: {}", s),
            ArchiveError::InvalidFourCC(s) => write!(f, "invalid FourCC: {}", s),
            ArchiveError::MissingMeta => write!(f, "missing .meta member at 00000000"),
            ArchiveError::MissingAsym => write!(f, "missing .asym member"),
            ArchiveError::Tkv(e) => write!(f, "TKV error: {}", e),
        }
    }
}

impl From<TkvError> for ArchiveError {
    fn from(e: TkvError) -> Self {
        ArchiveError::Tkv(e)
    }
}

/// A single archive member.
#[derive(Debug, Clone)]
pub struct ArchiveMember {
    pub ordinal: Ordinal,
    pub fourcc: FourCC,
    pub data: Vec<u8>,
}

impl ArchiveMember {
    pub fn new(ordinal: Ordinal, fourcc: FourCC, data: Vec<u8>) -> Self {
        Self { ordinal, fourcc, data }
    }

    /// Full member name: "ordinal.fourcc"
    pub fn name(&self) -> String {
        format!("{}.{}", self.ordinal.as_str(), self.fourcc.as_str())
    }
}

/// MTSM Archive container.
#[derive(Debug, Clone)]
pub struct MtsmArchive {
    pub members: Vec<ArchiveMember>,
}

impl MtsmArchive {
    pub fn new() -> Self {
        Self { members: Vec::new() }
    }

    pub fn add(&mut self, member: ArchiveMember) {
        self.members.push(member);
    }

    /// Serialize to standard Unix ar format.
    pub fn to_ar_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(AR_MAGIC);

        for member in &self.members {
            let name = member.name();
            let size = member.data.len();

            // 60-byte ASCII header:
            // name/     16 bytes (padded with spaces)
            // mtime     12 bytes
            // owner     6 bytes
            // group     6 bytes
            // mode      8 bytes
            // size      10 bytes
            // end       2 bytes (`\n)
            let mut header = [b' '; HEADER_SIZE];

            // Name field (16 bytes): "name/" padded
            let name_field = format!("{}/", name);
            let name_bytes = name_field.as_bytes();
            let copy_len = name_bytes.len().min(16);
            header[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

            // mtime (12 bytes): "0" padded
            header[16] = b'0';

            // owner (6 bytes): "0" padded
            header[28] = b'0';

            // group (6 bytes): "0" padded
            header[34] = b'0';

            // mode (8 bytes): "100644" padded
            header[40..46].copy_from_slice(b"100644");

            // size (10 bytes): decimal
            let size_str = format!("{}", size);
            let size_bytes = size_str.as_bytes();
            header[48..48 + size_bytes.len()].copy_from_slice(size_bytes);

            // end marker
            header[58..60].copy_from_slice(HEADER_END);

            buf.extend_from_slice(&header);
            buf.extend_from_slice(&member.data);

            // Pad to even boundary
            if !size.is_multiple_of(2) {
                buf.push(b'\n');
            }
        }

        buf
    }

    /// Parse from standard Unix ar format.
    pub fn from_ar_bytes(data: &[u8]) -> Result<Self, ArchiveError> {
        if data.len() < 8 || &data[..8] != AR_MAGIC {
            return Err(ArchiveError::InvalidMagic);
        }

        let mut members = Vec::new();
        let mut pos = 8;

        while pos < data.len() {
            if pos + HEADER_SIZE > data.len() {
                return Err(ArchiveError::TruncatedData);
            }

            let header = &data[pos..pos + HEADER_SIZE];

            // Validate end marker
            if &header[58..60] != HEADER_END {
                return Err(ArchiveError::InvalidHeader);
            }

            // Parse name (16 bytes, strip trailing spaces and '/')
            let name_raw = &header[..16];
            let name_str = std::str::from_utf8(name_raw)
                .map_err(|_| ArchiveError::InvalidHeader)?
                .trim_end()
                .trim_end_matches('/');

            // Parse size (10 bytes)
            let size_raw = &header[48..58];
            let size_str = std::str::from_utf8(size_raw)
                .map_err(|_| ArchiveError::InvalidHeader)?
                .trim();
            let size: usize = size_str
                .parse()
                .map_err(|_| ArchiveError::InvalidHeader)?;

            // Parse ordinal.fourcc from name
            let (ordinal, fourcc) = parse_member_name(name_str)?;

            let data_start = pos + HEADER_SIZE;
            let data_end = data_start + size;
            if data_end > data.len() {
                return Err(ArchiveError::TruncatedData);
            }

            let member_data = data[data_start..data_end].to_vec();
            members.push(ArchiveMember {
                ordinal,
                fourcc,
                data: member_data,
            });

            // Advance past data + padding
            pos = data_end;
            if !size.is_multiple_of(2) {
                pos += 1;
            }
        }

        Ok(Self { members })
    }

    /// Get the .meta manifest at ordinal 00000000, parsed as TKV.
    pub fn manifest(&self) -> Result<TkvDocument, ArchiveError> {
        let meta = self
            .members
            .iter()
            .find(|m| m.ordinal == Ordinal::zero() && m.fourcc == FourCC::Meta)
            .ok_or(ArchiveError::MissingMeta)?;
        TkvDocument::decode(&meta.data).map_err(ArchiveError::from)
    }

    /// Get the mandatory .asym member.
    pub fn asym(&self) -> Result<&ArchiveMember, ArchiveError> {
        self.members
            .iter()
            .find(|m| m.fourcc == FourCC::Asym)
            .ok_or(ArchiveError::MissingAsym)
    }

    /// Get all .mrbc (bincode) members.
    pub fn bincode_members(&self) -> Vec<&ArchiveMember> {
        self.members
            .iter()
            .filter(|m| m.fourcc == FourCC::Mrbc)
            .collect()
    }

    /// Validate the archive: check mandatory members and valid ordinals.
    pub fn validate(&self) -> Result<(), ArchiveError> {
        // Must have .meta at 00000000
        let has_meta = self
            .members
            .iter()
            .any(|m| m.ordinal == Ordinal::zero() && m.fourcc == FourCC::Meta);
        if !has_meta {
            return Err(ArchiveError::MissingMeta);
        }

        // Must have at least one .asym
        let has_asym = self.members.iter().any(|m| m.fourcc == FourCC::Asym);
        if !has_asym {
            return Err(ArchiveError::MissingAsym);
        }

        Ok(())
    }
}

impl Default for MtsmArchive {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_member_name(name: &str) -> Result<(Ordinal, FourCC), ArchiveError> {
    let dot_pos = name
        .rfind('.')
        .ok_or_else(|| ArchiveError::InvalidOrdinal(name.to_owned()))?;
    let ord_str = &name[..dot_pos];
    let ext_str = &name[dot_pos + 1..];

    let ordinal = Ordinal::new(ord_str).map_err(|e| match e {
        OrdinalError::InvalidLength(_) | OrdinalError::InvalidChar(_) => {
            ArchiveError::InvalidOrdinal(ord_str.to_owned())
        }
    })?;
    let fourcc =
        FourCC::from_ext(ext_str).ok_or_else(|| ArchiveError::InvalidFourCC(ext_str.to_owned()))?;

    Ok((ordinal, fourcc))
}
