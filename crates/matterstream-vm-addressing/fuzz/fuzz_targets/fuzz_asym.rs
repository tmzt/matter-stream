#![no_main]

use libfuzzer_sys::fuzz_target;
use matterstream_vm_addressing::aslr::{AslrToken, AsymError, AsymTable};

fuzz_target!(|data: &[u8]| {
    match AsymTable::from_bytes(data) {
        Ok(table) => {
            // Round-trip: serialize and re-parse.
            let bytes = table.to_bytes();
            let table2 = AsymTable::from_bytes(&bytes)
                .expect("round-trip re-parse must succeed");
            assert_eq!(table.len(), table2.len());
            assert_eq!(table.generation(), table2.generation());

            // Resolve every token from 0..256 -- must not panic.
            for i in 0..256u32 {
                let _ = table.resolve(AslrToken(i));
            }
        }
        Err(AsymError::TruncatedData) => {}
    }
});
