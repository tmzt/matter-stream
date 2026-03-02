#![no_main]

use libfuzzer_sys::fuzz_target;
use matterstream_packaging::tkv::{TkvDocument, TkvError};

fuzz_target!(|data: &[u8]| {
    match TkvDocument::decode(data) {
        Ok(doc) => {
            // Round-trip: encode and re-decode must produce identical document.
            let encoded = doc.encode();
            let doc2 = TkvDocument::decode(&encoded)
                .expect("round-trip re-decode must succeed");
            assert_eq!(doc, doc2);

            // strip_comments must not panic
            let mut doc3 = doc.clone();
            doc3.strip_comments();

            // ordinal_map must not panic
            let _ = doc.ordinal_map();
        }
        Err(TkvError::InvalidTypeByte(_)) => {}
        Err(TkvError::TruncatedData) => {}
        Err(TkvError::InvalidUtf8) => {}
    }
});
