#![no_main]

use libfuzzer_sys::fuzz_target;
use matterstream_vm_scl::keyless::{EntropyClass, KeylessPolicy};
use matterstream_vm_scl::scl::{shannon_entropy, Scl, SclVerdict};

fuzz_target!(|data: &[u8]| {
    // SCL validation must not panic on any input.
    let scl = Scl::default();
    let verdict = scl.validate(data);
    match verdict {
        SclVerdict::Accept => {}
        SclVerdict::RejectDictionaryExplosion => {}
        SclVerdict::RejectLiteralEscape => {}
        SclVerdict::RejectHighEntropy => {}
    }

    // load_member is the same path
    let verdict2 = scl.load_member(data);
    assert_eq!(verdict, verdict2);

    // validate_archive with single member must agree
    let members: Vec<&[u8]> = vec![data];
    let results = scl.validate_archive(&members);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, verdict);

    // shannon_entropy must not panic
    let e = shannon_entropy(data);
    assert!(e >= 0.0);
    assert!(e <= 8.0);

    // Keyless classification must not panic
    let policy = KeylessPolicy::new();
    let class = policy.classify(data);
    match class {
        EntropyClass::Structured | EntropyClass::Compressed => {
            // Must be storable
            assert!(policy.assert_storable(data).is_ok());
        }
        EntropyClass::Secret => {
            // Must be rejected for storage
            assert!(policy.assert_storable(data).is_err());
        }
    }

    // Transient always succeeds
    let _ = policy.assert_transient(data);
});
