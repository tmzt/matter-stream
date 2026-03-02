//! Tests for AR archive container.

use matterstream_packaging::archive::{ArchiveError, ArchiveMember, MtsmArchive};
use matterstream_vm_addressing::fqa::{FourCC, Ordinal};
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
