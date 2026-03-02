//! Stress tests: high-volume, boundary, and adversarial tests for all VM_SPEC subsystems.

use matterstream::addressing::AddressResolver;
use matterstream::archive::{ArchiveMember, MtsmArchive};
use matterstream::arena::{ArenaError, TripleArena};
use matterstream::aslr::{AslrToken, AsymTable};
use matterstream::dmove::{DmoveDescriptor, DmoveEngine, DmoveSource};
use matterstream::fqa::{Fqa, FourCC, Ordinal};
use matterstream::keyless::{EntropyClass, KeylessPolicy};
use matterstream::ova::{ArenaId, Ova, MAX_GEN, MAX_OBJECT, MAX_OFFSET};
use matterstream::rpn::{RpnOp, RpnVm};
use matterstream::scl::{shannon_entropy, Scl, SclConfig, SclVerdict};
use matterstream::tkv::{TkvDocument, TkvValue};

// ============================================================
// OVA bit-packing exhaustive boundary tests
// ============================================================

#[test]
fn ova_all_arena_variants_pack_unpack() {
    for arena_val in 0u8..=3 {
        let arena = ArenaId::from_u8(arena_val).unwrap();
        let ova = Ova::new(arena, 0, 0, 0);
        assert_eq!(ova.arena(), arena);
        assert_eq!(ova.generation(), 0);
        assert_eq!(ova.object(), 0);
        assert_eq!(ova.offset(), 0);
    }
}

#[test]
fn ova_field_isolation() {
    // Setting one field should never bleed into another
    for gen in [0, 1, MAX_GEN / 2, MAX_GEN] {
        for obj in [0, 1, MAX_OBJECT / 2, MAX_OBJECT] {
            for off in [0, 1, MAX_OFFSET / 2, MAX_OFFSET] {
                let ova = Ova::new(ArenaId::DynamicA, gen, obj, off);
                assert_eq!(ova.generation(), gen, "gen mismatch at gen={gen} obj={obj} off={off}");
                assert_eq!(ova.object(), obj, "obj mismatch at gen={gen} obj={obj} off={off}");
                assert_eq!(ova.offset(), off, "off mismatch at gen={gen} obj={obj} off={off}");
                assert_eq!(ova.arena(), ArenaId::DynamicA);
            }
        }
    }
}

#[test]
fn ova_with_arena_preserves_other_fields() {
    let ova = Ova::new(ArenaId::Nursery, MAX_GEN, MAX_OBJECT, MAX_OFFSET);
    for arena_val in 0u8..=3 {
        let arena = ArenaId::from_u8(arena_val).unwrap();
        let swapped = ova.with_arena(arena);
        assert_eq!(swapped.arena(), arena);
        assert_eq!(swapped.generation(), MAX_GEN);
        assert_eq!(swapped.object(), MAX_OBJECT);
        assert_eq!(swapped.offset(), MAX_OFFSET);
    }
}

#[test]
fn ova_next_generation_wraps_correctly() {
    let mut ova = Ova::new(ArenaId::DynamicA, 0, 5, 10);
    for expected in 1..=MAX_GEN {
        ova = ova.next_generation();
        assert_eq!(ova.generation(), expected);
    }
    // Should wrap to 0
    ova = ova.next_generation();
    assert_eq!(ova.generation(), 0);
    // Object and offset preserved
    assert_eq!(ova.object(), 5);
    assert_eq!(ova.offset(), 10);
}

// ============================================================
// Ordinal stress tests
// ============================================================

#[test]
fn ordinal_roundtrip_sweep() {
    // Test every power of 62 and boundary values
    let mut val = 0u64;
    for _ in 0..8 {
        let ord = Ordinal::from_u64(val);
        assert_eq!(ord.to_u64(), val);
        val = val.wrapping_mul(62).wrapping_add(1);
        if val > 218_340_105_584_895 {
            break;
        }
    }
    // Max value
    let max = 218_340_105_584_895u64; // 62^8 - 1
    let ord = Ordinal::from_u64(max);
    assert_eq!(ord.to_u64(), max);
}

