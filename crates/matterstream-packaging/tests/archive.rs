//! Tests for AR archive container.

use matterstream_packaging::archive::{ArchiveError, ArchiveMember, MtsmArchive};
use matterstream_vm_addressing::fqa::{FourCC, Ordinal, Fqa};
use matterstream_vm_addressing::oid::{Oid, ImportKind};
use matterstream_vm_addressing::oid_index::OidIndexBuilder;
use matterstream_packaging::tkv::{TkvDocument, TkvValue};

fn make_valid_archive() -> MtsmArchive {
    let mut archive = MtsmArchive::new();

    // .meta manifest at 00000000
    let mut manifest = TkvDocument::new();
    manifest.push("name", TkvValue::String("test-archive".into()));
    manifest.push("version", TkvValue::Integer(1));
    archive.add(ArchiveMember::new(
        Ordinal::zero(),
        FourCC::Meta,
        manifest.encode(),
    ));

    // .asym table
    archive.add(ArchiveMember::new(
        Ordinal::new("00000001").unwrap(),
        FourCC::Asym,
        vec![0u8; 8], // minimal valid asym (gen=0, count=0)
    ));

    // .mrbc bincode
    archive.add(ArchiveMember::new(
        Ordinal::new("00000002").unwrap(),
        FourCC::Mrbc,
        vec![0x00, 0x0F], // Nop, Sync
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
        Ordinal::new("00000001").unwrap(),
        FourCC::Asym,
        vec![0u8; 8],
    ));

    let result = archive.validate();
    assert!(matches!(result, Err(ArchiveError::MissingMeta)));
}

#[test]
fn validate_fails_missing_asym() {
    let mut archive = MtsmArchive::new();
    let manifest = TkvDocument::new();
    archive.add(ArchiveMember::new(
        Ordinal::zero(),
        FourCC::Meta,
        manifest.encode(),
    ));

    let result = archive.validate();
    assert!(matches!(result, Err(ArchiveError::MissingAsym)));
}

#[test]
fn member_data_integrity() {
    let payload = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE];
    let mut archive = MtsmArchive::new();
    archive.add(ArchiveMember::new(
        Ordinal::zero(),
        FourCC::Meta,
        TkvDocument::new().encode(),
    ));
    archive.add(ArchiveMember::new(
        Ordinal::new("00000001").unwrap(),
        FourCC::Asym,
        vec![0u8; 8],
    ));
    archive.add(ArchiveMember::new(
        Ordinal::new("00000002").unwrap(),
        FourCC::Mrbc,
        payload.clone(),
    ));

    let bytes = archive.to_ar_bytes();
    let restored = MtsmArchive::from_ar_bytes(&bytes).unwrap();

    let mrbc = restored.bincode_members();
    assert_eq!(mrbc.len(), 1);
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

// ── OID archive integration tests ──────────────────────────────────────

fn make_archive_with_osym() -> MtsmArchive {
    let mut archive = make_valid_archive();

    // Build a sorted .osym index
    let mut builder = OidIndexBuilder::new();
    builder.add_fqa(
        Oid::from_segments(&[1, 1, 1, 1, 1]),
        ImportKind::Component,
        Fqa::new(0x1000),
    );
    builder.add_fqa(
        Oid::from_segments(&[1, 1, 1, 1, 2]),
        ImportKind::Hook,
        Fqa::new(0x2000),
    );
    let osym_data = builder.build();

    archive.add(ArchiveMember::new(
        Ordinal::new("00000003").unwrap(),
        FourCC::Osym,
        osym_data,
    ));

    archive
}

#[test]
fn archive_with_osym_roundtrip() {
    let archive = make_archive_with_osym();
    archive.validate().unwrap();

    let bytes = archive.to_ar_bytes();
    let restored = MtsmArchive::from_ar_bytes(&bytes).unwrap();
    restored.validate().unwrap();

    assert!(restored.oid_index().is_some());
    let idx = restored.oid_index_parsed().unwrap().unwrap();
    assert_eq!(idx.len(), 2);

    // Lookup works through archive
    let entry = idx.lookup(Oid::from_segments(&[1, 1, 1, 1, 1])).unwrap();
    assert_eq!(entry.kind, ImportKind::Component);
    assert_eq!(entry.fqa().value(), 0x1000);

    let entry = idx.lookup(Oid::from_segments(&[1, 1, 1, 1, 2])).unwrap();
    assert_eq!(entry.kind, ImportKind::Hook);
    assert_eq!(entry.fqa().value(), 0x2000);
}

#[test]
fn archive_without_osym_is_valid() {
    let archive = make_valid_archive();
    archive.validate().unwrap();
    assert!(archive.oid_index().is_none());
    assert!(archive.oid_index_parsed().unwrap().is_none());
}

#[test]
fn archive_with_odat_members() {
    let mut archive = make_valid_archive();
    archive.add(ArchiveMember::new(
        Ordinal::new("00000010").unwrap(),
        FourCC::Odat,
        vec![0u8; 64], // placeholder embedding data
    ));
    archive.add(ArchiveMember::new(
        Ordinal::new("00000011").unwrap(),
        FourCC::Odat,
        vec![0u8; 64],
    ));

    let bytes = archive.to_ar_bytes();
    let restored = MtsmArchive::from_ar_bytes(&bytes).unwrap();

    let odat = restored.oid_data_members();
    assert_eq!(odat.len(), 2);
    assert_eq!(odat[0].fourcc, FourCC::Odat);
    assert_eq!(odat[1].fourcc, FourCC::Odat);
}

#[test]
fn archive_validates_osym_sorting() {
    let mut archive = make_valid_archive();

    // Manually build unsorted .osym data
    let mut buf = Vec::new();
    buf.extend_from_slice(&2u32.to_le_bytes()); // count=2
    buf.extend_from_slice(&[0u8; 4]); // reserved

    // Entry 0: larger OID (1.1.2)
    let oid_big = Oid::from_segments(&[1, 1, 2]);
    buf.extend_from_slice(&oid_big.hi.to_le_bytes());
    buf.extend_from_slice(&oid_big.lo.to_le_bytes());
    buf.push(ImportKind::Symbol as u8);
    buf.extend_from_slice(&[0u8; 7 + 24]);

    // Entry 1: smaller OID (1.1.1) — wrong order
    let oid_small = Oid::from_segments(&[1, 1, 1]);
    buf.extend_from_slice(&oid_small.hi.to_le_bytes());
    buf.extend_from_slice(&oid_small.lo.to_le_bytes());
    buf.push(ImportKind::Symbol as u8);
    buf.extend_from_slice(&[0u8; 7 + 24]);

    archive.add(ArchiveMember::new(
        Ordinal::new("00000003").unwrap(),
        FourCC::Osym,
        buf,
    ));

    assert!(archive.validate().is_err());
}
