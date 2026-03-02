//! Keyless invariant enforcement and entropy classification.
//!
//! Ensures that secret-class (high entropy) data is never stored at rest.
//! Only Structured and Compressed data may be persisted.

use crate::scl::shannon_entropy;
use std::fmt;

/// Entropy classification for the Keyless invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyClass {
    /// Low entropy: structured data (opcodes, TKV, etc.)
    Structured,
    /// Medium entropy: compressed or encoded data
    Compressed,
    /// High entropy: cryptographic keys, random data -- NEVER storable
    Secret,
}

impl fmt::Display for EntropyClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntropyClass::Structured => write!(f, "Structured"),
            EntropyClass::Compressed => write!(f, "Compressed"),
            EntropyClass::Secret => write!(f, "Secret"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeylessError {
    SecretDataRejected,
}

impl fmt::Display for KeylessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeylessError::SecretDataRejected => {
                write!(f, "keyless invariant violation: secret-class data cannot be stored")
            }
        }
    }
}

/// Keyless policy enforcement.
pub struct KeylessPolicy {
    /// Threshold for Structured -> Compressed boundary (bits per byte)
    compressed_threshold: f64,
    /// Threshold for Compressed -> Secret boundary (bits per byte)
    secret_threshold: f64,
}

impl KeylessPolicy {
    pub fn new() -> Self {
        Self {
            compressed_threshold: 4.0,
            secret_threshold: 7.0,
        }
    }

    /// Classify data by its entropy.
    pub fn classify(&self, data: &[u8]) -> EntropyClass {
        if data.is_empty() {
            return EntropyClass::Structured;
        }
        let entropy = shannon_entropy(data);
        if entropy >= self.secret_threshold {
            EntropyClass::Secret
        } else if entropy >= self.compressed_threshold {
            EntropyClass::Compressed
        } else {
            EntropyClass::Structured
        }
    }

    /// Assert data is storable (not Secret-class). Rejects high-entropy data.
    pub fn assert_storable(&self, data: &[u8]) -> Result<EntropyClass, KeylessError> {
        let class = self.classify(data);
        if class == EntropyClass::Secret {
            Err(KeylessError::SecretDataRejected)
        } else {
            Ok(class)
        }
    }

    /// Assert data is transient (inbound/outbound matter -- always OK).
    pub fn assert_transient(&self, data: &[u8]) -> EntropyClass {
        self.classify(data)
    }
}

impl Default for KeylessPolicy {
    fn default() -> Self {
        Self::new()
    }
}