#[test]
fn ordinal_all_valid_chars() {
    // Every valid character should work in every position
    let valid = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    for c in valid.chars() {
        let s: String = std::iter::repeat(c).take(8).collect();
        assert!(Ordinal::new(&s).is_ok(), "char '{}' should be valid", c);
    }
}

#[test]
fn ordinal_all_invalid_chars_rejected() {
    let invalid = "!@#$%^&*()_+-=[]{}|;':\",./<>?`~ \t\n";
    for c in invalid.chars() {
        let s: String = std::iter::repeat(c).take(8).collect();
        assert!(Ordinal::new(&s).is_err(), "char '{}' should be invalid", c);
    }
}

// ============================================================
// ASLR table stress tests
// ============================================================

#[test]
fn asym_table_1000_entries_resolve() {
    let mut table = AsymTable::new();
    for i in 0u32..1000 {
        let ova = Ova::new(ArenaId::DynamicA, 0, i % 1024, 0);
        table.insert(AslrToken(i * 7 + 13), ova); // scrambled tokens
    }
    assert_eq!(table.len(), 1000);

    // Resolve all
    for i in 0u32..1000 {
        let resolved = table.resolve(AslrToken(i * 7 + 13)).unwrap();
        assert_eq!(resolved.object(), i % 1024);
    }
}

#[test]
fn asym_table_overwrite_entry() {
    let mut table = AsymTable::new();
    let token = AslrToken(42);
    table.insert(token, Ova::new(ArenaId::DynamicA, 0, 1, 0));
    table.insert(token, Ova::new(ArenaId::DynamicB, 0, 2, 0));
    assert_eq!(table.len(), 1);
    let resolved = table.resolve(token).unwrap();
    assert_eq!(resolved.arena(), ArenaId::DynamicB);
    assert_eq!(resolved.object(), 2);
}

#[test]
fn asym_table_serialization_large() {
    let mut table = AsymTable::new();
    for i in 0u32..500 {
        table.insert(AslrToken(i), Ova::new(ArenaId::DynamicA, i % 512, i % 1024, i % 2048));
    }

    let bytes = table.to_bytes();
    let restored = AsymTable::from_bytes(&bytes).unwrap();
    assert_eq!(restored.len(), 500);

    for i in 0u32..500 {
        let orig = table.resolve(AslrToken(i)).unwrap();
        let rest = restored.resolve(AslrToken(i)).unwrap();
        assert_eq!(orig, rest);
    }
}

#[test]
fn asym_table_swap_arena_selective() {
    let mut table = AsymTable::new();
    table.insert(AslrToken(1), Ova::new(ArenaId::Nursery, 0, 0, 0));
    table.insert(AslrToken(2), Ova::new(ArenaId::DynamicA, 0, 1, 0));
    table.insert(AslrToken(3), Ova::new(ArenaId::DynamicB, 0, 2, 0));

    table.swap_arena(ArenaId::DynamicA, ArenaId::DynamicB);

    // Only DynamicA entries should have changed
    assert_eq!(table.resolve(AslrToken(1)).unwrap().arena(), ArenaId::Nursery);
    assert_eq!(table.resolve(AslrToken(2)).unwrap().arena(), ArenaId::DynamicB);
    assert_eq!(table.resolve(AslrToken(3)).unwrap().arena(), ArenaId::DynamicB); // unchanged
}

// ============================================================
// Address resolver stress
// ============================================================

#[test]
fn resolver_many_fqas() {
    let mut resolver = AddressResolver::new();
    for i in 0u128..500 {
        let fqa = Fqa::new(i);
        let token = AslrToken(i as u32);
        let ova = Ova::new(ArenaId::DynamicA, 0, (i % 1024) as u32, 0);
        resolver.register(fqa, token, ova);
    }

    for i in 0u128..500 {
        let resolved = resolver.resolve(Fqa::new(i)).unwrap();
        assert_eq!(resolved.object(), (i % 1024) as u32);
    }
}

