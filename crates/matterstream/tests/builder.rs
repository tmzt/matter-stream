//! Tests for the `StreamBuilder`.

use matterstream::*;

#[test]
fn test_build_sequence() {
    let ops = StreamBuilder::new()
        .draw(Primitive::Slab, 0)
        .set_trans([1.0, 2.0, 3.0])
        .push_proj()
        .pop_proj()
        .build();

    assert_eq!(ops.len(), 4);
    match &ops[0] {
        Op::Draw { primitive, position_rsi } => {
            assert!(matches!(primitive, Primitive::Slab));
            assert_eq!(*position_rsi, 0);
        }
        _ => panic!("Expected Draw op"),
    }
    match &ops[1] {
        Op::SetTrans(trans) => assert_eq!(*trans, [1.0, 2.0, 3.0]),
        _ => panic!("Expected SetTrans op"),
    }
    match &ops[2] {
        Op::PushProj => {}
        _ => panic!("Expected PushProj op"),
    }
    match &ops[3] {
        Op::PopProj => {}
        _ => panic!("Expected PopProj op"),
    }
}

#[test]
fn test_build_push() {
    smol::block_on(async {
        let data = vec![1, 2, 3, 4];
        let ops = StreamBuilder::new().push(data.clone()).build();

        let mut stream = MatterStream::new();
        let header = OpsHeader::new(vec![], false);
        stream.execute(&header, &ops).await.unwrap();

        assert_eq!(stream.stream, data);
    });
}
