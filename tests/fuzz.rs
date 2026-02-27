//! Fuzz tests: randomized/adversarial input fuzzing for all parsers and decoders.
//!
//! Uses a simple PRNG (xorshift) instead of an external crate to generate
//! pseudo-random byte sequences that exercise error paths.

struct Xorshift64(u64);

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_u8(&mut self) -> u8 {
        self.next() as u8
    }

    fn next_bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u8()).collect()
    }

    fn next_range(&mut self, max: u64) -> u64 {
        self.next() % (max + 1)
    }
}

// ============================================================
// TKV decode fuzzing
// ============================================================

use matterstream::tkv::{TkvDocument, TkvError};

#[test]
fn fuzz_tkv_random_bytes_no_panic() {
    // Feed random bytes into TKV decoder — should never panic, only return errors
    for seed in 0u64..1000 {
        let mut rng = Xorshift64::new(seed + 1);
        let len = rng.next_range(200) as usize;
        let data = rng.next_bytes(len);
        let _ = TkvDocument::decode(&data); // must not panic
    }
}

#[test]
fn fuzz_tkv_valid_prefix_random_payload() {
    // Start with valid type bytes, random key lengths and payloads
    let type_bytes: [u8; 5] = [0x01, 0x02, 0x03, 0x04, 0x05];

    for seed in 0u64..500 {
        let mut rng = Xorshift64::new(seed + 1);
        let mut data = Vec::new();

        // Valid type byte
        data.push(type_bytes[rng.next_range(4) as usize]);
        // Random key length (may be too large)
        data.extend_from_slice(&(rng.next_range(100) as u16).to_le_bytes());
        // Random remaining bytes
        let extra = rng.next_range(50) as usize;
        data.extend_from_slice(&rng.next_bytes(extra));

        let _ = TkvDocument::decode(&data); // must not panic
    }
}

#[test]
fn fuzz_tkv_truncated_at_every_position() {
    // Create a valid TKV document, then truncate at every byte position
    let mut doc = TkvDocument::new();
    doc.push("test_key", matterstream::tkv::TkvValue::String("hello world".into()));
    doc.push("number", matterstream::tkv::TkvValue::Integer(42));
    doc.push("fqa", matterstream::tkv::TkvValue::Fqa(matterstream::fqa::Fqa::new(0x1234)));
    doc.push("flag", matterstream::tkv::TkvValue::Boolean(true));

    let full = doc.encode();

    for truncate_at in 0..full.len() {
        let truncated = &full[..truncate_at];
        let result = TkvDocument::decode(truncated);
        // Truncated data should either succeed (if it happens at an entry boundary) or error
        match result {
            Ok(_) => {} // valid partial document
            Err(TkvError::TruncatedData) => {} // expected
            Err(TkvError::InvalidTypeByte(_)) => {} // also possible
            Err(TkvError::InvalidUtf8) => {} // possible if cut mid-string
        }
    }
}

#[test]
fn fuzz_tkv_all_invalid_type_bytes() {
    // Every type byte not in {0x01..0x05} should be rejected
    for b in 0u8..=255 {
        if (0x01..=0x05).contains(&b) {
            continue;
        }
        let data = [b, 0x01, 0x00, b'k']; // type, key_len=1, key='k'
        let result = TkvDocument::decode(&data);
        assert!(matches!(result, Err(TkvError::InvalidTypeByte(_))),
            "type byte {:#04x} should be invalid", b);
    }
}

#[test]
fn fuzz_tkv_nested_table_depth_bomb() {
    // Craft deeply nested table bytes manually to test stack safety
    let mut data = Vec::new();
    for _ in 0..50 {
        data.push(0x04); // Table type
        data.extend_from_slice(&1u16.to_le_bytes()); // key_len = 1
        data.push(b'x'); // key
        data.extend_from_slice(&1u32.to_le_bytes()); // count = 1 sub-entry
    }
    // Terminate with a leaf Integer
    data.push(0x03); // Integer type
    data.extend_from_slice(&1u16.to_le_bytes()); // key_len = 1
    data.push(b'v'); // key
    data.extend_from_slice(&42u64.to_le_bytes()); // value

    let result = TkvDocument::decode(&data);
    // Should either succeed (deep nesting) or return truncation error, never panic
    assert!(result.is_ok() || matches!(result, Err(TkvError::TruncatedData)));
}

// ============================================================
// AR archive fuzzing
// ============================================================

use matterstream::archive::MtsmArchive;

