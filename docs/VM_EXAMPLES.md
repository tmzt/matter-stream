# VM_SPEC v0.1.0 -- Tests & Examples

Complete reference for all test suites and runnable examples in the MTSM-VM implementation.

---

## Table of Contents

- [Test Suites](#test-suites)
  - [1. FQA, Ordinal, OVA & Addressing](#1-fqa-ordinal-ova--addressing)
  - [2. Triple-Arena Memory & DMOVE](#2-triple-arena-memory--dmove)
  - [3. TKV Metadata Format](#3-tkv-metadata-format)
  - [4. AR Archive Container](#4-ar-archive-container)
  - [5. SCL Entropy Guard & Keyless](#5-scl-entropy-guard--keyless)
  - [6. RPN Stack Language VM](#6-rpn-stack-language-vm)
  - [7. Integration Tests](#7-integration-tests)
- [Runnable Examples](#runnable-examples)
  - [rpn-arena -- Fibonacci Bar Chart](#rpn-arena----fibonacci-bar-chart)
  - [vm-pipeline -- Full Pipeline Demo](#vm-pipeline----full-pipeline-demo)
  - [run-tsx -- TSX Compiler + Renderer](#run-tsx----tsx-compiler--renderer)

---

## Test Suites

Run all tests:

```sh
cargo test
```

Run a specific suite:

```sh
cargo test --test rpn
cargo test --test fqa_ova
cargo test --test arena
# etc.
```

---

### 1. FQA, Ordinal, OVA & Addressing

**File:** `tests/fqa_ova.rs` (24 tests)

Tests for the core addressing types: FQA (u128 identity anchor), Base62 Ordinals, OVA bit-packed addresses, ASLR token tables, and the full resolution pipeline.

```rust
use matterstream::fqa::{Fqa, FourCC, Ordinal, OrdinalError};
use matterstream::ova::{ArenaId, Ova, OvaWide, MAX_GEN, MAX_OBJECT, MAX_OFFSET};
use matterstream::aslr::{AslrToken, AsymTable};
use matterstream::addressing::AddressResolver;

// --- Ordinal tests ---

#[test]
fn ordinal_roundtrip_base62() {
    // Max value for 8 base-62 digits: 62^8 - 1 = 218_340_105_584_895
    let values = [0u64, 1, 61, 62, 3843, 100_000, 218_340_105_584_895];
    for val in values {
        let ord = Ordinal::from_u64(val);
        let decoded = ord.to_u64();
        assert_eq!(decoded, val, "roundtrip failed for {}", val);
    }
}

#[test]
fn ordinal_new_valid() {
    let ord = Ordinal::new("abcd1234").unwrap();
    assert_eq!(ord.as_str(), "abcd1234");
}

#[test]
fn ordinal_new_invalid_length() {
    assert!(matches!(Ordinal::new("short"), Err(OrdinalError::InvalidLength(5))));
    assert!(matches!(Ordinal::new("toolongstring"), Err(OrdinalError::InvalidLength(_))));
}

#[test]
fn ordinal_new_invalid_char() {
    assert!(matches!(Ordinal::new("abcd123!"), Err(OrdinalError::InvalidChar('!'))));
}

#[test]
fn ordinal_zero() {
    let z = Ordinal::zero();
    assert_eq!(z.as_str(), "00000000");
    assert_eq!(z.to_u64(), 0);
}

#[test]
fn ordinal_prefix_extraction() {
    let ord = Ordinal::new("XYab0000").unwrap();
    let prefix = ord.prefix();
    assert_eq!(prefix, [b'X', b'Y']);
}

#[test]
fn ordinal_display() {
    let ord = Ordinal::new("test0001").unwrap();
    assert_eq!(format!("{}", ord), "test0001");
}

// --- FQA tests ---

#[test]
fn fqa_ordinal_roundtrip() {
    let fqa = Fqa::new(12345);
    let ord = fqa.to_ordinal();
    let fqa2 = Fqa::from_ordinal(&ord);
    assert_eq!(fqa2.value(), 12345);
}

#[test]
fn fqa_display() {
    let fqa = Fqa::new(0xFF);
    let s = format!("{}", fqa);
    assert!(s.contains("0x"));
}

// --- FourCC tests ---

#[test]
fn fourcc_roundtrip() {
    let variants = [FourCC::Meta, FourCC::Caps, FourCC::Mrbc,
                    FourCC::Tsxd, FourCC::Asym, FourCC::Symb];
    for v in variants {
        let s = v.as_str();
        let parsed = FourCC::from_ext(s).unwrap();
        assert_eq!(parsed, v);
    }
}

#[test]
fn fourcc_unknown_returns_none() {
    assert!(FourCC::from_ext("xxxx").is_none());
}

// --- OVA tests ---

#[test]
fn ova_pack_unpack_zero() {
    let ova = Ova::new(ArenaId::Nursery, 0, 0, 0);
    assert_eq!(ova.arena(), ArenaId::Nursery);
    assert_eq!(ova.generation(), 0);
    assert_eq!(ova.object(), 0);
    assert_eq!(ova.offset(), 0);
}

#[test]
fn ova_pack_unpack_max() {
    let ova = Ova::new(ArenaId::Reserved, MAX_GEN, MAX_OBJECT, MAX_OFFSET);
    assert_eq!(ova.arena(), ArenaId::Reserved);
    assert_eq!(ova.generation(), MAX_GEN);
    assert_eq!(ova.object(), MAX_OBJECT);
    assert_eq!(ova.offset(), MAX_OFFSET);
}

#[test]
fn ova_pack_unpack_mid() {
    let ova = Ova::new(ArenaId::DynamicA, 100, 500, 1000);
    assert_eq!(ova.arena(), ArenaId::DynamicA);
    assert_eq!(ova.generation(), 100);
    assert_eq!(ova.object(), 500);
    assert_eq!(ova.offset(), 1000);
}

#[test]
fn ova_with_arena() {
    let ova = Ova::new(ArenaId::DynamicA, 42, 10, 20);
    let swapped = ova.with_arena(ArenaId::DynamicB);
    assert_eq!(swapped.arena(), ArenaId::DynamicB);
    assert_eq!(swapped.generation(), 42);
    assert_eq!(swapped.object(), 10);
    assert_eq!(swapped.offset(), 20);
}

#[test]
fn ova_with_offset() {
    let ova = Ova::new(ArenaId::Nursery, 1, 2, 100);
    let modified = ova.with_offset(200);
    assert_eq!(modified.offset(), 200);
    assert_eq!(modified.arena(), ArenaId::Nursery);
    assert_eq!(modified.generation(), 1);
    assert_eq!(modified.object(), 2);
}

#[test]
fn ova_next_generation() {
    let ova = Ova::new(ArenaId::DynamicA, 0, 5, 0);
    let next = ova.next_generation();
    assert_eq!(next.generation(), 1);

    let max_gen = Ova::new(ArenaId::DynamicA, MAX_GEN, 5, 0);
    let wrapped = max_gen.next_generation();
    assert_eq!(wrapped.generation(), 0); // wraps
}

#[test]
fn ova_wide_conversion() {
    let ova = Ova::new(ArenaId::DynamicB, 50, 100, 200);
    let wide = OvaWide::from_ova(ova);
    let back = wide.to_ova();
    assert_eq!(back, ova);
}

// --- ASLR / AsymTable tests ---

#[test]
fn asym_table_insert_resolve() {
    let mut table = AsymTable::new();
    let token = AslrToken(0xDEAD);
    let ova = Ova::new(ArenaId::DynamicA, 1, 10, 0);
    table.insert(token, ova);

    let resolved = table.resolve(token).unwrap();
    assert_eq!(resolved, ova);
}

#[test]
fn asym_table_resolve_missing() {
    let table = AsymTable::new();
    assert!(table.resolve(AslrToken(999)).is_none());
}

#[test]
fn asym_table_swap_arena() {
    let mut table = AsymTable::new();
    let token = AslrToken(1);
    let ova = Ova::new(ArenaId::DynamicA, 0, 5, 0);
    table.insert(token, ova);

    table.swap_arena(ArenaId::DynamicA, ArenaId::DynamicB);

    let resolved = table.resolve(token).unwrap();
    assert_eq!(resolved.arena(), ArenaId::DynamicB);
    assert_eq!(resolved.object(), 5);
}

#[test]
fn asym_table_serialization_roundtrip() {
    let mut table = AsymTable::new();
    table.insert(AslrToken(10), Ova::new(ArenaId::Nursery, 0, 0, 0));
    table.insert(AslrToken(20), Ova::new(ArenaId::DynamicA, 1, 5, 100));

    let bytes = table.to_bytes();
    let restored = AsymTable::from_bytes(&bytes).unwrap();

    assert_eq!(restored.len(), 2);
    assert_eq!(
        restored.resolve(AslrToken(10)).unwrap(),
        Ova::new(ArenaId::Nursery, 0, 0, 0)
    );
    assert_eq!(
        restored.resolve(AslrToken(20)).unwrap(),
        Ova::new(ArenaId::DynamicA, 1, 5, 100)
    );
}

// --- AddressResolver tests ---

#[test]
fn full_resolution_fqa_to_ova() {
    let mut resolver = AddressResolver::new();
    let fqa = Fqa::new(42);
    let token = AslrToken(0xBEEF);
    let ova = Ova::new(ArenaId::DynamicA, 0, 7, 0);

    resolver.register(fqa, token, ova);

    let resolved = resolver.resolve(fqa).unwrap();
    assert_eq!(resolved, ova);
}

#[test]
fn resolution_unknown_fqa() {
    let resolver = AddressResolver::new();
    assert!(resolver.resolve(Fqa::new(999)).is_err());
}
```

---

### 2. Triple-Arena Memory & DMOVE

**File:** `tests/arena.rs` (11 tests)

Tests for the Nursery (256 immortal slots) + DynamicA/B (1024 each) ping-pong arenas, SYNC operations, generation validation, and the DMOVE scatter-gather DMA engine.

```rust
use matterstream::arena::{ArenaError, TripleArena};
use matterstream::dmove::{DmoveDescriptor, DmoveEngine, DmoveSource};
use matterstream::ova::ArenaId;

#[test]
fn nursery_alloc_read_roundtrip() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_nursery(16).unwrap();
    assert_eq!(ova.arena(), ArenaId::Nursery);

    arenas.write(ova, &[1, 2, 3, 4]).unwrap();
    let data = arenas.read(ova).unwrap();
    assert_eq!(&data[..4], &[1, 2, 3, 4]);
}

#[test]
fn dynamic_arena_alloc_write_read() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_staging(32).unwrap();

    let payload = b"hello arena";
    arenas.write(ova, payload).unwrap();
    let data = arenas.read(ova).unwrap();
    assert_eq!(&data[..payload.len()], payload);
}

#[test]
fn sync_swaps_active() {
    let mut arenas = TripleArena::new();
    assert_eq!(arenas.active_arena(), ArenaId::DynamicA);

    let result = arenas.sync();
    assert_eq!(result.old_active, ArenaId::DynamicA);
    assert_eq!(result.new_active, ArenaId::DynamicB);

    let result2 = arenas.sync();
    assert_eq!(result2.new_active, ArenaId::DynamicA);
}

#[test]
fn generation_mismatch_rejected() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_nursery(8).unwrap();

    let bad_ova = ova.next_generation();
    let result = arenas.read(bad_ova);
    assert!(matches!(result, Err(ArenaError::GenerationMismatch { .. })));
}

#[test]
fn arena_full_out_of_space() {
    let mut arenas = TripleArena::new();
    // Nursery has 256 slots
    for _ in 0..256 {
        arenas.alloc_nursery(1).unwrap();
    }
    let result = arenas.alloc_nursery(1);
    assert!(matches!(result, Err(ArenaError::OutOfSpace)));
}

#[test]
fn nursery_free_rejected() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_nursery(8).unwrap();
    let result = arenas.free(ova);
    assert!(matches!(result, Err(ArenaError::NurseryWriteViolation)));
}

#[test]
fn nursery_survives_sync() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_nursery(4).unwrap();
    arenas.write(ova, &[0xAA, 0xBB, 0xCC, 0xDD]).unwrap();

    arenas.sync();
    arenas.sync();

    let data = arenas.read(ova).unwrap();
    assert_eq!(&data[..4], &[0xAA, 0xBB, 0xCC, 0xDD]);
}

#[test]
fn dynamic_free_and_realloc() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_staging(8).unwrap();
    arenas.write(ova, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();

    arenas.free(ova).unwrap();

    let result = arenas.read(ova);
    assert!(result.is_err());
}

// --- DMOVE tests ---

#[test]
fn dmove_buffer_transfer() {
    let mut arenas = TripleArena::new();
    let dest = arenas.alloc_nursery(16).unwrap();

    let desc = DmoveDescriptor {
        source: DmoveSource::Buffer(vec![10, 20, 30, 40]),
        dest_ova: dest,
        length: 4,
        source_offset: 0,
    };

    let transferred = DmoveEngine::execute(&mut arenas, &[desc]).unwrap();
    assert_eq!(transferred, 4);

    let data = arenas.read(dest).unwrap();
    assert_eq!(&data[..4], &[10, 20, 30, 40]);
}

#[test]
fn dmove_nursery_ref_cross_arena() {
    let mut arenas = TripleArena::new();
    let src = arenas.alloc_nursery(8).unwrap();
    arenas.write(src, &[0xDE, 0xAD, 0xBE, 0xEF, 0, 0, 0, 0]).unwrap();

    let dest = arenas.alloc_staging(8).unwrap();

    let desc = DmoveDescriptor {
        source: DmoveSource::NurseryRef(src),
        dest_ova: dest,
        length: 4,
        source_offset: 0,
    };

    let transferred = DmoveEngine::execute(&mut arenas, &[desc]).unwrap();
    assert_eq!(transferred, 4);

    let data = arenas.read(dest).unwrap();
    assert_eq!(&data[..4], &[0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn dmove_source_too_short() {
    let mut arenas = TripleArena::new();
    let dest = arenas.alloc_nursery(16).unwrap();

    let desc = DmoveDescriptor {
        source: DmoveSource::Buffer(vec![1, 2]),
        dest_ova: dest,
        length: 10, // too long
        source_offset: 0,
    };

    let result = DmoveEngine::execute(&mut arenas, &[desc]);
    assert!(result.is_err());
}
```

---

### 3. TKV Metadata Format

**File:** `tests/tkv.rs` (10 tests)

Tests for the binary TKV (Type, Key-Value) metadata format. Covers all five value types, nested tables, comment stripping, and ordinal map extraction.

```rust
use matterstream::fqa::Fqa;
use matterstream::tkv::{TkvDocument, TkvEntry, TkvError, TkvValue};

#[test]
fn string_roundtrip() {
    let mut doc = TkvDocument::new();
    doc.push("comment", TkvValue::String("hello world".into()));

    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();

    assert_eq!(decoded.entries.len(), 1);
    assert_eq!(decoded.entries[0].key, "comment");
    assert_eq!(decoded.entries[0].value, TkvValue::String("hello world".into()));
}

#[test]
fn fqa_roundtrip() {
    let mut doc = TkvDocument::new();
    let fqa = Fqa::new(0xDEADBEEF_CAFEBABE);
    doc.push("addr", TkvValue::Fqa(fqa));

    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();
    assert_eq!(decoded.entries[0].value, TkvValue::Fqa(fqa));
}

#[test]
fn integer_roundtrip() {
    let mut doc = TkvDocument::new();
    doc.push("count", TkvValue::Integer(42));
    doc.push("max", TkvValue::Integer(u64::MAX));

    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();

    assert_eq!(decoded.entries[0].value, TkvValue::Integer(42));
    assert_eq!(decoded.entries[1].value, TkvValue::Integer(u64::MAX));
}

#[test]
fn boolean_roundtrip() {
    let mut doc = TkvDocument::new();
    doc.push("enabled", TkvValue::Boolean(true));
    doc.push("disabled", TkvValue::Boolean(false));

    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();

    assert_eq!(decoded.entries[0].value, TkvValue::Boolean(true));
    assert_eq!(decoded.entries[1].value, TkvValue::Boolean(false));
}

#[test]
fn nested_table() {
    let inner = vec![
        TkvEntry { key: "x".into(), value: TkvValue::Integer(10) },
        TkvEntry { key: "y".into(), value: TkvValue::Integer(20) },
    ];

    let mut doc = TkvDocument::new();
    doc.push("coords", TkvValue::Table(inner));

    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();

    if let TkvValue::Table(entries) = &decoded.entries[0].value {
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "x");
        assert_eq!(entries[0].value, TkvValue::Integer(10));
    } else {
        panic!("expected Table");
    }
}

#[test]
fn strip_comments_removes_strings_only() {
    let mut doc = TkvDocument::new();
    doc.push("comment", TkvValue::String("a note".into()));
    doc.push("count", TkvValue::Integer(5));
    doc.push("note", TkvValue::String("another note".into()));
    doc.push("flag", TkvValue::Boolean(true));

    doc.strip_comments();

    assert_eq!(doc.entries.len(), 2);
    assert_eq!(doc.entries[0].key, "count");
    assert_eq!(doc.entries[1].key, "flag");
}

#[test]
fn ordinal_map_extraction() {
    let mut doc = TkvDocument::new();
    doc.push("module_a", TkvValue::Fqa(Fqa::new(100)));
    doc.push("comment", TkvValue::String("ignored".into()));
    doc.push("module_b", TkvValue::Fqa(Fqa::new(200)));

    let map = doc.ordinal_map();
    assert_eq!(map.len(), 2);
    assert_eq!(map["module_a"], Fqa::new(100));
    assert_eq!(map["module_b"], Fqa::new(200));
}

#[test]
fn invalid_type_byte_rejected() {
    let data = vec![0xFF, 0x01, 0x00, b'k'];
    let result = TkvDocument::decode(&data);
    assert!(matches!(result, Err(TkvError::InvalidTypeByte(0xFF))));
}

#[test]
fn truncated_data_rejected() {
    let data = vec![0x01]; // String type but no key length
    let result = TkvDocument::decode(&data);
    assert!(matches!(result, Err(TkvError::TruncatedData)));
}

#[test]
fn multi_entry_document() {
    let mut doc = TkvDocument::new();
    doc.push("name", TkvValue::String("test".into()));
    doc.push("version", TkvValue::Integer(1));
    doc.push("main", TkvValue::Fqa(Fqa::new(0x1234)));
    doc.push("debug", TkvValue::Boolean(false));

    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();
    assert_eq!(decoded, doc);
}
```

---

### 4. AR Archive Container

**File:** `tests/archive.rs` (9 tests)

Tests for the standard Unix ar format container with Base62 ordinals and FourCC extensions. Covers serialization roundtrips, manifest lookup, validation rules, and data integrity.

```rust
use matterstream::archive::{ArchiveError, ArchiveMember, MtsmArchive};
use matterstream::fqa::{FourCC, Ordinal};
use matterstream::tkv::{TkvDocument, TkvValue};

fn make_valid_archive() -> MtsmArchive {
    let mut archive = MtsmArchive::new();

    // .meta manifest at 00000000
    let mut manifest = TkvDocument::new();
    manifest.push("name", TkvValue::String("test-archive".into()));
    manifest.push("version", TkvValue::Integer(1));
    archive.add(ArchiveMember::new(
        Ordinal::zero(), FourCC::Meta, manifest.encode(),
    ));

    // .asym table
    archive.add(ArchiveMember::new(
        Ordinal::new("00000001").unwrap(), FourCC::Asym, vec![0u8; 8],
    ));

    // .mrbc bincode
    archive.add(ArchiveMember::new(
        Ordinal::new("00000002").unwrap(), FourCC::Mrbc, vec![0x00, 0x0F],
    ));

    archive
}

#[test]
fn archive_roundtrip() {
    let archive = make_valid_archive();
    let bytes = archive.to_ar_bytes();
    let restored = MtsmArchive::from_ar_bytes(&bytes).unwrap();

    assert_eq!(restored.members.len(), 3);
    for (orig, rest) in archive.members.iter().zip(restored.members.iter()) {
        assert_eq!(orig.ordinal, rest.ordinal);
        assert_eq!(orig.fourcc, rest.fourcc);
        assert_eq!(orig.data, rest.data);
    }
}

#[test]
fn manifest_found_at_00000000() {
    let archive = make_valid_archive();
    let manifest = archive.manifest().unwrap();
    assert!(manifest.entries.iter().any(|e| e.key == "name"));
}

#[test]
fn validate_passes_well_formed() {
    let archive = make_valid_archive();
    assert!(archive.validate().is_ok());
}

#[test]
fn validate_fails_missing_meta() {
    let mut archive = MtsmArchive::new();
    archive.add(ArchiveMember::new(
        Ordinal::new("00000001").unwrap(), FourCC::Asym, vec![0u8; 8],
    ));
    assert!(matches!(archive.validate(), Err(ArchiveError::MissingMeta)));
}

#[test]
fn validate_fails_missing_asym() {
    let mut archive = MtsmArchive::new();
    archive.add(ArchiveMember::new(
        Ordinal::zero(), FourCC::Meta, TkvDocument::new().encode(),
    ));
    assert!(matches!(archive.validate(), Err(ArchiveError::MissingAsym)));
}

#[test]
fn member_data_integrity() {
    let payload = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE];
    let mut archive = MtsmArchive::new();
    archive.add(ArchiveMember::new(Ordinal::zero(), FourCC::Meta, TkvDocument::new().encode()));
    archive.add(ArchiveMember::new(Ordinal::new("00000001").unwrap(), FourCC::Asym, vec![0u8; 8]));
    archive.add(ArchiveMember::new(Ordinal::new("00000002").unwrap(), FourCC::Mrbc, payload.clone()));

    let bytes = archive.to_ar_bytes();
    let restored = MtsmArchive::from_ar_bytes(&bytes).unwrap();

    let mrbc = restored.bincode_members();
    assert_eq!(mrbc[0].data, payload);
}

#[test]
fn asym_member_found() {
    let archive = make_valid_archive();
    let asym = archive.asym().unwrap();
    assert_eq!(asym.fourcc, FourCC::Asym);
}

#[test]
fn invalid_magic_rejected() {
    let result = MtsmArchive::from_ar_bytes(b"not_ar!!");
    assert!(matches!(result, Err(ArchiveError::InvalidMagic)));
}

#[test]
fn bincode_members_filter() {
    let archive = make_valid_archive();
    let bincodes = archive.bincode_members();
    assert_eq!(bincodes.len(), 1);
    assert_eq!(bincodes[0].fourcc, FourCC::Mrbc);
}
```

---

### 5. SCL Entropy Guard & Keyless

**File:** `tests/scl.rs` (14 tests)

Tests for the Secure Code Loader (modified LZW entropy guard with Shannon entropy validation) and the Keyless invariant enforcement (entropy classification preventing secret-class data storage).

```rust
use matterstream::keyless::{EntropyClass, KeylessError, KeylessPolicy};
use matterstream::scl::{shannon_entropy, Scl, SclConfig, SclVerdict};

// --- Shannon entropy tests ---

#[test]
fn shannon_entropy_empty() {
    assert_eq!(shannon_entropy(&[]), 0.0);
}

#[test]
fn shannon_entropy_single_value() {
    let data = vec![0xAA; 100];
    assert_eq!(shannon_entropy(&data), 0.0);
}

#[test]
fn shannon_entropy_two_values() {
    // Equal distribution of 2 values: entropy = 1.0 bit
    let mut data = vec![0u8; 100];
    for i in 0..50 { data[i] = 1; }
    let e = shannon_entropy(&data);
    assert!((e - 1.0).abs() < 0.01);
}

#[test]
fn shannon_entropy_uniform_random() {
    // All 256 byte values equally: entropy ~8.0
    let mut data = Vec::new();
    for _ in 0..4 {
        for i in 0u16..=255 { data.push(i as u8); }
    }
    let e = shannon_entropy(&data);
    assert!((e - 8.0).abs() < 0.01);
}

// --- SCL validation tests ---

#[test]
fn mtsm_opcodes_pass_validation() {
    let scl = Scl::default();
    let data: Vec<u8> = (0..100).map(|i| [0x00, 0x01, 0x0F, 0x07][i % 4]).collect();
    assert_eq!(scl.validate(&data), SclVerdict::Accept);
}

#[test]
fn tkv_metadata_passes() {
    let scl = Scl::default();
    let mut data = Vec::new();
    for _ in 0..50 {
        data.extend_from_slice(&[0x01, 0x03, 0x00, b'k', 0x04, 0x00, 0x00, 0x00,
                                  b't', b'e', b's', b't']);
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
    let scl = Scl::new(SclConfig { entropy_threshold: 0.85, ..SclConfig::default() });
    let mut data = vec![0u8; 256];
    let mut state: u64 = 0xDEADBEEFCAFEBABE;
    for byte in &mut data {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *byte = (state >> 33) as u8;
    }
    let verdict = scl.validate(&data);
    assert_ne!(verdict, SclVerdict::Accept);
}

#[test]
fn validate_archive_scans_all() {
    let scl = Scl::default();
    let good = vec![0x00u8; 50];
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
    let data = vec![0u8; 100];
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
    let mut data = vec![0u8; 1024];
    let mut state: u64 = 0x123456789ABCDEF0;
    for byte in &mut data {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *byte = (state >> 33) as u8;
    }
    assert!(matches!(policy.assert_storable(&data), Err(KeylessError::SecretDataRejected)));
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
    let mut data = vec![0u8; 256];
    let mut state: u64 = 0xFEDCBA9876543210;
    for byte in &mut data {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *byte = (state >> 33) as u8;
    }
    let class = policy.assert_transient(&data);
    assert!(matches!(class, EntropyClass::Structured
                          | EntropyClass::Compressed
                          | EntropyClass::Secret));
}
```

---

### 6. RPN Stack Language VM

**File:** `tests/rpn.rs` (38 tests)

Tests for the RPN bytecode VM including all 25 opcodes, gas metering, backward-jump loop detection, control flow (Jmp/JmpIf/Halt), comparison operators, and a full Fibonacci computation using arena memory.

#### Opcode Reference

| Opcode | Hex  | Payload  | Gas | Description |
|--------|------|----------|-----|-------------|
| Nop    | 0x00 | --       | 1   | No operation |
| Push32 | 0x01 | 4 bytes  | 1   | Push u32 literal |
| Push64 | 0x02 | 8 bytes  | 1   | Push u64 literal |
| PushFqa| 0x03 | 16 bytes | 1   | Push FQA (u128) |
| Dup    | 0x04 | --       | 1   | Duplicate top of stack |
| Drop   | 0x05 | --       | 1   | Pop and discard top |
| Swap   | 0x06 | --       | 1   | Swap top two values |
| Add    | 0x07 | --       | 2   | a + b (wrapping) |
| Sub    | 0x08 | --       | 2   | a - b (wrapping) |
| Mul    | 0x09 | --       | 2   | a * b (wrapping) |
| Div    | 0x0A | --       | 2   | a / b |
| Load   | 0x0B | --       | 10  | Read u32 from arena at OVA |
| Store  | 0x0C | --       | 10  | Write u32 to arena at OVA |
| Call   | 0x0D | --       | 5   | Push return addr, jump to target |
| Ret    | 0x0E | --       | 5   | Pop return addr, jump back |
| Sync   | 0x0F | --       | 100 | Swap active/staging arenas |
| MapNew | 0x10 | --       | 5   | Push empty HashMap |
| MapSet | 0x11 | --       | 5   | Insert key/value into map |
| MapGet | 0x12 | --       | 5   | Lookup key in map |
| Jmp    | 0x13 | 8 bytes  | 2   | Unconditional jump |
| JmpIf  | 0x14 | 8 bytes  | 2   | Jump if top-of-stack != 0 |
| Halt   | 0x15 | --       | 1   | Stop execution |
| Mod    | 0x16 | --       | 2   | a % b |
| CmpEq  | 0x17 | --       | 2   | Push 1 if a == b, else 0 |
| CmpLt  | 0x18 | --       | 2   | Push 1 if a < b, else 0 |
| CmpGt  | 0x19 | --       | 2   | Push 1 if a > b, else 0 |

#### Test Code

```rust
use matterstream::arena::TripleArena;
use matterstream::ova::ArenaId;
use matterstream::rpn::{GasConfig, RpnError, RpnOp, RpnVm};

fn encode_push32(val: u32) -> Vec<u8> {
    let mut buf = vec![RpnOp::Push32 as u8];
    buf.extend_from_slice(&val.to_le_bytes());
    buf
}

fn encode_push64(val: u64) -> Vec<u8> {
    let mut buf = vec![RpnOp::Push64 as u8];
    buf.extend_from_slice(&val.to_le_bytes());
    buf
}

fn encode_jmp(target: u64) -> Vec<u8> {
    let mut buf = vec![RpnOp::Jmp as u8];
    buf.extend_from_slice(&target.to_le_bytes());
    buf
}

fn encode_jmpif(target: u64) -> Vec<u8> {
    let mut buf = vec![RpnOp::JmpIf as u8];
    buf.extend_from_slice(&target.to_le_bytes());
    buf
}

// --- Core opcode tests ---

#[test]
fn push_pop_roundtrip() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push32(42);
    bc.push(RpnOp::Drop as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert!(vm.stack.is_empty());
}

#[test]
fn arithmetic_add() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(3);
    bc.extend_from_slice(&encode_push64(4));
    bc.push(RpnOp::Add as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 7);
}

#[test]
fn arithmetic_sub() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(10);
    bc.extend_from_slice(&encode_push64(3));
    bc.push(RpnOp::Sub as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 7);
}

#[test]
fn arithmetic_mul() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(6);
    bc.extend_from_slice(&encode_push64(7));
    bc.push(RpnOp::Mul as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 42);
}

#[test]
fn division_by_zero_error() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(10);
    bc.extend_from_slice(&encode_push64(0));
    bc.push(RpnOp::Div as u8);
    assert!(matches!(vm.execute(&bc, &mut arenas), Err(RpnError::DivisionByZero)));
}

#[test]
fn stack_underflow_error() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let bc = vec![RpnOp::Add as u8];
    assert!(matches!(vm.execute(&bc, &mut arenas), Err(RpnError::StackUnderflow)));
}

#[test]
fn stack_overflow_error() {
    let mut vm = RpnVm::new();
    vm.max_stack_depth = 2;
    let mut arenas = TripleArena::new();
    let mut bc = encode_push32(1);
    bc.extend_from_slice(&encode_push32(2));
    bc.extend_from_slice(&encode_push32(3));
    assert!(matches!(vm.execute(&bc, &mut arenas), Err(RpnError::StackOverflow)));
}

#[test]
fn sync_swaps_arenas() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    assert_eq!(arenas.active_arena(), ArenaId::DynamicA);
    let bc = vec![RpnOp::Sync as u8];
    vm.execute(&bc, &mut arenas).unwrap();
    assert!(vm.synced);
    assert_eq!(arenas.active_arena(), ArenaId::DynamicB);
}

#[test]
fn load_store_via_ova() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_nursery(16).unwrap();

    // Store 0x42 then load it back
    let mut bc = encode_push64(0x42);
    let mut ova_push = vec![RpnOp::Push32 as u8];
    ova_push.extend_from_slice(&ova.0.to_le_bytes());
    bc.extend_from_slice(&ova_push);
    bc.push(RpnOp::Store as u8);
    bc.extend_from_slice(&ova_push);
    bc.push(RpnOp::Load as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u32().unwrap(), 0x42);
}

#[test]
fn map_new_set_get() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = vec![RpnOp::MapNew as u8];
    bc.extend_from_slice(&encode_push64(1));  // key
    bc.extend_from_slice(&encode_push64(99)); // value
    bc.push(RpnOp::MapSet as u8);
    bc.extend_from_slice(&encode_push64(1));  // key
    bc.push(RpnOp::MapGet as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 99);
}

#[test]
fn invalid_opcode_rejected() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let bc = vec![0xFF];
    assert!(matches!(vm.execute(&bc, &mut arenas), Err(RpnError::InvalidOpcode(0xFF))));
}

// --- Gas metering tests ---

#[test]
fn gas_metering_tracks_consumption() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let bc = vec![RpnOp::Nop as u8; 3];
    let trace = vm.execute_metered(&bc, &mut arenas).unwrap();
    assert_eq!(trace.opcodes_executed, 3);
    assert_eq!(trace.gas_consumed, 3); // 3 * cost_nop(1)
}

#[test]
fn gas_exhaustion_error() {
    let mut vm = RpnVm::with_gas(5);
    let mut arenas = TripleArena::new();
    // Push64(1) + Push64(2) + Add + Push64(3) + Push64(4) + Add = 1+1+2+1+1+2 = 8 > 5
    let mut bc = encode_push64(1);
    bc.extend_from_slice(&encode_push64(2));
    bc.push(RpnOp::Add as u8);
    bc.extend_from_slice(&encode_push64(3));
    bc.extend_from_slice(&encode_push64(4));
    bc.push(RpnOp::Add as u8);
    assert!(matches!(vm.execute(&bc, &mut arenas), Err(RpnError::GasExhausted { .. })));
}

#[test]
fn gas_sync_costs_more() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let bc = vec![RpnOp::Sync as u8];
    let trace = vm.execute_metered(&bc, &mut arenas).unwrap();
    assert_eq!(trace.gas_consumed, 100); // cost_sync default
    assert_eq!(trace.syncs, 1);
}

#[test]
fn gas_custom_config() {
    let mut config = GasConfig::new(50);
    config.cost_arithmetic = 10;
    let mut vm = RpnVm::with_gas_config(config);
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(1);
    bc.extend_from_slice(&encode_push64(2));
    bc.push(RpnOp::Add as u8);
    let trace = vm.execute_metered(&bc, &mut arenas).unwrap();
    assert_eq!(trace.gas_consumed, 12); // 1 + 1 + 10
}

// --- Loop detection tests ---

#[test]
fn backward_jump_detected() {
    let mut vm = RpnVm::new();
    vm.gas.max_backward_jumps = 5;
    let mut arenas = TripleArena::new();
    let bc = encode_jmp(0); // infinite loop to offset 0
    assert!(matches!(
        vm.execute(&bc, &mut arenas),
        Err(RpnError::BackwardJumpLimitExceeded { count: 6, limit: 5 })
    ));
}

#[test]
fn forward_jump_not_limited() {
    let mut vm = RpnVm::new();
    vm.gas.max_backward_jumps = 1;
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(1);
    let end = bc.len() + 9;
    bc.extend_from_slice(&encode_jmp(end as u64));
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.trace.forward_jumps, 1);
    assert_eq!(vm.trace.backward_jumps, 0);
}

#[test]
fn counted_loop_with_jmpif() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    // counter=5, loop: counter-=1, if counter>0 goto loop
    let mut bc = encode_push64(5);
    let loop_start = bc.len();
    bc.extend_from_slice(&encode_push64(1));
    bc.push(RpnOp::Sub as u8);
    bc.push(RpnOp::Dup as u8);
    bc.extend_from_slice(&encode_push64(0));
    bc.push(RpnOp::CmpGt as u8);
    bc.extend_from_slice(&encode_jmpif(loop_start as u64));
    bc.push(RpnOp::Halt as u8);

    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 0);
    assert_eq!(vm.trace.backward_jumps, 4);
    assert!(vm.trace.halted);
}

// --- Control flow & comparison tests ---

#[test]
fn halt_stops_execution() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(42);
    bc.push(RpnOp::Halt as u8);
    bc.extend_from_slice(&encode_push64(99)); // not executed
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u64().unwrap(), 42);
    assert!(vm.trace.halted);
}

#[test]
fn jmp_unconditional() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let jmp_target = 9 + 9; // skip Push64(99)
    let mut bc = encode_jmp(jmp_target as u64);
    bc.extend_from_slice(&encode_push64(99));  // skipped
    bc.extend_from_slice(&encode_push64(42));  // target
    bc.push(RpnOp::Halt as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 42);
}

#[test]
fn jmpif_conditional_true() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let jmpif_end = 9 + 9 + 9;
    let mut bc = encode_push64(1); // truthy
    bc.extend_from_slice(&encode_jmpif(jmpif_end as u64));
    bc.extend_from_slice(&encode_push64(99)); // skipped
    bc.extend_from_slice(&encode_push64(42)); // target
    bc.push(RpnOp::Halt as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 42);
}

#[test]
fn jmpif_conditional_false() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(0); // falsy
    bc.extend_from_slice(&encode_jmpif(100));
    bc.extend_from_slice(&encode_push64(99)); // executed
    bc.push(RpnOp::Halt as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 99);
}

#[test]
fn mod_operation() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(17);
    bc.extend_from_slice(&encode_push64(5));
    bc.push(RpnOp::Mod as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 2); // 17 % 5 = 2
}

#[test]
fn cmp_eq_true() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(42);
    bc.extend_from_slice(&encode_push64(42));
    bc.push(RpnOp::CmpEq as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 1);
}

#[test]
fn cmp_lt() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(3);
    bc.extend_from_slice(&encode_push64(5));
    bc.push(RpnOp::CmpLt as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 1); // 3 < 5
}

#[test]
fn cmp_gt() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(10);
    bc.extend_from_slice(&encode_push64(5));
    bc.push(RpnOp::CmpGt as u8);
    vm.execute(&bc, &mut arenas).unwrap();
    assert_eq!(vm.stack[0].as_u64().unwrap(), 1); // 10 > 5
}

#[test]
fn invalid_jump_target_error() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let bc = encode_jmp(9999);
    assert!(matches!(vm.execute(&bc, &mut arenas), Err(RpnError::InvalidJumpTarget(9999))));
}

#[test]
fn trace_max_stack_depth() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    let mut bc = encode_push64(1);
    bc.extend_from_slice(&encode_push64(2));
    bc.extend_from_slice(&encode_push64(3));
    bc.push(RpnOp::Drop as u8);
    bc.push(RpnOp::Drop as u8);
    let trace = vm.execute_metered(&bc, &mut arenas).unwrap();
    assert_eq!(trace.max_stack_depth_seen, 3);
    assert_eq!(vm.stack.len(), 1);
}

#[test]
fn disassemble_output() {
    let mut bc = encode_push64(42);
    bc.extend_from_slice(&encode_push32(10));
    bc.push(RpnOp::Add as u8);
    let jmp_target = bc.len() + 9;
    bc.extend_from_slice(&encode_jmp(jmp_target as u64));
    bc.push(RpnOp::Halt as u8);
    let disasm = RpnVm::disassemble(&bc).unwrap();
    assert!(disasm.contains("Push64 42"));
    assert!(disasm.contains("Push32 10"));
    assert!(disasm.contains("Jmp"));
    assert!(disasm.contains("Halt"));
}

// --- Fibonacci loop (full program using arena memory) ---

#[test]
fn fibonacci_loop() {
    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();

    let ova_n = arenas.alloc_nursery(4).unwrap();
    let ova_a = arenas.alloc_nursery(4).unwrap();
    let ova_b = arenas.alloc_nursery(4).unwrap();

    let mut bc = Vec::new();

    // n=10, a=0, b=1
    bc.extend_from_slice(&encode_push64(10));
    bc.extend_from_slice(&encode_push32(ova_n.0));
    bc.push(RpnOp::Store as u8);
    bc.extend_from_slice(&encode_push64(0));
    bc.extend_from_slice(&encode_push32(ova_a.0));
    bc.push(RpnOp::Store as u8);
    bc.extend_from_slice(&encode_push64(1));
    bc.extend_from_slice(&encode_push32(ova_b.0));
    bc.push(RpnOp::Store as u8);

    let loop_start = bc.len();

    // if n > 0
    bc.extend_from_slice(&encode_push32(ova_n.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push64(0));
    bc.push(RpnOp::CmpGt as u8);

    let jmpif_pos = bc.len();
    bc.extend_from_slice(&encode_jmpif(0)); // -> body
    let jmp_end_pos = bc.len();
    bc.extend_from_slice(&encode_jmp(0));   // -> end

    let body_start = bc.len();

    // next = a + b
    bc.extend_from_slice(&encode_push32(ova_a.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push32(ova_b.0));
    bc.push(RpnOp::Load as u8);
    bc.push(RpnOp::Add as u8);

    // a = b
    bc.extend_from_slice(&encode_push32(ova_b.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push32(ova_a.0));
    bc.push(RpnOp::Store as u8);

    // b = next
    bc.extend_from_slice(&encode_push32(ova_b.0));
    bc.push(RpnOp::Store as u8);

    // n -= 1
    bc.extend_from_slice(&encode_push32(ova_n.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push64(1));
    bc.push(RpnOp::Sub as u8);
    bc.extend_from_slice(&encode_push32(ova_n.0));
    bc.push(RpnOp::Store as u8);

    bc.extend_from_slice(&encode_jmp(loop_start as u64));

    let loop_end = bc.len();

    // result = a (fib(n) after n iterations)
    bc.extend_from_slice(&encode_push32(ova_a.0));
    bc.push(RpnOp::Load as u8);
    bc.push(RpnOp::Halt as u8);

    // Patch jump targets
    bc[jmpif_pos + 1..jmpif_pos + 9].copy_from_slice(&(body_start as u64).to_le_bytes());
    bc[jmp_end_pos + 1..jmp_end_pos + 9].copy_from_slice(&(loop_end as u64).to_le_bytes());

    vm.execute(&bc, &mut arenas).unwrap();

    assert_eq!(vm.stack[0].as_u64().unwrap(), 55); // fib(10) = 55
    assert_eq!(vm.trace.backward_jumps, 10);        // 10 loop iterations
}
```

---

### 7. Integration Tests

**File:** `tests/integration.rs` (8 tests)

End-to-end tests verifying the full MatterStream pipeline: existing UI ISA regression tests, new VM_SPEC subsystem integration, mixed old+new ops, and a complete archive-to-RPN pipeline.

```rust
use matterstream::addressing::AddressResolver;
use matterstream::archive::{ArchiveMember, MtsmArchive};
use matterstream::arena::TripleArena;
use matterstream::aslr::{AslrToken, AsymTable};
use matterstream::builder::StreamBuilder;
use matterstream::fqa::{Fqa, FourCC, Ordinal};
use matterstream::keyless::KeylessPolicy;
use matterstream::ops::{Op, OpsHeader, Primitive, RsiPointer};
use matterstream::rpn::{RpnOp, RpnVm};
use matterstream::scl::{Scl, SclVerdict};
use matterstream::stream::MatterStream;
use matterstream::tkv::{TkvDocument, TkvValue};

// --- Regression: existing architecture tests pass ---

#[test]
fn regression_test_a_direct_register_access() {
    smol::block_on(async {
        let mut ms = MatterStream::new();
        let rsi = RsiPointer::new(1, 2, 0);
        let header = OpsHeader::new(vec![rsi], true);
        ms.registers.vec3.write(0, [1.0, 2.0, 3.0]);
        let ops = vec![Op::Draw { primitive: Primitive::Slab, position_rsi: 0 }];
        ms.execute(&header, &ops).await.unwrap();
        assert_eq!(ms.draws[0].position, [1.0, 2.0, 3.0]);
    });
}

#[test]
fn regression_test_d_translation_fast_path() {
    smol::block_on(async {
        let mut ms = MatterStream::new();
        let rsi = RsiPointer::new(1, 2, 0);
        let header = OpsHeader::new(vec![rsi], true);
        let ops = vec![
            Op::SetTrans([5.0, 10.0, 15.0]),
            Op::Draw { primitive: Primitive::Slab, position_rsi: 0 },
        ];
        ms.execute(&header, &ops).await.unwrap();
        assert!(ms.draws[0].used_fast_path);
        assert_eq!(ms.draws[0].transform_bytes, 12);
    });
}

// --- New VM_SPEC v0.1.0 integration tests ---

#[test]
fn matterstream_with_new_fields() {
    let ms = MatterStream::new();
    assert_eq!(ms.arenas.active_arena(), matterstream::ArenaId::DynamicA);
    assert_eq!(ms.rpn_vm.stack.len(), 0);
}

#[test]
fn op_sync_swaps_arenas() {
    smol::block_on(async {
        let mut ms = MatterStream::new();
        let header = OpsHeader::new(vec![], false);
        ms.execute(&header, &[Op::Sync]).await.unwrap();
        assert_eq!(ms.arenas.active_arena(), matterstream::ArenaId::DynamicB);
    });
}

#[test]
fn op_exec_rpn_runs_bytecode() {
    smol::block_on(async {
        let mut ms = MatterStream::new();
        let header = OpsHeader::new(vec![], false);
        let mut bytecode = vec![RpnOp::Push64 as u8];
        bytecode.extend_from_slice(&3u64.to_le_bytes());
        bytecode.push(RpnOp::Push64 as u8);
        bytecode.extend_from_slice(&4u64.to_le_bytes());
        bytecode.push(RpnOp::Add as u8);
        ms.execute(&header, &[Op::ExecRpn(bytecode)]).await.unwrap();
        assert_eq!(ms.rpn_vm.stack[0].as_u64().unwrap(), 7);
    });
}

#[test]
fn mixed_old_and_new_ops() {
    smol::block_on(async {
        let mut ms = MatterStream::new();
        let rsi = RsiPointer::new(1, 2, 0);
        let header = OpsHeader::new(vec![rsi], true);
        let ops = vec![
            Op::SetTrans([1.0, 2.0, 3.0]),
            Op::Draw { primitive: Primitive::Slab, position_rsi: 0 },
            Op::Sync,
            Op::Push(vec![0xAA, 0xBB]),
        ];
        ms.execute(&header, &ops).await.unwrap();
        assert_eq!(ms.draws.len(), 1);
        assert_eq!(ms.stream, vec![0xAA, 0xBB]);
    });
}

#[test]
fn full_pipeline_archive_scl_arena_rpn() {
    // 1. Build archive with .meta, .asym, .mrbc
    let mut manifest = TkvDocument::new();
    manifest.push("name", TkvValue::String("pipeline-test".into()));
    manifest.push("main", TkvValue::Fqa(Fqa::new(0x1000)));

    let mut asym_table = AsymTable::new();
    let fqa = Fqa::new(0x1000);
    let token = AslrToken(0xBEEF);
    asym_table.insert(token, matterstream::Ova::new(matterstream::ArenaId::Nursery, 0, 0, 0));

    let push32_bytes = 42u32.to_le_bytes();
    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&push32_bytes)),
        (RpnOp::Sync, None),
    ]);

    let mut archive = MtsmArchive::new();
    archive.add(ArchiveMember::new(Ordinal::zero(), FourCC::Meta, manifest.encode()));
    archive.add(ArchiveMember::new(Ordinal::new("00000001").unwrap(), FourCC::Asym, asym_table.to_bytes()));
    archive.add(ArchiveMember::new(Ordinal::new("00000002").unwrap(), FourCC::Mrbc, bytecode.clone()));

    // 2. Validate archive
    archive.validate().unwrap();

    // 3. SCL validates all members
    let scl = Scl::default();
    for member in &archive.members {
        assert_eq!(scl.load_member(&member.data), SclVerdict::Accept);
    }

    // 4. Load into arena
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_nursery(bytecode.len()).unwrap();
    arenas.write(ova, &bytecode).unwrap();

    // 5. Execute RPN
    let mut rpn = RpnVm::new();
    rpn.execute(&bytecode, &mut arenas).unwrap();
    assert_eq!(rpn.stack[0].as_u32().unwrap(), 42);
    assert!(rpn.synced);

    // 6. Keyless enforcement
    let keyless = KeylessPolicy::new();
    keyless.assert_storable(&bytecode).unwrap();

    // 7. Address resolution
    let mut resolver = AddressResolver::new();
    resolver.register(fqa, token, ova);
    let resolved_ova = resolver.resolve(fqa).unwrap();
    assert_eq!(resolved_ova.arena(), matterstream::ArenaId::Nursery);
}

#[test]
fn builder_with_new_ops() {
    let fqa = Fqa::new(42);
    let ops = StreamBuilder::new()
        .resolve_fqa(fqa)
        .sync()
        .exec_rpn(vec![RpnOp::Nop as u8])
        .build();
    assert_eq!(ops.len(), 3);
    assert!(matches!(ops[0], Op::ResolveFqa(_)));
    assert!(matches!(ops[1], Op::Sync));
    assert!(matches!(ops[2], Op::ExecRpn(_)));
}
```

---

## Runnable Examples

### rpn-arena -- Fibonacci Bar Chart

**File:** `examples/rpn-arena.rs`

Computes fib(1) through fib(15) using the RPN VM with arena memory, prints gas/opcode traces to stdout, then renders a color-gradient bar chart in a window using winit + softbuffer.

```sh
cargo run --example rpn-arena
cargo run --example rpn-arena -- --timeout 5   # auto-close after 5 seconds
```

**What it demonstrates:**
- RPN bytecode construction with loop (Jmp/JmpIf/CmpGt)
- Arena nursery allocation for loop variables (ova_n, ova_a, ova_b)
- Gas metering via `execute_metered()` returning `ExecTrace`
- Bytecode disassembly via `RpnVm::disassemble()`
- Window rendering of computed results

**Console output:**

```
fib( 1) =      1  [gas:    56, opcodes:   40, backward_jumps:   1]
fib( 2) =      1  [gas:   102, opcodes:   72, backward_jumps:   2]
fib( 3) =      2  [gas:   148, opcodes:  104, backward_jumps:   3]
...
fib(15) =    610  [gas:   700, opcodes:  488, backward_jumps:  15]

Disassembly of fib(10):
0000: Push64 10
0009: Store
000a: Push64 0
...
```

**Source:**

```rust
use matterstream::arena::TripleArena;
use matterstream::rpn::{RpnOp, RpnVm};

/// Build RPN bytecode that computes fib(n) using arena memory.
fn build_fib_bytecode(arenas: &mut TripleArena, n: u32) -> (Vec<u8>, matterstream::Ova) {
    let ova_n = arenas.alloc_nursery(4).unwrap();
    let ova_a = arenas.alloc_nursery(4).unwrap();
    let ova_b = arenas.alloc_nursery(4).unwrap();

    let mut bc = Vec::new();

    // Store n, a=0, b=1
    bc.extend_from_slice(&encode_push64(n as u64));
    bc.extend_from_slice(&encode_push32(ova_n.0));
    bc.push(RpnOp::Store as u8);
    // ... (a=0, b=1 similarly)

    let loop_start = bc.len();

    // Load n, check > 0
    bc.extend_from_slice(&encode_push32(ova_n.0));
    bc.push(RpnOp::Load as u8);
    bc.extend_from_slice(&encode_push64(0));
    bc.push(RpnOp::CmpGt as u8);

    // JmpIf -> body, Jmp -> end (placeholders patched later)
    let jmpif_pos = bc.len();
    bc.extend_from_slice(&encode_jmpif(0));
    let jmp_end_pos = bc.len();
    bc.extend_from_slice(&encode_jmp(0));

    let body_start = bc.len();

    // next = a + b; a = b; b = next; n -= 1
    // ... (Load, Add, Store operations)

    bc.extend_from_slice(&encode_jmp(loop_start as u64)); // backward jump
    let loop_end = bc.len();

    // Load result, Halt
    bc.extend_from_slice(&encode_push32(ova_b.0));
    bc.push(RpnOp::Load as u8);
    bc.push(RpnOp::Halt as u8);

    // Patch jump targets
    bc[jmpif_pos + 1..jmpif_pos + 9].copy_from_slice(&(body_start as u64).to_le_bytes());
    bc[jmp_end_pos + 1..jmp_end_pos + 9].copy_from_slice(&(loop_end as u64).to_le_bytes());

    (bc, ova_b)
}

fn main() {
    // Compute fib(1)..fib(15)
    for n in 1..=15 {
        let mut arenas = TripleArena::new();
        let (bytecode, _) = build_fib_bytecode(&mut arenas, n);
        let mut vm = RpnVm::new();
        let trace = vm.execute_metered(&bytecode, &mut arenas).unwrap();
        let result = vm.stack.last().and_then(|v| v.as_u64()).unwrap_or(0);
        println!("fib({:2}) = {:6}  [gas: {:5}]", n, result, trace.gas_consumed);
    }
    // ... then render bar chart with winit + softbuffer
}
```

---

### vm-pipeline -- Full Pipeline Demo

**File:** `examples/vm-pipeline.rs`

Demonstrates the complete VM_SPEC v0.1.0 pipeline: builds an AR archive with TKV manifest, ASYM table, and RPN bytecode members; validates all members with SCL; executes bytecode in fresh arenas; renders a dual bar chart (results + gas consumption).

```sh
cargo run --example vm-pipeline
cargo run --example vm-pipeline -- --timeout 5
```

**Pipeline stages:**

1. **Build AR Archive** -- TKV manifest (`.meta`), ASYM table (`.asym`), 20 RPN bytecode programs (`.mrbc`)
2. **SCL Validation** -- Entropy guard scans all members
3. **RPN Execution** -- Each bytecode computes sum(1..=n) using a loop, verified against n*(n+1)/2
4. **Render** -- Dual bar chart: green = sum results, orange = gas consumption

**Console output:**

```
=== Step 1: Building AR Archive ===
  Archive: 22 members, 3842 bytes serialized
  Manifest name: String("pipeline-demo")
  Bincode members: 20

=== Step 2: SCL Entropy Validation ===
  Accepted: 22, Rejected: 0

=== Step 3: RPN Execution (sum 1..=n) ===
  sum(1..= 1) =    1  [gas:    42]
  sum(1..= 2) =    3  [gas:    74]
  ...
  sum(1..=20) =  210  [gas:   650]

=== Step 4: Rendering ===
```

**Key source excerpts:**

```rust
use matterstream::archive::{ArchiveMember, MtsmArchive};
use matterstream::fqa::{FourCC, Ordinal};
use matterstream::scl::{Scl, SclConfig, SclVerdict};
use matterstream::tkv::{TkvDocument, TkvValue};

// Step 1: Build archive
let mut archive = MtsmArchive::new();
let mut manifest = TkvDocument::new();
manifest.push("name", TkvValue::String("pipeline-demo".into()));
manifest.push("version", TkvValue::Integer(1));
archive.add(ArchiveMember::new(Ordinal::zero(), FourCC::Meta, manifest.encode()));
// ... add .asym and .mrbc members

archive.validate().expect("archive validation failed");
let ar_bytes = archive.to_ar_bytes();

// Step 2: SCL validation
let scl = Scl::new(SclConfig::default());
for member in &archive.members {
    assert_eq!(scl.validate(&member.data), SclVerdict::Accept);
}

// Step 3: Execute
for (n, bc) in &bytecodes {
    let mut arenas = TripleArena::new();
    let mut vm = RpnVm::new();
    let trace = vm.execute_metered(bc, &mut arenas).unwrap();
    let result = vm.stack.last().and_then(|v| v.as_u64()).unwrap_or(0);
    assert_eq!(result, (*n as u64) * (*n as u64 + 1) / 2);
}
```

---

### run-tsx -- TSX Compiler + Renderer

**File:** `examples/run-tsx.rs`

The original MatterStream example. Compiles a TSX file into Ops, executes them against the MatterStream executor, and renders colored 10x10 squares at draw positions using winit + softbuffer.

```sh
cargo run --example run-tsx -- examples/example.tsx
cargo run --example run-tsx -- --timeout 5 examples/login_form.tsx
```

**TSX files:**
- `examples/example.tsx` -- Three colored Slabs (red, green, blue)
- `examples/login_form.tsx` -- Login form UI with title bar, inputs, button
