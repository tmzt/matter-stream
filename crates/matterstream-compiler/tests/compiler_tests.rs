use matterstream_compiler::{Compiler, CompilerResult};
use matterstream_core::{Op, Primitive};

#[test]
fn test_compile_empty_tsx() -> CompilerResult<()> {
    let tsx_source = "";
    let compiled_ops = Compiler::compile(tsx_source)?;
    assert!(compiled_ops.ops.is_empty());
    Ok(())
}

#[test]
fn test_compile_simple_slab() -> CompilerResult<()> {
    let tsx_source = r#"
        function App() {
            return <Slab x={10} y={20} color={"\u{23}FF0000FF"} />;
        }
    "#;
    let compiled_ops = Compiler::compile(tsx_source)?;
    
    // Expected ops: SetColor, SetTrans, Draw
    assert_eq!(compiled_ops.ops.len(), 3);

    // Verify SetColor
    if let Op::SetColor(color) = compiled_ops.ops[0] {
        assert_eq!(color, [1.0, 0.0, 0.0, 1.0]);
    } else {
        panic!("Expected SetColor op, got {:?}", compiled_ops.ops[0]);
    }

    // Verify SetTrans
    if let Op::SetTrans(trans) = compiled_ops.ops[1] {
        assert_eq!(trans, [10.0, 20.0, 0.0]);
    } else {
        panic!("Expected SetTrans op, got {:?}", compiled_ops.ops[1]);
    }

    // Verify Draw
    if let Op::Draw { primitive, position_rsi: _ } = &compiled_ops.ops[2] {
        assert_eq!(*primitive, Primitive::Slab);
    } else {
        panic!("Expected Draw op, got {:?}", compiled_ops.ops[2]);
    }
    
    Ok(())
}

#[test]
fn test_compile_multiple_slabs() -> CompilerResult<()> {
    let tsx_source = r#"
        function App() {
            return (
                <>
                    <Slab x={1} y={2} color={"\u{23}00FF00FF"} />
                    <Slab x={3} y={4} color={"\u{23}0000FFFF"} />
                </>
            );
        }
    "#;
    let compiled_ops = Compiler::compile(tsx_source)?;

    // Expected ops: (SetColor, SetTrans, Draw) * 2 = 6 ops
    assert_eq!(compiled_ops.ops.len(), 6);

    // First Slab
    if let Op::SetColor(color) = compiled_ops.ops[0] {
        assert_eq!(color, [0.0, 1.0, 0.0, 1.0]);
    } else { panic!("Expected SetColor op, got {:?}", compiled_ops.ops[0]); }
    if let Op::SetTrans(trans) = compiled_ops.ops[1] {
        assert_eq!(trans, [1.0, 2.0, 0.0]);
    } else { panic!("Expected SetTrans op, got {:?}", compiled_ops.ops[1]); }
    if let Op::Draw { primitive, position_rsi: _ } = &compiled_ops.ops[2] {
        assert_eq!(*primitive, Primitive::Slab);
    } else { panic!("Expected Draw op, got {:?}", compiled_ops.ops[2]); }

    // Second Slab
    if let Op::SetColor(color) = compiled_ops.ops[3] {
        assert_eq!(color, [0.0, 0.0, 1.0, 1.0]);
    } else { panic!("Expected SetColor op, got {:?}", compiled_ops.ops[3]); }
    if let Op::SetTrans(trans) = compiled_ops.ops[4] {
        assert_eq!(trans, [3.0, 4.0, 0.0]);
    } else { panic!("Expected SetTrans op, got {:?}", compiled_ops.ops[4]); }
    if let Op::Draw { primitive, position_rsi: _ } = &compiled_ops.ops[5] {
        assert_eq!(*primitive, Primitive::Slab);
    } else { panic!("Expected Draw op, got {:?}", compiled_ops.ops[5]); }

    Ok(())
}

#[test]
fn test_compile_slab_default_color() -> CompilerResult<()> {
    let tsx_source = r#"
        function App() {
            return <Slab x={10} y={20} />;
        }
    "#;
    let compiled_ops = Compiler::compile(tsx_source)?;
    
    // Expected ops: SetColor (default white), SetTrans, Draw
    assert_eq!(compiled_ops.ops.len(), 3);

    // Verify SetColor is default white
    if let Op::SetColor(color) = compiled_ops.ops[0] {
        assert_eq!(color, [1.0, 1.0, 1.0, 1.0]);
    } else {
        panic!("Expected default white SetColor op, got {:?}", compiled_ops.ops[0]);
    }
    
    Ok(())
}

#[test]
fn test_compile_invalid_color_format() -> CompilerResult<()> {
    let tsx_source = r#"
        function App() {
            return <Slab x={10} y={20} color={"\u{23}INVALID"} />;
        }
    "#;
    let compiled_ops = Compiler::compile(tsx_source)?;
    
    // Should still compile but use default white color
    assert_eq!(compiled_ops.ops.len(), 3);
    if let Op::SetColor(color) = compiled_ops.ops[0] {
        assert_eq!(color, [1.0, 1.0, 1.0, 1.0]);
    } else {
        panic!("Expected default white SetColor op due to invalid format, got {:?}", compiled_ops.ops[0]);
    }
    
    Ok(())
}

#[test]
fn test_compile_invalid_tsx_syntax() {
    let tsx_source = r#"
        function App() {
            return <Slab x={10 y={20} />; // Missing closing brace
        }
    "#;
    let result = Compiler::compile(tsx_source);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Expected `}`"));
}
