//! DMOVE scatter-gather DMA engine.
//!
//! Writes data directly to OVA addresses in arenas.
//! Secrets never enter guest logic -- DMOVE writes directly.

use crate::arena::{ArenaError, TripleArena};
use crate::ova::Ova;
use std::fmt;

/// Source of data for a DMOVE transfer.
#[derive(Debug, Clone)]
pub enum DmoveSource {
    /// Inline buffer data.
    Buffer(Vec<u8>),
    /// Reference to a nursery object (cross-arena copy).
    NurseryRef(Ova),
    /// External API endpoint (identified by hash).
    ExternalApi { endpoint_hash: u64 },
}

/// A single DMOVE transfer descriptor.
#[derive(Debug, Clone)]
pub struct DmoveDescriptor {
    pub source: DmoveSource,
    pub dest_ova: Ova,
    pub length: usize,
    pub source_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DmoveError {
    Arena(ArenaError),
    SourceTooShort { available: usize, requested: usize },
    ExternalApiNotSupported,
}

impl fmt::Display for DmoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DmoveError::Arena(e) => write!(f, "DMOVE arena error: {}", e),
            DmoveError::SourceTooShort { available, requested } => {
                write!(f, "DMOVE source too short: {} available, {} requested", available, requested)
            }
            DmoveError::ExternalApiNotSupported => write!(f, "external API DMOVE not yet supported"),
        }
    }
}

impl From<ArenaError> for DmoveError {
    fn from(e: ArenaError) -> Self {
        DmoveError::Arena(e)
    }
}

/// DMOVE engine: executes scatter-gather transfers into arenas.
pub struct DmoveEngine;

impl DmoveEngine {
    /// Execute a batch of DMOVE descriptors. Returns total bytes transferred.
    pub fn execute(
        arenas: &mut TripleArena,
        descriptors: &[DmoveDescriptor],
    ) -> Result<usize, DmoveError> {
        let mut total = 0;

        for desc in descriptors {
            let data = match &desc.source {
                DmoveSource::Buffer(buf) => {
                    let end = desc.source_offset + desc.length;
                    if end > buf.len() {
                        return Err(DmoveError::SourceTooShort {
                            available: buf.len().saturating_sub(desc.source_offset),
                            requested: desc.length,
                        });
                    }
                    buf[desc.source_offset..end].to_vec()
                }
                DmoveSource::NurseryRef(src_ova) => {
                    let src_data = arenas.read(*src_ova)?;
                    let end = desc.source_offset + desc.length;
                    if end > src_data.len() {
                        return Err(DmoveError::SourceTooShort {
                            available: src_data.len().saturating_sub(desc.source_offset),
                            requested: desc.length,
                        });
                    }
                    src_data[desc.source_offset..end].to_vec()
                }
                DmoveSource::ExternalApi { .. } => {
                    return Err(DmoveError::ExternalApiNotSupported);
                }
            };

            arenas.write(desc.dest_ova, &data)?;
            total += data.len();
        }

        Ok(total)
    }
}
