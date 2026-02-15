//! Tests for the `Parser`.

use matterstream::*;

use std::io::Write;

#[test]
fn test_parse_sequence() {
    let input = "
        draw slab 0
        set_trans 1.0 2.0 3.0
        push 1 2 3 4
    ";
    let ops = Parser::parse(input).unwrap();

    assert_eq!(ops.len(), 3);
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
        Op::Push(payload) => assert_eq!(*payload, vec![1, 2, 3, 4]),
        _ => panic!("Expected Push op"),
    }
}

#[test]
fn test_parse_file() {
    let input = "
        draw slab 0
        set_trans 1.0 2.0 3.0
        push 1 2 3 4
    ";
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "{}", input).unwrap();
    let ops = Parser::parse_file(file.path().to_str().unwrap()).unwrap();

    assert_eq!(ops.len(), 3);
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
        Op::Push(payload) => assert_eq!(*payload, vec![1, 2, 3, 4]),
        _ => panic!("Expected Push op"),
    }
}
