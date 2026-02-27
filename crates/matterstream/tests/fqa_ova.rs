//! Tests for FQA, Ordinal, OVA, ASLR, and AddressResolver.

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
    let variants = [FourCC::Meta, FourCC::Caps, FourCC::Mrbc, FourCC::Tsxd, FourCC::Asym, FourCC::Symb];
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