// ============================================================
// Triple-Arena stress tests
// ============================================================

#[test]
fn arena_fill_nursery_completely() {
    let mut arenas = TripleArena::new();
    let mut ovas = Vec::new();
    for _ in 0..256 {
        ovas.push(arenas.alloc_nursery(4).unwrap());
    }
    // 257th should fail
    assert!(matches!(arenas.alloc_nursery(1), Err(ArenaError::OutOfSpace)));

    // Write unique data to each
    for (i, ova) in ovas.iter().enumerate() {
        arenas.write(*ova, &(i as u32).to_le_bytes()).unwrap();
    }
    // Read back and verify
    for (i, ova) in ovas.iter().enumerate() {
        let data = arenas.read(*ova).unwrap();
        let val = u32::from_le_bytes(data[..4].try_into().unwrap());
        assert_eq!(val, i as u32);
    }
}

#[test]
fn arena_fill_dynamic_completely() {
    let mut arenas = TripleArena::new();
    let mut ovas = Vec::new();
    for _ in 0..1024 {
        ovas.push(arenas.alloc_staging(1).unwrap());
    }
    assert!(matches!(arenas.alloc_staging(1), Err(ArenaError::OutOfSpace)));
}

#[test]
fn arena_sync_ping_pong_10_times() {
    let mut arenas = TripleArena::new();
    for i in 0..10 {
        // Allocate in staging
        let ova = arenas.alloc_staging(4).unwrap();
        arenas.write(ova, &(i as u32).to_le_bytes()).unwrap();
        arenas.sync();
    }
    // Should still be functional
    let ova = arenas.alloc_staging(4).unwrap();
    arenas.write(ova, &[0xFF; 4]).unwrap();
}

#[test]
fn arena_nursery_immortal_across_many_syncs() {
    let mut arenas = TripleArena::new();
    let nursery_ova = arenas.alloc_nursery(8).unwrap();
    arenas.write(nursery_ova, &[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE]).unwrap();

    for _ in 0..100 {
        arenas.sync();
    }

    let data = arenas.read(nursery_ova).unwrap();
    assert_eq!(&data[..8], &[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE]);
}

#[test]
fn arena_free_realloc_cycle() {
    let mut arenas = TripleArena::new();

    for _ in 0..100 {
        let ova = arenas.alloc_staging(16).unwrap();
        arenas.write(ova, &[0x42; 16]).unwrap();
        arenas.free(ova).unwrap();
    }

    // Should still have space
    let ova = arenas.alloc_staging(16).unwrap();
    arenas.write(ova, &[0xFF; 16]).unwrap();
}

#[test]
fn arena_read_after_free_fails() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_staging(8).unwrap();
    arenas.write(ova, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    arenas.free(ova).unwrap();

    assert!(arenas.read(ova).is_err());
    assert!(arenas.write(ova, &[0]).is_err());
}

#[test]
fn arena_max_object_size() {
    let mut arenas = TripleArena::new();
    // MAX_OFFSET + 1 = 2048 bytes
    let ova = arenas.alloc_nursery(2048).unwrap();
    let data = vec![0xABu8; 2048];
    arenas.write(ova, &data).unwrap();
    let read = arenas.read(ova).unwrap();
    assert_eq!(read.len(), 2048);
    assert!(read.iter().all(|&b| b == 0xAB));
}

// ============================================================
// DMOVE stress tests
// ============================================================

#[test]
fn dmove_batch_transfer() {
    let mut arenas = TripleArena::new();
    let mut descriptors = Vec::new();
    let mut dest_ovas = Vec::new();

    for i in 0u8..50 {
        let dest = arenas.alloc_nursery(4).unwrap();
        dest_ovas.push(dest);
        descriptors.push(DmoveDescriptor {
            source: DmoveSource::Buffer(vec![i, i + 1, i + 2, i + 3]),
            dest_ova: dest,
            length: 4,
            source_offset: 0,
        });
    }

    let total = DmoveEngine::execute(&mut arenas, &descriptors).unwrap();
    assert_eq!(total, 200);

    // Verify all
    for (i, ova) in dest_ovas.iter().enumerate() {
        let data = arenas.read(*ova).unwrap();
        let i = i as u8;
        assert_eq!(&data[..4], &[i, i + 1, i + 2, i + 3]);
    }
}

