#![no_main]

use libfuzzer_sys::fuzz_target;
use matterstream_packaging::archive::{ArchiveError, MtsmArchive};

fuzz_target!(|data: &[u8]| {
    match MtsmArchive::from_ar_bytes(data) {
        Ok(archive) => {
            // Round-trip: serialize and re-parse.
            let bytes = archive.to_ar_bytes();
            let archive2 = MtsmArchive::from_ar_bytes(&bytes)
                .expect("round-trip re-parse must succeed");
            assert_eq!(archive.members.len(), archive2.members.len());
            for (a, b) in archive.members.iter().zip(archive2.members.iter()) {
                assert_eq!(a.ordinal, b.ordinal);
                assert_eq!(a.fourcc, b.fourcc);
                assert_eq!(a.data, b.data);
            }

            // validate must not panic
            let _ = archive.validate();

            // manifest parse must not panic
            let _ = archive.manifest();

            // asym lookup must not panic
            let _ = archive.asym();

            // bincode_members must not panic
            let _ = archive.bincode_members();
        }
        Err(ArchiveError::InvalidMagic) => {}
        Err(ArchiveError::InvalidHeader) => {}
        Err(ArchiveError::TruncatedData) => {}
        Err(ArchiveError::InvalidOrdinal(_)) => {}
        Err(ArchiveError::InvalidFourCC(_)) => {}
        Err(ArchiveError::MissingMeta) => {}
        Err(ArchiveError::MissingAsym) => {}
        Err(ArchiveError::Tkv(_)) => {}
    }
});
