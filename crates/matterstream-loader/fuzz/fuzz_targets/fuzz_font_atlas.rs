#![no_main]

use libfuzzer_sys::fuzz_target;
use matterstream_loader::fonts::{FontAtlasBin, FontAtlasError};

fuzz_target!(|data: &[u8]| {
    match FontAtlasBin::from_bytes(data) {
        Ok(bin) => {
            // Round-trip: serialise back and re-parse, must be identical.
            let bytes = bin.to_bytes();
            let bin2 = FontAtlasBin::from_bytes(&bytes)
                .expect("round-trip re-parse must succeed");
            assert_eq!(bin.header.version, bin2.header.version);
            assert_eq!(bin.header.font_type, bin2.header.font_type);
            assert_eq!(bin.header.atlas_rows, bin2.header.atlas_rows);
            assert_eq!(bin.header.atlas_cols, bin2.header.atlas_cols);
            assert_eq!(bin.header.glyph_rows, bin2.header.glyph_rows);
            assert_eq!(bin.header.glyph_cols, bin2.header.glyph_cols);
            assert_eq!(bin.header.pixel_format, bin2.header.pixel_format);
            assert_eq!(bin.data, bin2.data);
            assert_eq!(bytes.len(), FontAtlasBin::HEADER_SIZE + bin.data.len());
        }
        Err(FontAtlasError::TooShort(_)) => {}
        Err(FontAtlasError::BadMagic(_)) => {}
        Err(FontAtlasError::UnsupportedVersion(_)) => {}
        Err(FontAtlasError::ZeroGlyphDimension) => {}
        Err(FontAtlasError::UnknownPixelFormat(_)) => {}
        Err(FontAtlasError::DataLengthMismatch { .. }) => {}
    }
});
