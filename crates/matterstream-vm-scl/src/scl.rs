//! Secure Code Loader: modified LZW entropy guard.
//!
//! Validates bytecode and metadata by checking that content exhibits
//! structured (low-entropy) patterns consistent with MTSM opcodes/TKV data.

use std::collections::HashMap;
use std::fmt;

/// SCL configuration thresholds.
#[derive(Debug, Clone)]
pub struct SclConfig {
    pub max_dict_size: usize,
    pub max_literal_run: usize,
    pub entropy_threshold: f64,
}

impl Default for SclConfig {
    fn default() -> Self {
        Self {
            max_dict_size: 4096,
            max_literal_run: 64,
            entropy_threshold: 0.85,
        }
    }
}

/// Result of SCL validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SclVerdict {
    Accept,
    RejectDictionaryExplosion,
    RejectLiteralEscape,
    RejectHighEntropy,
}

impl fmt::Display for SclVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SclVerdict::Accept => write!(f, "accepted"),
            SclVerdict::RejectDictionaryExplosion => write!(f, "rejected: dictionary explosion"),
            SclVerdict::RejectLiteralEscape => write!(f, "rejected: excessive literal escapes"),
            SclVerdict::RejectHighEntropy => write!(f, "rejected: high entropy"),
        }
    }
}

/// Modified LZW dictionary seeded with MTSM opcodes and TKV type bytes.
struct LzwDictionary {
    entries: HashMap<Vec<u8>, u32>,
    next_code: u32,
    max_size: usize,
}

impl LzwDictionary {
    fn new(max_size: usize) -> Self {
        let mut entries = HashMap::new();
        // Seed with all single bytes
        for i in 0u16..=255 {
            entries.insert(vec![i as u8], i as u32);
        }
        // Seed with known MTSM opcode pairs
        let mtsm_seeds: &[&[u8]] = &[
            &[0x00, 0x00], // Nop Nop
            &[0x01, 0x00], // Push32
            &[0x02, 0x00], // Push64
            &[0x0F, 0x00], // Sync
            // TKV type bytes
            &[0x01, 0x01], // String String
            &[0x02, 0x02], // Fqa Fqa
            &[0x03, 0x03], // Integer Integer
        ];
        let mut next_code = 256u32;
        for seed in mtsm_seeds {
            if (next_code as usize) < max_size {
                entries.insert(seed.to_vec(), next_code);
                next_code += 1;
            }
        }
        Self {
            entries,
            next_code,
            max_size,
        }
    }

    fn contains(&self, seq: &[u8]) -> bool {
        self.entries.contains_key(seq)
    }

    fn insert(&mut self, seq: Vec<u8>) -> bool {
        if (self.next_code as usize) >= self.max_size {
            return false; // dictionary full
        }
        self.entries.insert(seq, self.next_code);
        self.next_code += 1;
        true
    }

    fn size(&self) -> usize {
        self.entries.len()
    }

    fn reset(&mut self) {
        let max_size = self.max_size;
        *self = Self::new(max_size);
    }
}

/// Secure Code Loader.
pub struct Scl {
    pub config: SclConfig,
}

impl Scl {
    pub fn new(config: SclConfig) -> Self {
        Self { config }
    }

    /// Validate data against the SCL entropy guard.
    pub fn validate(&self, data: &[u8]) -> SclVerdict {
        if data.is_empty() {
            return SclVerdict::Accept;
        }

        // Check 1: Shannon entropy
        let entropy = shannon_entropy(data);
        let max_entropy = (data.len().min(256) as f64).log2();
        if max_entropy > 0.0 && entropy / max_entropy > self.config.entropy_threshold {
            return SclVerdict::RejectHighEntropy;
        }

        // Check 2: Modified LZW compression analysis
        let mut dict = LzwDictionary::new(self.config.max_dict_size);
        let mut literal_run = 0usize;
        let mut w: Vec<u8> = Vec::new();

        for &byte in data {
            let mut wc = w.clone();
            wc.push(byte);

            if dict.contains(&wc) {
                w = wc;
                literal_run = 0;
            } else {
                // New sequence -- add to dictionary
                if !dict.insert(wc) {
                    return SclVerdict::RejectDictionaryExplosion;
                }
                literal_run += 1;
                if literal_run > self.config.max_literal_run {
                    return SclVerdict::RejectLiteralEscape;
                }
                w = vec![byte];
            }

            // Check dictionary size hasn't exploded
            if dict.size() >= self.config.max_dict_size {
                dict.reset();
            }
        }

        SclVerdict::Accept
    }

    /// Validate a single archive member.
    pub fn load_member(&self, data: &[u8]) -> SclVerdict {
        self.validate(data)
    }

    /// Validate all members of an archive.
    pub fn validate_archive(&self, members: &[&[u8]]) -> Vec<(usize, SclVerdict)> {
        members
            .iter()
            .enumerate()
            .map(|(i, data)| (i, self.validate(data)))
            .collect()
    }
}

impl Default for Scl {
    fn default() -> Self {
        Self::new(SclConfig::default())
    }
}

/// Compute Shannon entropy of a byte slice (bits per byte, 0.0 to 8.0).
pub fn shannon_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let len = data.len() as f64;
    let mut entropy = 0.0f64;
    for &count in &counts {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }
    entropy
}