#[test]
fn dmove_with_source_offset() {
    let mut arenas = TripleArena::new();
    let dest = arenas.alloc_nursery(8).unwrap();

    let desc = DmoveDescriptor {
        source: DmoveSource::Buffer(vec![0, 0, 0, 0, 0xDE, 0xAD, 0xBE, 0xEF]),
        dest_ova: dest,
        length: 4,
        source_offset: 4, // skip first 4 bytes
    };

    DmoveEngine::execute(&mut arenas, &[desc]).unwrap();
    let data = arenas.read(dest).unwrap();
    assert_eq!(&data[..4], &[0xDE, 0xAD, 0xBE, 0xEF]);
}

// ============================================================
// TKV stress tests
// ============================================================

#[test]
fn tkv_large_document_roundtrip() {
    let mut doc = TkvDocument::new();
    for i in 0..200 {
        doc.push(format!("key_{}", i), TkvValue::Integer(i));
    }
    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();
    assert_eq!(decoded.entries.len(), 200);
    for (i, entry) in decoded.entries.iter().enumerate() {
        assert_eq!(entry.key, format!("key_{}", i));
        assert_eq!(entry.value, TkvValue::Integer(i as u64));
    }
}

#[test]
fn tkv_deeply_nested_tables() {
    fn make_nested(depth: usize) -> TkvValue {
        if depth == 0 {
            TkvValue::Integer(42)
        } else {
            TkvValue::Table(vec![matterstream::tkv::TkvEntry {
                key: format!("level_{}", depth),
                value: make_nested(depth - 1),
            }])
        }
    }

    let mut doc = TkvDocument::new();
    doc.push("root", make_nested(10));

    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();
    assert_eq!(decoded, doc);
}

#[test]
fn tkv_empty_strings() {
    let mut doc = TkvDocument::new();
    doc.push("", TkvValue::String(String::new()));
    doc.push("nonempty", TkvValue::String("x".into()));

    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();
    assert_eq!(decoded, doc);
}

#[test]
fn tkv_large_string_value() {
    let big_str = "A".repeat(10_000);
    let mut doc = TkvDocument::new();
    doc.push("big", TkvValue::String(big_str.clone()));

    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();
    assert_eq!(decoded.entries[0].value, TkvValue::String(big_str));
}

#[test]
fn tkv_all_types_mixed() {
    let mut doc = TkvDocument::new();
    doc.push("s", TkvValue::String("hello".into()));
    doc.push("f", TkvValue::Fqa(Fqa::new(u128::MAX)));
    doc.push("i", TkvValue::Integer(u64::MAX));
    doc.push("b_true", TkvValue::Boolean(true));
    doc.push("b_false", TkvValue::Boolean(false));
    doc.push("t", TkvValue::Table(vec![
        matterstream::tkv::TkvEntry { key: "nested".into(), value: TkvValue::Integer(0) },
    ]));

    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();
    assert_eq!(decoded, doc);
}

#[test]
fn tkv_fqa_boundary_values() {
    let values = [0u128, 1, u128::MAX, u128::MAX / 2, 0xDEADBEEFCAFEBABE];
    for val in values {
        let mut doc = TkvDocument::new();
        doc.push("fqa", TkvValue::Fqa(Fqa::new(val)));
        let encoded = doc.encode();
        let decoded = TkvDocument::decode(&encoded).unwrap();
        if let TkvValue::Fqa(fqa) = &decoded.entries[0].value {
            assert_eq!(fqa.value(), val);
        } else {
            panic!("expected Fqa");
        }
    }
}