#[test]
fn fuzz_archive_random_bytes_no_panic() {
    for seed in 0u64..1000 {
        let mut rng = Xorshift64::new(seed + 1);
        let len = rng.next_range(300) as usize;
        let data = rng.next_bytes(len);
        let _ = MtsmArchive::from_ar_bytes(&data); // must not panic
    }
}

#[test]
fn fuzz_archive_valid_magic_random_body() {
    for seed in 0u64..500 {
        let mut rng = Xorshift64::new(seed + 1);
        let mut data = b"!<arch>\n".to_vec();
        let extra = rng.next_range(200) as usize;
        data.extend_from_slice(&rng.next_bytes(extra));
        let _ = MtsmArchive::from_ar_bytes(&data); // must not panic
    }
}

#[test]
fn fuzz_archive_truncated_header() {
    // Valid magic + partial header
    for len in 8..68 {
        let mut data = b"!<arch>\n".to_vec();
        data.extend_from_slice(&vec![b' '; len - 8]);
        let _ = MtsmArchive::from_ar_bytes(&data); // must not panic
    }
}

#[test]
fn fuzz_archive_corrupted_size_field() {
    // Build a valid archive, then corrupt the size field
    let mut archive = MtsmArchive::new();
    archive.add(matterstream::archive::ArchiveMember::new(
        matterstream::fqa::Ordinal::zero(),
        matterstream::fqa::FourCC::Meta,
        vec![0u8; 10],
    ));
    let mut bytes = archive.to_ar_bytes();

    // Size field is at offset 8 + 48 = 56, 10 bytes
    if bytes.len() > 58 {
        // Set size to something huge
        bytes[56..58].copy_from_slice(b"99");
    }
    let _ = MtsmArchive::from_ar_bytes(&bytes); // must not panic
}

// ============================================================
// ASLR from_bytes fuzzing
// ============================================================

use matterstream::aslr::AsymTable;

#[test]
fn fuzz_asym_random_bytes_no_panic() {
    for seed in 0u64..1000 {
        let mut rng = Xorshift64::new(seed + 1);
        let len = rng.next_range(100) as usize;
        let data = rng.next_bytes(len);
        let _ = AsymTable::from_bytes(&data); // must not panic
    }
}

#[test]
fn fuzz_asym_valid_header_random_entries() {
    for seed in 0u64..500 {
        let mut rng = Xorshift64::new(seed + 1);
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_le_bytes()); // generation
        let count = rng.next_range(50) as u32;
        data.extend_from_slice(&count.to_le_bytes());
        // Random entry data (may be truncated)
        let extra = rng.next_range(count as u64 * 8 + 10) as usize;
        data.extend_from_slice(&rng.next_bytes(extra));
        let _ = AsymTable::from_bytes(&data); // must not panic
    }
}

#[test]
fn fuzz_asym_huge_count() {
    // Count claims 1 million entries but data is only 8 bytes
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1_000_000u32.to_le_bytes());
    let result = AsymTable::from_bytes(&data);
    assert!(result.is_err()); // should fail with truncated data
}

// ============================================================
// RPN VM fuzzing
// ============================================================

use matterstream::rpn::RpnVm;
use matterstream::arena::TripleArena;

#[test]
fn fuzz_rpn_random_bytecode_no_panic() {
    for seed in 0u64..2000 {
        let mut rng = Xorshift64::new(seed + 1);
        let len = rng.next_range(100) as usize;
        let bytecode = rng.next_bytes(len);

        let mut vm = RpnVm::new();
        let mut arenas = TripleArena::new();
        let _ = vm.execute(&bytecode, &mut arenas); // must not panic
    }
}

#[test]
fn fuzz_rpn_valid_opcode_random_payload() {
    // Start with valid opcodes, random inline data
    let valid_ops: &[u8] = &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
                              0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
                              0x10, 0x11, 0x12];

    for seed in 0u64..500 {
        let mut rng = Xorshift64::new(seed + 1);
        let mut bytecode = Vec::new();
        let num_ops = rng.next_range(20) as usize;
        for _ in 0..num_ops {
            let op = valid_ops[rng.next_range(valid_ops.len() as u64 - 1) as usize];
            bytecode.push(op);
            // Add random payload bytes
            let payload = rng.next_range(16) as usize;
            bytecode.extend_from_slice(&rng.next_bytes(payload));
        }

        let mut vm = RpnVm::new();
        vm.max_stack_depth = 64; // prevent OOM from stack explosion
        let mut arenas = TripleArena::new();
        let _ = vm.execute(&bytecode, &mut arenas); // must not panic
    }
}

