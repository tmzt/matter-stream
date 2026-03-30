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

#[test]
fn test_compile_hbox_layout() -> CompilerResult<()> {
    let tsx_source = r##"
        <>
          <HBox x={0} y={0.3} gap={0.25}>
            <Slab color="#E74C3CFF" />
            <Slab color="#2ECC71FF" />
            <Slab color="#3498DBFF" />
          </HBox>
        </>
    "##;
    let compiled_ops = Compiler::compile(tsx_source)?;

    // HBox emits: PushState, then per-child (SetColor, SetTrans, Draw), then PopState
    // 1 PushState + 3*(SetColor + SetTrans + Draw) + 1 PopState = 11 ops
    assert_eq!(compiled_ops.ops.len(), 11, "ops: {:?}", compiled_ops.ops);

    // First op: PushState
    assert!(matches!(compiled_ops.ops[0], Op::PushState));

    // Child 0: color, trans at (0 + 0*(0.2+0.25), 0.3) = (0.0, 0.3)
    if let Op::SetTrans(trans) = compiled_ops.ops[2] {
        assert!((trans[0] - 0.0).abs() < 0.01, "child 0 x: {}", trans[0]);
        assert!((trans[1] - 0.3).abs() < 0.01, "child 0 y: {}", trans[1]);
    } else { panic!("Expected SetTrans, got {:?}", compiled_ops.ops[2]); }

    // Child 1: trans at (0 + 1*(0.2+0.25), 0.3) = (0.45, 0.3)
    if let Op::SetTrans(trans) = compiled_ops.ops[5] {
        assert!((trans[0] - 0.45).abs() < 0.01, "child 1 x: {}", trans[0]);
        assert!((trans[1] - 0.3).abs() < 0.01, "child 1 y: {}", trans[1]);
    } else { panic!("Expected SetTrans, got {:?}", compiled_ops.ops[5]); }

    // Child 2: trans at (0 + 2*(0.2+0.25), 0.3) = (0.9, 0.3)
    if let Op::SetTrans(trans) = compiled_ops.ops[8] {
        assert!((trans[0] - 0.9).abs() < 0.01, "child 2 x: {}", trans[0]);
        assert!((trans[1] - 0.3).abs() < 0.01, "child 2 y: {}", trans[1]);
    } else { panic!("Expected SetTrans, got {:?}", compiled_ops.ops[8]); }

    // Last op: PopState
    assert!(matches!(compiled_ops.ops[10], Op::PopState));

    Ok(())
}

#[test]
fn test_compile_vbox_layout() -> CompilerResult<()> {
    let tsx_source = r##"
        <>
          <VBox x={-0.5} y={0.4}>
            <Slab color="#F1C40FFF" />
            <Slab color="#9B59B6FF" />
          </VBox>
        </>
    "##;
    let compiled_ops = Compiler::compile(tsx_source)?;

    // VBox: PushState + 2*(SetColor + SetTrans + Draw) + PopState = 8 ops
    assert_eq!(compiled_ops.ops.len(), 8, "ops: {:?}", compiled_ops.ops);

    assert!(matches!(compiled_ops.ops[0], Op::PushState));

    // Child 0: trans at (-0.5, 0.4 - 0*(0.15+0.1)) = (-0.5, 0.4)
    if let Op::SetTrans(trans) = compiled_ops.ops[2] {
        assert!((trans[0] - (-0.5)).abs() < 0.01, "child 0 x: {}", trans[0]);
        assert!((trans[1] - 0.4).abs() < 0.01, "child 0 y: {}", trans[1]);
    } else { panic!("Expected SetTrans, got {:?}", compiled_ops.ops[2]); }

    // Child 1: trans at (-0.5, 0.4 - 1*(0.15+0.1)) = (-0.5, 0.15)
    if let Op::SetTrans(trans) = compiled_ops.ops[5] {
        assert!((trans[0] - (-0.5)).abs() < 0.01, "child 1 x: {}", trans[0]);
        assert!((trans[1] - 0.15).abs() < 0.01, "child 1 y: {}", trans[1]);
    } else { panic!("Expected SetTrans, got {:?}", compiled_ops.ops[5]); }

    assert!(matches!(compiled_ops.ops[7], Op::PopState));

    Ok(())
}

