//! Tests for Triple-Arena memory and DMOVE engine.

use matterstream::arena::{ArenaError, TripleArena};
use matterstream::dmove::{DmoveDescriptor, DmoveEngine, DmoveSource};
use matterstream::ova::ArenaId;

#[test]
fn nursery_alloc_read_roundtrip() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_nursery(16).unwrap();
    assert_eq!(ova.arena(), ArenaId::Nursery);

    arenas.write(ova, &[1, 2, 3, 4]).unwrap();
    let data = arenas.read(ova).unwrap();
    assert_eq!(&data[..4], &[1, 2, 3, 4]);
}

#[test]
fn dynamic_arena_alloc_write_read() {
    let mut arenas = TripleArena::new();
    // Staging is the inactive arena
    let ova = arenas.alloc_staging(32).unwrap();

    let payload = b"hello arena";
    arenas.write(ova, payload).unwrap();
    let data = arenas.read(ova).unwrap();
    assert_eq!(&data[..payload.len()], payload);
}

#[test]
fn sync_swaps_active() {
    let mut arenas = TripleArena::new();
    let initial_active = arenas.active_arena();
    assert_eq!(initial_active, ArenaId::DynamicA);

    let result = arenas.sync();
    assert_eq!(result.old_active, ArenaId::DynamicA);
    assert_eq!(result.new_active, ArenaId::DynamicB);
    assert_eq!(arenas.active_arena(), ArenaId::DynamicB);

    // Sync again
    let result2 = arenas.sync();
    assert_eq!(result2.new_active, ArenaId::DynamicA);
}

#[test]
fn generation_mismatch_rejected() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_nursery(8).unwrap();

    // Artificially create a bad OVA with wrong generation
    let bad_ova = ova.next_generation();
    let result = arenas.read(bad_ova);
    assert!(matches!(result, Err(ArenaError::GenerationMismatch { .. })));
}

#[test]
fn arena_full_out_of_space() {
    let mut arenas = TripleArena::new();
    // Nursery has 256 slots
    for _ in 0..256 {
        arenas.alloc_nursery(1).unwrap();
    }
    let result = arenas.alloc_nursery(1);
    assert!(matches!(result, Err(ArenaError::OutOfSpace)));
}

#[test]
fn nursery_free_rejected() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_nursery(8).unwrap();
    let result = arenas.free(ova);
    assert!(matches!(result, Err(ArenaError::NurseryWriteViolation)));
}

#[test]
fn nursery_survives_sync() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_nursery(4).unwrap();
    arenas.write(ova, &[0xAA, 0xBB, 0xCC, 0xDD]).unwrap();

    arenas.sync();
    arenas.sync();

    // Nursery data still readable
    let data = arenas.read(ova).unwrap();
    assert_eq!(&data[..4], &[0xAA, 0xBB, 0xCC, 0xDD]);
}

#[test]
fn dynamic_free_and_realloc() {
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_staging(8).unwrap();
    arenas.write(ova, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();

    arenas.free(ova).unwrap();

    // Re-reading freed object should fail
    let result = arenas.read(ova);
    assert!(result.is_err());
}

// --- DMOVE tests ---

#[test]
fn dmove_buffer_transfer() {
    let mut arenas = TripleArena::new();
    let dest = arenas.alloc_nursery(16).unwrap();

    let desc = DmoveDescriptor {
        source: DmoveSource::Buffer(vec![10, 20, 30, 40]),
        dest_ova: dest,
        length: 4,
        source_offset: 0,
    };

    let transferred = DmoveEngine::execute(&mut arenas, &[desc]).unwrap();
    assert_eq!(transferred, 4);

    let data = arenas.read(dest).unwrap();
    assert_eq!(&data[..4], &[10, 20, 30, 40]);
}

#[test]
fn dmove_nursery_ref_cross_arena() {
    let mut arenas = TripleArena::new();

    // Source in nursery
    let src = arenas.alloc_nursery(8).unwrap();
    arenas.write(src, &[0xDE, 0xAD, 0xBE, 0xEF, 0, 0, 0, 0]).unwrap();

    // Dest in staging
    let dest = arenas.alloc_staging(8).unwrap();

    let desc = DmoveDescriptor {
        source: DmoveSource::NurseryRef(src),
        dest_ova: dest,
        length: 4,
        source_offset: 0,
    };

    let transferred = DmoveEngine::execute(&mut arenas, &[desc]).unwrap();
    assert_eq!(transferred, 4);

    let data = arenas.read(dest).unwrap();
    assert_eq!(&data[..4], &[0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn dmove_source_too_short() {
    let mut arenas = TripleArena::new();
    let dest = arenas.alloc_nursery(16).unwrap();

    let desc = DmoveDescriptor {
        source: DmoveSource::Buffer(vec![1, 2]),
        dest_ova: dest,
        length: 10, // too long
        source_offset: 0,
    };

    let result = DmoveEngine::execute(&mut arenas, &[desc]);
    assert!(result.is_err());
}