#[test]
fn fuzz_rpn_only_push_ops_stack_overflow() {
    // Fill the stack to the limit with Push32 ops
    let mut bytecode = Vec::new();
    for i in 0u32..300 {
        bytecode.push(0x01); // Push32
        bytecode.extend_from_slice(&i.to_le_bytes());
    }

    let mut vm = RpnVm::new();
    vm.max_stack_depth = 256;
    let mut arenas = TripleArena::new();
    let result = vm.execute(&bytecode, &mut arenas);
    // Should hit stack overflow, not panic
    assert!(result.is_err());
}

#[test]
fn fuzz_rpn_alternating_push_underflow() {
    // Alternate push and arithmetic to stress type checking
    let ops_needing_two: &[u8] = &[0x07, 0x08, 0x09, 0x0A]; // Add, Sub, Mul, Div

    for seed in 0u64..200 {
        let mut rng = Xorshift64::new(seed + 1);
        let mut bytecode = Vec::new();

        for _ in 0..20 {
            if rng.next_range(1) == 0 {
                bytecode.push(0x02); // Push64
                bytecode.extend_from_slice(&rng.next().to_le_bytes());
            } else {
                bytecode.push(ops_needing_two[rng.next_range(3) as usize]);
            }
        }

        let mut vm = RpnVm::new();
        let mut arenas = TripleArena::new();
        let _ = vm.execute(&bytecode, &mut arenas); // must not panic
    }
}

#[test]
fn fuzz_rpn_call_ret_random_targets() {
    // Call with random addresses — should handle gracefully
    for seed in 0u64..200 {
        let mut rng = Xorshift64::new(seed + 1);

        let mut bytecode = Vec::new();
        // Push random target, Call
        bytecode.push(0x02); // Push64
        let target = rng.next_range(500);
        bytecode.extend_from_slice(&target.to_le_bytes());
        bytecode.push(0x0D); // Call
        // Ret
        bytecode.push(0x0E); // Ret
        // Some nops
        for _ in 0..10 {
            bytecode.push(0x00); // Nop
        }

        let mut vm = RpnVm::new();
        let mut arenas = TripleArena::new();
        let _ = vm.execute(&bytecode, &mut arenas); // must not panic
    }
}

#[test]
fn fuzz_rpn_decode_random_bytecode() {
    for seed in 0u64..500 {
        let mut rng = Xorshift64::new(seed + 1);
        let len = rng.next_range(100) as usize;
        let bytecode = rng.next_bytes(len);
        let _ = RpnVm::decode(&bytecode); // must not panic
    }
}

// ============================================================
// SCL validation fuzzing
// ============================================================

use matterstream::scl::Scl;

#[test]
fn fuzz_scl_random_data_no_panic() {
    let scl = Scl::default();
    for seed in 0u64..1000 {
        let mut rng = Xorshift64::new(seed + 1);
        let len = rng.next_range(500) as usize;
        let data = rng.next_bytes(len);
        let _ = scl.validate(&data); // must not panic
    }
}

#[test]
fn fuzz_scl_extreme_configs() {
    // Test with various extreme config values
    let configs = vec![
        matterstream::scl::SclConfig { max_dict_size: 1, max_literal_run: 1, entropy_threshold: 0.0 },
        matterstream::scl::SclConfig { max_dict_size: 256, max_literal_run: 0, entropy_threshold: 1.0 },
        matterstream::scl::SclConfig { max_dict_size: 100_000, max_literal_run: 100_000, entropy_threshold: 0.5 },
        matterstream::scl::SclConfig { max_dict_size: 4096, max_literal_run: 64, entropy_threshold: 0.0001 },
    ];

    for config in configs {
        let scl = Scl::new(config);
        for seed in 0u64..50 {
            let mut rng = Xorshift64::new(seed + 1);
            let data = rng.next_bytes(100);
            let _ = scl.validate(&data); // must not panic
        }
    }
}

#[test]
fn fuzz_shannon_entropy_single_bytes() {
    // Shannon entropy on every possible single byte
    for b in 0u8..=255 {
        let e = matterstream::scl::shannon_entropy(&[b]);
        assert_eq!(e, 0.0, "single byte should have 0 entropy");
    }
}

#[test]
fn fuzz_shannon_entropy_two_bytes() {
    for a in 0u8..=255 {
        let e = matterstream::scl::shannon_entropy(&[a, a]);
        assert_eq!(e, 0.0, "two same bytes should have 0 entropy");

        if a < 255 {
            let e2 = matterstream::scl::shannon_entropy(&[a, a.wrapping_add(1)]);
            assert!((e2 - 1.0).abs() < 0.01, "two different bytes should have ~1.0 entropy, got {}", e2);
        }
    }
}

