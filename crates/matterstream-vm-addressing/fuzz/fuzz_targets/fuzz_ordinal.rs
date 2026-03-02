#![no_main]

use libfuzzer_sys::fuzz_target;
use matterstream_vm_addressing::fqa::{Fqa, Ordinal, OrdinalError};

fuzz_target!(|data: &[u8]| {
    // Fuzz Ordinal::new with arbitrary strings
    if let Ok(s) = std::str::from_utf8(data) {
        match Ordinal::new(s) {
            Ok(ord) => {
                // Valid ordinal: round-trip through u64 must succeed.
                let val = ord.to_u64();
                let ord2 = Ordinal::from_u64(val);
                assert_eq!(ord, ord2);

                // prefix must not panic
                let _ = ord.prefix();

                // Display must not panic
                let _ = format!("{}", ord);

                // FQA round-trip through ordinal
                let fqa = Fqa::from_ordinal(&ord);
                let ord3 = fqa.to_ordinal();
                assert_eq!(ord3.to_u64(), val);
            }
            Err(OrdinalError::InvalidLength(_)) => {}
            Err(OrdinalError::InvalidChar(_)) => {}
        }
    }

    // Fuzz Ordinal::from_u64 with arbitrary u64 values
    if data.len() >= 8 {
        let val = u64::from_le_bytes(data[..8].try_into().unwrap());
        let ord = Ordinal::from_u64(val);
        // Truncation to 62^8 range: decode must give back a value in range.
        let decoded = ord.to_u64();
        assert!(decoded < 62u64.pow(8));
    }
});