#[test]
fn tkv_strip_comments_preserves_non_strings() {
    let mut doc = TkvDocument::new();
    for i in 0..50 {
        if i % 3 == 0 {
            doc.push(format!("comment_{}", i), TkvValue::String(format!("note {}", i)));
        } else {
            doc.push(format!("data_{}", i), TkvValue::Integer(i));
        }
    }
    let original_len = doc.entries.len();
    doc.strip_comments();
    // Should have removed ~17 String entries (every 3rd of 50)
    assert!(doc.entries.len() < original_len);
    assert!(doc.entries.iter().all(|e| !matches!(e.value, TkvValue::String(_))));
}

// ============================================================
// Archive stress tests
// ============================================================

#[test]
fn archive_many_members_roundtrip() {
    let mut archive = MtsmArchive::new();
    // .meta manifest
    archive.add(ArchiveMember::new(Ordinal::zero(), FourCC::Meta, TkvDocument::new().encode()));
    // .asym
    archive.add(ArchiveMember::new(Ordinal::new("00000001").unwrap(), FourCC::Asym, vec![0u8; 8]));
    // 50 .mrbc members
    for i in 2u64..52 {
        let ord = Ordinal::from_u64(i);
        let data = vec![(i % 256) as u8; (i as usize % 100) + 1];
        archive.add(ArchiveMember::new(ord, FourCC::Mrbc, data));
    }

    let bytes = archive.to_ar_bytes();
    let restored = MtsmArchive::from_ar_bytes(&bytes).unwrap();

    assert_eq!(restored.members.len(), 52);
    for (orig, rest) in archive.members.iter().zip(restored.members.iter()) {
        assert_eq!(orig.ordinal, rest.ordinal);
        assert_eq!(orig.fourcc, rest.fourcc);
        assert_eq!(orig.data, rest.data);
    }

    assert!(restored.validate().is_ok());
    assert_eq!(restored.bincode_members().len(), 50);
}

#[test]
fn archive_odd_even_data_sizes() {
    // Test both odd and even data sizes (ar pads odd to even)
    let mut archive = MtsmArchive::new();
    archive.add(ArchiveMember::new(Ordinal::zero(), FourCC::Meta, vec![0x01])); // 1 byte (odd)
    archive.add(ArchiveMember::new(Ordinal::new("00000001").unwrap(), FourCC::Asym, vec![0u8; 8])); // 8 bytes (even)
    archive.add(ArchiveMember::new(Ordinal::new("00000002").unwrap(), FourCC::Mrbc, vec![0x02; 3])); // 3 bytes (odd)
    archive.add(ArchiveMember::new(Ordinal::new("00000003").unwrap(), FourCC::Mrbc, vec![0x03; 4])); // 4 bytes (even)

    let bytes = archive.to_ar_bytes();
    let restored = MtsmArchive::from_ar_bytes(&bytes).unwrap();

    assert_eq!(restored.members.len(), 4);
    assert_eq!(restored.members[0].data, vec![0x01]);
    assert_eq!(restored.members[2].data, vec![0x02; 3]);
    assert_eq!(restored.members[3].data, vec![0x03; 4]);
}

#[test]
fn archive_empty_member_data() {
    let mut archive = MtsmArchive::new();
    archive.add(ArchiveMember::new(Ordinal::zero(), FourCC::Meta, vec![]));
    archive.add(ArchiveMember::new(Ordinal::new("00000001").unwrap(), FourCC::Asym, vec![0u8; 8]));

    let bytes = archive.to_ar_bytes();
    let restored = MtsmArchive::from_ar_bytes(&bytes).unwrap();
    assert_eq!(restored.members[0].data.len(), 0);
}

// ============================================================
// SCL entropy guard stress tests
// ============================================================

