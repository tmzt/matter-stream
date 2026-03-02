//! Tests for SCL entropy guard and Keyless invariant.

use matterstream_vm_scl::keyless::{EntropyClass, KeylessError, KeylessPolicy};
use matterstream_vm_scl::scl::{shannon_entropy, Scl, SclConfig, SclVerdict};

// --- Shannon entropy tests ---

#[test]
fn shannon_entropy_empty() {
    assert_eq!(shannon_entropy(&[]), 0.0);
}

#[test]
fn shannon_entropy_single_value() {
    // All same bytes: entropy = 0
    let data = vec![0xAA; 100];
    assert_eq!(shannon_entropy(&data), 0.0);
}

#[test]
fn shannon_entropy_two_values() {
    // Equal distribution of 2 values: entropy = 1.0 bit
    let mut data = vec![0u8; 100];
    for i in 0..50 {
        data[i] = 1;
    }
    let e = shannon_entropy(&data);
    assert!((e - 1.0).abs() < 0.01, "expected ~1.0, got {}", e);
}

#[test]
fn shannon_entropy_uniform_random() {
    // All 256 byte values equally represented: entropy should be ~8.0
    let mut data = Vec::new();
    for _ in 0..4 {
        for i in 0u16..=255 {
            data.push(i as u8);
        }
    }
    let e = shannon_entropy(&data);
    assert!((e - 8.0).abs() < 0.01, "expected ~8.0, got {}", e);
}

// --- SCL validation tests ---

#[test]
fn mtsm_opcodes_pass_validation() {
    let scl = Scl::default();
    // Structured MTSM bytecode: repeated opcodes
    let data: Vec<u8> = (0..100).map(|i| [0x00, 0x01, 0x0F, 0x07][i % 4]).collect();
    assert_eq!(scl.validate(&data), SclVerdict::Accept);
}

#[test]
fn tkv_metadata_passes() {
    let scl = Scl::default();
    // Simulated TKV: structured, low-entropy
    let mut data = Vec::new();
    for _ in 0..50 {
        data.extend_from_slice(&[0x01, 0x03, 0x00, b'k', 0x04, 0x00, 0x00, 0x00, b't', b'e', b's', b't']);
    }
    assert_eq!(scl.validate(&data), SclVerdict::Accept);
}

#[test]
fn empty_data_passes() {
    let scl = Scl::default();
    assert_eq!(scl.validate(&[]), SclVerdict::Accept);
}

#[test]
fn simulated_private_key_rejected() {
    let scl = Scl::new(SclConfig {
        entropy_threshold: 0.85,
        ..SclConfig::default()
    });
    // High-entropy pseudo-random data simulating a private key
    // Use a simple PRNG to generate pseudo-random bytes
    let mut data = vec![0u8; 256];
    let mut state: u64 = 0xDEADBEEFCAFEBABE;
    for byte in &mut data {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *byte = (state >> 33) as u8;
    }
    let verdict = scl.validate(&data);
    assert_ne!(verdict, SclVerdict::Accept, "high-entropy data should be rejected");
}

#[test]
fn validate_archive_scans_all() {
    let scl = Scl::default();
    let good = vec![0x00u8; 50]; // all zeros - structured
    let members: Vec<&[u8]> = vec![&good, &good, &good];
    let results = scl.validate_archive(&members);
    assert_eq!(results.len(), 3);
    for (_, verdict) in &results {
        assert_eq!(*verdict, SclVerdict::Accept);
    }
}

// --- Keyless tests ---

#[test]
fn keyless_classify_structured() {
    let policy = KeylessPolicy::new();
    let data = vec![0u8; 100]; // all zeros
    assert_eq!(policy.classify(&data), EntropyClass::Structured);
}

#[test]
fn keyless_classify_empty() {
    let policy = KeylessPolicy::new();
    assert_eq!(policy.classify(&[]), EntropyClass::Structured);
}

#[test]
fn keyless_assert_storable_rejects_secret() {
    let policy = KeylessPolicy::new();
    // Generate high-entropy data
    let mut data = vec![0u8; 1024];
    let mut state: u64 = 0x123456789ABCDEF0;
    for byte in &mut data {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *byte = (state >> 33) as u8;
    }
    let result = policy.assert_storable(&data);
    assert!(matches!(result, Err(KeylessError::SecretDataRejected)));
}

#[test]
fn keyless_assert_storable_accepts_structured() {
    let policy = KeylessPolicy::new();
    let data = vec![0x42u8; 100];
    let result = policy.assert_storable(&data);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), EntropyClass::Structured);
}

#[test]
fn keyless_assert_transient_accepts_all() {
    let policy = KeylessPolicy::new();
    // Even high-entropy data is OK for transient (in/out matter)
    let mut data = vec![0u8; 256];
    let mut state: u64 = 0xFEDCBA9876543210;
    for byte in &mut data {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *byte = (state >> 33) as u8;
    }
    let class = policy.assert_transient(&data);
    // Returns a class but doesn't error
    assert!(matches!(class, EntropyClass::Structured | EntropyClass::Compressed | EntropyClass::Secret));
}