// ============================================================
// Keyless fuzzing
// ============================================================

use matterstream::keyless::KeylessPolicy;

#[test]
fn fuzz_keyless_random_data_classify() {
    let policy = KeylessPolicy::new();
    for seed in 0u64..500 {
        let mut rng = Xorshift64::new(seed + 1);
        let len = rng.next_range(1000) as usize + 1;
        let data = rng.next_bytes(len);
        let class = policy.classify(&data);
        // Should be one of the three classes, never panic
        assert!(matches!(class,
            matterstream::keyless::EntropyClass::Structured |
            matterstream::keyless::EntropyClass::Compressed |
            matterstream::keyless::EntropyClass::Secret
        ));
    }
}

#[test]
fn fuzz_keyless_assert_storable_consistency() {
    let policy = KeylessPolicy::new();
    for seed in 0u64..500 {
        let mut rng = Xorshift64::new(seed + 1);
        let len = rng.next_range(500) as usize + 1;
        let data = rng.next_bytes(len);

        let class = policy.classify(&data);
        let storable = policy.assert_storable(&data);

        // Consistency: Secret => Err, otherwise => Ok
        match class {
            matterstream::keyless::EntropyClass::Secret => {
                assert!(storable.is_err());
            }
            _ => {
                assert!(storable.is_ok());
                assert_eq!(storable.unwrap(), class);
            }
        }
    }
}

// ============================================================
// Cross-module adversarial tests
// ============================================================

#[test]
fn adversarial_ova_from_raw_u32() {
    // Construct OVAs from all possible raw u32 patterns in key ranges
    let test_values: Vec<u32> = vec![
        0, 1, u32::MAX, u32::MAX / 2,
        0x80000000, 0x7FFFFFFF, 0xFF, 0xFF00, 0xFF0000, 0xFF000000,
        0xAAAAAAAA, 0x55555555, 0xDEADBEEF, 0xCAFEBABE,
    ];

    for val in test_values {
        let ova = matterstream::ova::Ova(val);
        // Extracting fields must not panic
        let _ = ova.arena();
        let _ = ova.generation();
        let _ = ova.object();
        let _ = ova.offset();

        // with_arena must not corrupt other fields
        let swapped = ova.with_arena(matterstream::ova::ArenaId::DynamicB);
        assert_eq!(swapped.generation(), ova.generation());
        assert_eq!(swapped.object(), ova.object());
        assert_eq!(swapped.offset(), ova.offset());
    }
}

#[test]
fn adversarial_dmove_zero_length() {
    let mut arenas = TripleArena::new();
    let dest = arenas.alloc_nursery(8).unwrap();

    // Zero-length transfer
    let desc = matterstream::dmove::DmoveDescriptor {
        source: matterstream::dmove::DmoveSource::Buffer(vec![]),
        dest_ova: dest,
        length: 0,
        source_offset: 0,
    };

    let result = matterstream::dmove::DmoveEngine::execute(&mut arenas, &[desc]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0);
}

#[test]
fn adversarial_dmove_empty_descriptor_list() {
    let mut arenas = TripleArena::new();
    let result = matterstream::dmove::DmoveEngine::execute(&mut arenas, &[]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0);
}

#[test]
fn adversarial_archive_all_fourcc_types() {
    let fourccs = [
        matterstream::fqa::FourCC::Meta,
        matterstream::fqa::FourCC::Caps,
        matterstream::fqa::FourCC::Mrbc,
        matterstream::fqa::FourCC::Tsxd,
        matterstream::fqa::FourCC::Asym,
        matterstream::fqa::FourCC::Symb,
    ];

    let mut archive = MtsmArchive::new();
    // Add .meta at zero for validation
    archive.add(matterstream::archive::ArchiveMember::new(
        matterstream::fqa::Ordinal::zero(),
        matterstream::fqa::FourCC::Meta,
        matterstream::tkv::TkvDocument::new().encode(),
    ));

    for (i, &fourcc) in fourccs.iter().enumerate().skip(1) {
        archive.add(matterstream::archive::ArchiveMember::new(
            matterstream::fqa::Ordinal::from_u64(i as u64 + 1),
            fourcc,
            vec![0u8; i + 1],
        ));
    }

    let bytes = archive.to_ar_bytes();
    let restored = MtsmArchive::from_ar_bytes(&bytes).unwrap();
    assert_eq!(restored.members.len(), fourccs.len());

    for (orig, rest) in archive.members.iter().zip(restored.members.iter()) {
        assert_eq!(orig.fourcc, rest.fourcc);
        assert_eq!(orig.data, rest.data);
    }
}