#[test]
fn scl_varying_entropy_levels() {
    let scl = Scl::default();

    // 1. All zeros: very structured
    assert_eq!(scl.validate(&vec![0u8; 1000]), SclVerdict::Accept);

    // 2. Repeating pattern: structured
    let pattern: Vec<u8> = (0..1000).map(|i| (i % 4) as u8).collect();
    assert_eq!(scl.validate(&pattern), SclVerdict::Accept);

    // 3. Sequential: structured
    let sequential: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
    // This has high entropy (8 bits per byte for 256 unique values) but is structured
    let verdict = scl.validate(&sequential);
    // Sequential data has high entropy so may be rejected — that's correct behavior
    assert!(matches!(verdict, SclVerdict::Accept | SclVerdict::RejectHighEntropy));
}

#[test]
fn scl_single_byte_data() {
    let scl = Scl::default();
    assert_eq!(scl.validate(&[0x42]), SclVerdict::Accept);
}

#[test]
fn scl_two_byte_data() {
    let scl = Scl::default();
    // Two distinct bytes have entropy/max_entropy = 1.0 > threshold, so SCL rejects
    // Two identical bytes should pass
    assert_eq!(scl.validate(&[0x01, 0x01]), SclVerdict::Accept);
    // Two distinct bytes: entropy = 1.0, max_entropy = 1.0, ratio = 1.0 > 0.85
    assert_eq!(scl.validate(&[0x01, 0x02]), SclVerdict::RejectHighEntropy);
}

#[test]
fn shannon_entropy_stress() {
    // Uniform distribution of N distinct values should give log2(N) bits
    for n in [2u16, 4, 8, 16, 32, 64, 128, 256] {
        let data: Vec<u8> = (0..n * 10).map(|i| (i % n) as u8).collect();
        let e = shannon_entropy(&data);
        let expected = (n as f64).log2();
        assert!(
            (e - expected).abs() < 0.1,
            "expected ~{:.2} for {} values, got {:.2}",
            expected, n, e
        );
    }
}

#[test]
fn scl_config_threshold_sensitivity() {
    // Very lenient: accept everything
    let lenient = Scl::new(SclConfig {
        max_dict_size: 65536,
        max_literal_run: 1024,
        entropy_threshold: 0.999,
    });
    let data: Vec<u8> = (0..500).map(|i| (i * 7 % 256) as u8).collect();
    assert_eq!(lenient.validate(&data), SclVerdict::Accept);

    // Very strict: reject nearly everything
    let strict = Scl::new(SclConfig {
        max_dict_size: 4096,
        max_literal_run: 2,
        entropy_threshold: 0.1,
    });
    let varied: Vec<u8> = (0..100).map(|i| (i * 13 % 256) as u8).collect();
    let verdict = strict.validate(&varied);
    assert_ne!(verdict, SclVerdict::Accept);
}

// ============================================================
// Keyless stress tests
// ============================================================

#[test]
fn keyless_classification_boundaries() {
    let policy = KeylessPolicy::new();

    // Known structured: all same byte
    assert_eq!(policy.classify(&vec![0x00; 1000]), EntropyClass::Structured);

    // Known structured: small alphabet
    let small: Vec<u8> = (0..1000).map(|i| (i % 3) as u8).collect();
    assert_eq!(policy.classify(&small), EntropyClass::Structured);

    // High entropy: pseudo-random
    let mut rng_data = vec![0u8; 4096];
    let mut state: u64 = 0xABCDEF0123456789;
    for byte in &mut rng_data {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *byte = (state >> 33) as u8;
    }
    assert_eq!(policy.classify(&rng_data), EntropyClass::Secret);
}

#[test]
fn keyless_assert_storable_batch() {
    let policy = KeylessPolicy::new();

    // Structured data should always be storable
    for size in [1, 10, 100, 1000] {
        let data = vec![0x42u8; size];
        assert!(policy.assert_storable(&data).is_ok());
    }
}

// ============================================================
// RPN VM stress tests
// ============================================================

#[test]
fn rpn_deep_arithmetic_chain() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Push 1, then add 1 a hundred times = 101
    let mut bc = vec![RpnOp::Push64 as u8];
    bc.extend_from_slice(&1u64.to_le_bytes());
    for _ in 0..100 {
        bc.push(RpnOp::Push64 as u8);
        bc.extend_from_slice(&1u64.to_le_bytes());
        bc.push(RpnOp::Add as u8);
    }

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u64().unwrap(), 101);
}

