//! Tests for TKV (Type, Key-Value) binary metadata format.

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
    assert_eq!(
        decoded.entries[0].value,
        TkvValue::String("hello world".into())
    );
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
        TkvEntry {
            key: "x".into(),
            value: TkvValue::Integer(10),
        },
        TkvEntry {
            key: "y".into(),
            value: TkvValue::Integer(20),
        },
    ];

    let mut doc = TkvDocument::new();
    doc.push("coords", TkvValue::Table(inner));

    let encoded = doc.encode();
    let decoded = TkvDocument::decode(&encoded).unwrap();

    if let TkvValue::Table(entries) = &decoded.entries[0].value {
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "x");
        assert_eq!(entries[0].value, TkvValue::Integer(10));
        assert_eq!(entries[1].key, "y");
        assert_eq!(entries[1].value, TkvValue::Integer(20));
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
    let data = vec![0xFF, 0x01, 0x00, b'k']; // invalid type byte 0xFF
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