#[test]
fn test_compile_login_form() -> CompilerResult<()> {
    let tsx_source = r##"
        <>
          <Text x={0.0} y={0.7} color="#FFFFFFCC" label="Sign In" />
          <VBox x={0.0} y={0.25} gap={0.05}>
            <Text color="#AAAAAAFF" label="Username" />
            <Slab width={0.5} height={0.06} color="#333333FF" />
            <Text color="#AAAAAAFF" label="Password" />
            <Slab width={0.5} height={0.06} color="#333333FF" />
          </VBox>
          <Slab x={0.0} y={-0.55} padding={10} color="#1A73E8FF">
            <Text label="Login" color="#FFFFFFFF" />
          </Slab>
        </>
    "##;
    let compiled_ops = Compiler::compile(tsx_source)?;

    // Title: SetLabel, SetColor, SetTrans, Draw = 4
    // VBox: PushState + 4 children + PopState
    //   Text children: SetLabel + SetColor + SetTrans + Draw = 4 each (x2)
    //   Slab children: SetColor + SetSize + SetTrans + Draw = 4 each (x2)
    // Login button (nested text): SetTextColor + SetLabel + SetPadding + SetColor + SetTrans + Draw = 6
    // Total: 4 + 1 + 4*4 + 1 + 6 = 28
    assert!(compiled_ops.ops.len() > 20, "Expected many ops, got {}: {:?}", compiled_ops.ops.len(), compiled_ops.ops);

    // First op should be SetLabel for the title text
    assert!(matches!(compiled_ops.ops[0], Op::SetLabel(_)));

    Ok(())
}

#[test]
fn test_compile_width_height() -> CompilerResult<()> {
    let tsx_source = r##"
        <Slab x={0} y={0} width={0.5} height={0.3} color="#FFFFFFFF" />
    "##;
    let compiled_ops = Compiler::compile(tsx_source)?;

    // SetColor, SetSize, SetTrans, Draw = 4 ops
    assert_eq!(compiled_ops.ops.len(), 4, "ops: {:?}", compiled_ops.ops);

    if let Op::SetSize(size) = compiled_ops.ops[1] {
        assert!((size[0] - 0.5).abs() < 0.01);
        assert!((size[1] - 0.3).abs() < 0.01);
    } else { panic!("Expected SetSize, got {:?}", compiled_ops.ops[1]); }

    Ok(())
}

#[test]
fn test_compile_nested_text_in_slab() -> CompilerResult<()> {
    let tsx_source = r##"
        <Slab padding={8} color="#0000FFFF">
          <Text label="Hi" color="#FF0000FF" />
        </Slab>
    "##;
    let compiled_ops = Compiler::compile(tsx_source)?;

    // Expected ops: SetTextColor, SetLabel, SetPadding, SetColor, SetTrans, Draw = 6
    assert_eq!(compiled_ops.ops.len(), 6, "ops: {:?}", compiled_ops.ops);

    // SetTextColor from nested Text's color
    if let Op::SetTextColor(tc) = compiled_ops.ops[0] {
        assert_eq!(tc, [1.0, 0.0, 0.0, 1.0]); // #FF0000FF
    } else { panic!("Expected SetTextColor, got {:?}", compiled_ops.ops[0]); }

    // SetLabel from nested Text's label
    if let Op::SetLabel(ref label) = compiled_ops.ops[1] {
        assert_eq!(label, "Hi");
    } else { panic!("Expected SetLabel, got {:?}", compiled_ops.ops[1]); }

    // SetPadding from Slab's padding={8}
    if let Op::SetPadding(p) = compiled_ops.ops[2] {
        assert_eq!(p, [8.0, 8.0, 8.0, 8.0]);
    } else { panic!("Expected SetPadding, got {:?}", compiled_ops.ops[2]); }

    // SetColor from Slab's color
    if let Op::SetColor(c) = compiled_ops.ops[3] {
        assert_eq!(c, [0.0, 0.0, 1.0, 1.0]); // #0000FFFF
    } else { panic!("Expected SetColor, got {:?}", compiled_ops.ops[3]); }

    // SetTrans
    assert!(matches!(compiled_ops.ops[4], Op::SetTrans(_)));

    // Draw Slab
    if let Op::Draw { primitive, .. } = &compiled_ops.ops[5] {
        assert_eq!(*primitive, Primitive::Slab);
    } else { panic!("Expected Draw, got {:?}", compiled_ops.ops[5]); }

    Ok(())
}

#[test]
fn test_compile_slab_without_label() -> CompilerResult<()> {
    let tsx_source = r##"
        <Slab color="#FFFFFFFF" label="X" />
    "##;
    let compiled_ops = Compiler::compile(tsx_source)?;

    // label attribute on Slab is now ignored — no SetLabel should be emitted
    // Expected ops: SetColor, SetTrans, Draw = 3
    assert_eq!(compiled_ops.ops.len(), 3, "ops: {:?}", compiled_ops.ops);

    assert!(matches!(compiled_ops.ops[0], Op::SetColor(_)));
    assert!(matches!(compiled_ops.ops[1], Op::SetTrans(_)));
    if let Op::Draw { primitive, .. } = &compiled_ops.ops[2] {
        assert_eq!(*primitive, Primitive::Slab);
    } else { panic!("Expected Draw, got {:?}", compiled_ops.ops[2]); }

    // Verify no SetLabel was emitted
    for op in &compiled_ops.ops {
        assert!(!matches!(op, Op::SetLabel(_)), "Slab should not emit SetLabel from its own label attr");
    }

    Ok(())
}