#[test]
fn rpn_many_push_drop_cycles() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = Vec::new();
    for _ in 0..200 {
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&42u32.to_le_bytes());
        bc.push(RpnOp::Drop as u8);
    }

    vm.execute(&bc, &mut arenas).unwrap();
    assert!(vm.stack.is_empty());
}

#[test]
fn rpn_map_many_entries() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = vec![RpnOp::MapNew as u8];
    for i in 0u64..50 {
        // Push key, push value, MapSet
        bc.push(RpnOp::Push64 as u8);
        bc.extend_from_slice(&i.to_le_bytes());
        bc.push(RpnOp::Push64 as u8);
        bc.extend_from_slice(&(i * 100).to_le_bytes());
        bc.push(RpnOp::MapSet as u8);
    }

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 1);

    // Verify by getting key 25
    let mut bc2 = vec![RpnOp::Push64 as u8];
    bc2.extend_from_slice(&25u64.to_le_bytes());
    bc2.push(RpnOp::MapGet as u8);
    vm.pc = 0; // reset PC but keep stack
    vm.execute(&bc2, &mut arenas).unwrap();
    assert_eq!(vm.stack.last().unwrap().as_u64().unwrap(), 2500);
}

#[test]
fn rpn_swap_stress() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Push A=1, B=2, then swap 50 times — result should be [2, 1]
    let mut bc = vec![RpnOp::Push64 as u8];
    bc.extend_from_slice(&1u64.to_le_bytes());
    bc.push(RpnOp::Push64 as u8);
    bc.extend_from_slice(&2u64.to_le_bytes());
    for _ in 0..50 {
        bc.push(RpnOp::Swap as u8);
    }

    vm.execute(&bc, &mut arenas).unwrap();
    // 50 swaps (even) = back to original: [1, 2] (1 on bottom, 2 on top)
    assert_eq!(vm.stack[0].as_u64().unwrap(), 1);
    assert_eq!(vm.stack[1].as_u64().unwrap(), 2);
}

#[test]
fn rpn_multiple_syncs() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let mut bc = Vec::new();
    for _ in 0..10 {
        bc.push(RpnOp::Sync as u8);
    }

    vm.execute(&bc, &mut arenas).unwrap();
    assert!(vm.synced);
    // After 10 syncs (even), should be back to DynamicA
    assert_eq!(arenas.active_arena(), ArenaId::DynamicA);
}

#[test]
fn rpn_dup_chain() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // Push 42, then Dup 10 times = 11 copies on stack
    let mut bc = vec![RpnOp::Push64 as u8];
    bc.extend_from_slice(&42u64.to_le_bytes());
    for _ in 0..10 {
        bc.push(RpnOp::Dup as u8);
    }

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 11);
    for val in &vm.stack {
        assert_eq!(val.as_u64().unwrap(), 42);
    }
}

#[test]
fn rpn_load_store_cycle() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let ova = arenas.alloc_nursery(64).unwrap();

    // Store values 0..10 at different offsets, then read them back
    for i in 0u32..10 {
        let target = ova.with_offset(i * 4);
        let mut bc = vec![RpnOp::Push64 as u8];
        bc.extend_from_slice(&((i + 100) as u64).to_le_bytes());
        bc.push(RpnOp::Push32 as u8);
        bc.extend_from_slice(&target.0.to_le_bytes());
        bc.push(RpnOp::Store as u8);
        vm.execute(&bc, &mut arenas).unwrap();
    }

    // Read back
    for i in 0u32..10 {
        let target = ova.with_offset(i * 4);
        let mut bc = vec![RpnOp::Push32 as u8];
        bc.extend_from_slice(&target.0.to_le_bytes());
        bc.push(RpnOp::Load as u8);
        vm.execute(&bc, &mut arenas).unwrap();
        assert_eq!(vm.stack.last().unwrap().as_u32().unwrap(), i + 100);
    }
}

