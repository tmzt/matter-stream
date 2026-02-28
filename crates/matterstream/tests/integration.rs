//! End-to-end integration tests: existing tests pass + full pipeline.

use matterstream::addressing::AddressResolver;
use matterstream::archive::{ArchiveMember, MtsmArchive};
use matterstream::arena::TripleArena;
use matterstream::aslr::{AslrToken, AsymTable};
use matterstream::builder::StreamBuilder;
use matterstream::fqa::{Fqa, FourCC, Ordinal};
use matterstream::keyless::KeylessPolicy;
use matterstream::ops::{Op, OpsHeader, Primitive, RsiPointer};
use matterstream::rpn::{RpnOp, RpnVm};
use matterstream::scl::{Scl, SclVerdict};
use matterstream::stream::MatterStream;
use matterstream::tkv::{TkvDocument, TkvValue};

// --- Regression: existing architecture tests pass ---

#[test]
fn regression_test_a_direct_register_access() {
    smol::block_on(async {
        let mut ms = MatterStream::new();
        let rsi = RsiPointer::new(1, 2, 0); // Tier 1, Vec3, index 0
        let header = OpsHeader::new(vec![rsi], true);

        ms.registers.vec3.write(0, [1.0, 2.0, 3.0]);

        let ops = vec![Op::Draw {
            primitive: Primitive::Slab,
            position_rsi: 0,
        }];

        ms.execute(&header, &ops).await.unwrap();
        assert_eq!(ms.draws.len(), 1);
        assert_eq!(ms.draws[0].position, [1.0, 2.0, 3.0]);
    });
}

#[test]
fn regression_test_d_translation_fast_path() {
    smol::block_on(async {
        let mut ms = MatterStream::new();
        let rsi = RsiPointer::new(1, 2, 0);
        let header = OpsHeader::new(vec![rsi], true);

        let ops = vec![
            Op::SetTrans([5.0, 10.0, 15.0]),
            Op::Draw {
                primitive: Primitive::Slab,
                position_rsi: 0,
            },
        ];

        ms.execute(&header, &ops).await.unwrap();
        assert!(ms.draws[0].used_fast_path);
        assert_eq!(ms.draws[0].transform_bytes, 12);
    });
}

// --- New VM_SPEC v0.1.0 integration tests ---

#[test]
fn matterstream_with_new_fields() {
    let ms = MatterStream::new();
    assert_eq!(ms.arenas.active_arena(), matterstream::ArenaId::DynamicA);
    assert_eq!(ms.rpn_vm.stack.len(), 0);
}

#[test]
fn op_sync_swaps_arenas() {
    smol::block_on(async {
        let mut ms = MatterStream::new();
        let header = OpsHeader::new(vec![], false);

        let ops = vec![Op::Sync];
        ms.execute(&header, &ops).await.unwrap();

        assert_eq!(ms.arenas.active_arena(), matterstream::ArenaId::DynamicB);
    });
}

#[test]
fn op_exec_rpn_runs_bytecode() {
    smol::block_on(async {
        let mut ms = MatterStream::new();
        let header = OpsHeader::new(vec![], false);

        let mut bytecode = vec![RpnOp::Push64 as u8];
        bytecode.extend_from_slice(&3u64.to_le_bytes());
        bytecode.push(RpnOp::Push64 as u8);
        bytecode.extend_from_slice(&4u64.to_le_bytes());
        bytecode.push(RpnOp::Add as u8);

        let ops = vec![Op::ExecRpn(bytecode)];
        ms.execute(&header, &ops).await.unwrap();

        assert_eq!(ms.rpn_vm.stack.len(), 1);
        assert_eq!(ms.rpn_vm.stack[0].as_u64().unwrap(), 7);
    });
}

#[test]
fn mixed_old_and_new_ops() {
    smol::block_on(async {
        let mut ms = MatterStream::new();
        let rsi = RsiPointer::new(1, 2, 0);
        let header = OpsHeader::new(vec![rsi], true);

        let ops = vec![
            Op::SetTrans([1.0, 2.0, 3.0]),
            Op::Draw {
                primitive: Primitive::Slab,
                position_rsi: 0,
            },
            Op::Sync,
            Op::Push(vec![0xAA, 0xBB]),
        ];

        ms.execute(&header, &ops).await.unwrap();
        assert_eq!(ms.draws.len(), 1);
        assert_eq!(ms.stream, vec![0xAA, 0xBB]);
    });
}

#[test]
fn full_pipeline_archive_scl_arena_rpn() {
    // Step 1: Build an archive
    let mut manifest = TkvDocument::new();
    manifest.push("name", TkvValue::String("pipeline-test".into()));
    manifest.push("main", TkvValue::Fqa(Fqa::new(0x1000)));

    let mut asym_table = AsymTable::new();
    let fqa = Fqa::new(0x1000);
    let token = AslrToken(0xBEEF);
    asym_table.insert(token, matterstream::Ova::new(matterstream::ArenaId::Nursery, 0, 0, 0));

    // RPN bytecode: Push32(42), Sync
    let push32_bytes = 42u32.to_le_bytes();
    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&push32_bytes)),
        (RpnOp::Sync, None),
    ]);

    let mut archive = MtsmArchive::new();
    archive.add(ArchiveMember::new(
        Ordinal::zero(),
        FourCC::Meta,
        manifest.encode(),
    ));
    archive.add(ArchiveMember::new(
        Ordinal::new("00000001").unwrap(),
        FourCC::Asym,
        asym_table.to_bytes(),
    ));
    archive.add(ArchiveMember::new(
        Ordinal::new("00000002").unwrap(),
        FourCC::Mrbc,
        bytecode.clone(),
    ));

    // Step 2: Validate archive
    archive.validate().unwrap();

    // Step 3: SCL validates all members
    let scl = Scl::default();
    for member in &archive.members {
        assert_eq!(scl.load_member(&member.data), SclVerdict::Accept);
    }

    // Step 4: Load into arena
    let mut arenas = TripleArena::new();
    let ova = arenas.alloc_nursery(bytecode.len()).unwrap();
    arenas.write(ova, &bytecode).unwrap();

    // Step 5: Execute RPN
    let mut rpn = RpnVm::new();
    rpn.execute(&bytecode, &mut arenas).unwrap();

    assert_eq!(rpn.stack.len(), 1);
    assert_eq!(rpn.stack[0].as_u32().unwrap(), 42);
    assert!(rpn.synced);

    // Step 6: Keyless enforcement
    let keyless = KeylessPolicy::new();
    keyless.assert_storable(&bytecode).unwrap();

    // Step 7: Address resolution
    let mut resolver = AddressResolver::new();
    resolver.register(fqa, token, ova);
    let resolved_ova = resolver.resolve(fqa).unwrap();
    assert_eq!(resolved_ova.arena(), matterstream::ArenaId::Nursery);
}

#[test]
fn builder_with_new_ops() {
    let fqa = Fqa::new(42);
    let ops = StreamBuilder::new()
        .resolve_fqa(fqa)
        .sync()
        .exec_rpn(vec![RpnOp::Nop as u8])
        .build();

    assert_eq!(ops.len(), 3);
    assert!(matches!(ops[0], Op::ResolveFqa(_)));
    assert!(matches!(ops[1], Op::Sync));
    assert!(matches!(ops[2], Op::ExecRpn(_)));
}