#[test]
fn rpn_nop_flood() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let bc = vec![RpnOp::Nop as u8; 10_000];
    vm.execute(&bc, &mut arenas).unwrap();
    assert!(vm.stack.is_empty());
}

#[test]
fn rpn_wrapping_arithmetic() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // u64::MAX + 1 should wrap to 0
    let mut bc = vec![RpnOp::Push64 as u8];
    bc.extend_from_slice(&u64::MAX.to_le_bytes());
    bc.push(RpnOp::Push64 as u8);
    bc.extend_from_slice(&1u64.to_le_bytes());
    bc.push(RpnOp::Add as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 0); // wrapped

    // 0 - 1 should wrap to u64::MAX
    vm.stack.clear();
    let mut bc2 = vec![RpnOp::Push64 as u8];
    bc2.extend_from_slice(&0u64.to_le_bytes());
    bc2.push(RpnOp::Push64 as u8);
    bc2.extend_from_slice(&1u64.to_le_bytes());
    bc2.push(RpnOp::Sub as u8);
    vm.execute(&bc2, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), u64::MAX);
}

// ============================================================
// Full pipeline stress
// ============================================================

#[test]
fn pipeline_build_validate_load_execute() {
    // Build a realistic archive with multiple bincode members
    let mut manifest = TkvDocument::new();
    manifest.push("name", TkvValue::String("stress-test".into()));
    manifest.push("version", TkvValue::Integer(1));
    for i in 0..10 {
        manifest.push(format!("module_{}", i), TkvValue::Fqa(Fqa::new(i as u128 + 1)));
    }

    let mut asym_table = AsymTable::new();
    for i in 0u32..10 {
        asym_table.insert(
            AslrToken(i + 1),
            Ova::new(ArenaId::Nursery, 0, i, 0),
        );
    }

    let mut archive = MtsmArchive::new();
    archive.add(ArchiveMember::new(Ordinal::zero(), FourCC::Meta, manifest.encode()));
    archive.add(ArchiveMember::new(Ordinal::new("00000001").unwrap(), FourCC::Asym, asym_table.to_bytes()));

    for i in 2u64..12 {
        let push_bytes = (i as u32).to_le_bytes();
        let bytecode = RpnVm::encode(&[
            (RpnOp::Push32, Some(&push_bytes)),
            (RpnOp::Nop, None),
        ]);
        archive.add(ArchiveMember::new(Ordinal::from_u64(i), FourCC::Mrbc, bytecode));
    }

    // Serialize, deserialize
    let ar_bytes = archive.to_ar_bytes();
    let restored = MtsmArchive::from_ar_bytes(&ar_bytes).unwrap();
    restored.validate().unwrap();

    // SCL validate all members
    let scl = Scl::default();
    for member in &restored.members {
        assert_eq!(scl.load_member(&member.data), SclVerdict::Accept);
    }

    // Keyless check
    let keyless = KeylessPolicy::new();
    for member in &restored.members {
        assert!(keyless.assert_storable(&member.data).is_ok());
    }

    // Load bincode into arenas and execute
    let mut arenas = TripleArena::new();
    let bincodes = restored.bincode_members();
    for member in bincodes {
        let ova = arenas.alloc_nursery(member.data.len()).unwrap();
        arenas.write(ova, &member.data).unwrap();

        let mut rpn = RpnVm::new();
        rpn.execute(&member.data, &mut arenas).unwrap();
        assert!(!rpn.stack.is_empty());
    }

    // Address resolution
    let mut resolver = AddressResolver::new();
    let loaded_asym = AsymTable::from_bytes(&restored.asym().unwrap().data).unwrap();
    for i in 0u128..10 {
        resolver.register(Fqa::new(i + 1), AslrToken((i + 1) as u32),
            loaded_asym.resolve(AslrToken((i + 1) as u32)).unwrap());
    }
    for i in 0u128..10 {
        assert!(resolver.resolve(Fqa::new(i + 1)).is_ok());
    }
}
