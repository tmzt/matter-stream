use matterstream_compiler::Compiler;
use matterstream_processor::{Processor, ProcessorResult, ProcessorOutput};
use matterstream_packages::{CoreUiPackage, PackageRegistry};
use matterstream_core::{Op, Primitive, TsxFragment, MtsmExecFunctionalComponent, TsxElementContext, TsxAttributes};
use dashmap::DashMap; // For MtsmObject initialization in tests

#[test]
fn test_processor_simple_slab_import() -> ProcessorResult<()> {
    let tsx_source = r#"
        import { Slab } from '@mtsm/ui/core';
        function App() {
            return <Slab x={10} y={20} color={"\u{23}FF0000FF"} />;
        }
    "#;

    // 1. Compile TSX
    let compiler = Compiler;
    let compiled_ops = Compiler::compile(tsx_source)?;

    // 2. Setup PackageRegistry and Processor
    let mut package_registry = PackageRegistry::new();
    package_registry.register_package(CoreUiPackage);
    let processor = Processor::new();

    // 3. Process the compiled ops
    let processor_output = processor.process(compiled_ops, &package_registry)?;

    // Assertions on ProcessorOutput
    // For now, the processor simply passes through the ops from the compiler.
    // In future, it will resolve imports and expand them.
    assert_eq!(processor_output.ops.ops.len(), 3); // SetColor, SetTrans, Draw

    // Verify the ops are as expected from the compiler output
    if let Op::SetColor(color) = processor_output.ops.ops[0] {
        assert_eq!(color, [1.0, 0.0, 0.0, 1.0]);
    } else {
        panic!("Expected SetColor op, got {:?}", processor_output.ops.ops[0]);
    }
    if let Op::SetTrans(trans) = processor_output.ops.ops[1] {
        assert_eq!(trans, [10.0, 20.0, 0.0]);
    } else {
        panic!("Expected SetTrans op, got {:?}", processor_output.ops.ops[1]);
    }
    if let Op::Draw { primitive, position_rsi: _ } = &processor_output.ops.ops[2] {
        assert_eq!(*primitive, Primitive::Slab);
    } else {
        panic!("Expected Draw op, got {:?}", processor_output.ops.ops[2]);
    }

    // Assert that the root_fragment is still empty for now, as processing logic is not implemented
    assert!(processor_output.root_fragment.elements.is_empty());

    Ok(())
}

#[test]
fn test_processor_multiple_slabs_import() -> ProcessorResult<()> {
    let tsx_source = r#"
        import { Slab } from '@mtsm/ui/core';
        function App() {
            return (
                <>
                    <Slab x={1} y={2} color={"\u{23}00FF00FF"} />
                    <Slab x={3} y={4} color={"\u{23}0000FFFF"} />
                </>
            );
        }
    "#;

    // 1. Compile TSX
    let compiler = Compiler;
    let compiled_ops = Compiler::compile(tsx_source)?;

    // 2. Setup PackageRegistry and Processor
    let mut package_registry = PackageRegistry::new();
    package_registry.register_package(CoreUiPackage);
    let processor = Processor::new();

    // 3. Process the compiled ops
    let processor_output = processor.process(compiled_ops, &package_registry)?;

    assert_eq!(processor_output.ops.ops.len(), 6); // (SetColor, SetTrans, Draw) * 2

    // First Slab
    if let Op::SetColor(color) = processor_output.ops.ops[0] {
        assert_eq!(color, [0.0, 1.0, 0.0, 1.0]);
    } else { panic!("Expected SetColor op, got {:?}", processor_output.ops.ops[0]); }
    if let Op::SetTrans(trans) = processor_output.ops.ops[1] {
        assert_eq!(trans, [1.0, 2.0, 0.0]);
    } else { panic!("Expected SetTrans op, got {:?}", processor_output.ops.ops[1]); }
    if let Op::Draw { primitive, position_rsi: _ } = &processor_output.ops.ops[2] {
        assert_eq!(*primitive, Primitive::Slab);
    } else { panic!("Expected Draw op, got {:?}", processor_output.ops.ops[2]); }

    // Second Slab
    if let Op::SetColor(color) = processor_output.ops.ops[3] {
        assert_eq!(color, [0.0, 0.0, 1.0, 1.0]);
    } else { panic!("Expected SetColor op, got {:?}", processor_output.ops.ops[3]); }
    if let Op::SetTrans(trans) = processor_output.ops.ops[4] {
        assert_eq!(trans, [3.0, 4.0, 0.0]);
    } else { panic!("Expected SetTrans op, got {:?}", processor_output.ops.ops[4]); }
    if let Op::Draw { primitive, position_rsi: _ } = &processor_output.ops.ops[5] {
        assert_eq!(*primitive, Primitive::Slab);
    } else { panic!("Expected Draw op, got {:?}", processor_output.ops.ops[5]); }

    assert!(processor_output.root_fragment.elements.is_empty());

    Ok(())
}

#[test]
fn test_processor_unresolved_import() {
    let tsx_source = r#"
        import { NonExistent } from '@mtsm/ui/core';
        function App() {
            return <NonExistent />;
        }
    "#;

    // 1. Compile TSX
    let compiler = Compiler;
    let compiled_ops = Compiler::compile(tsx_source).unwrap(); // Should compile even with unresolved import

    // 2. Setup PackageRegistry (without registering "NonExistent") and Processor
    let mut package_registry = PackageRegistry::new();
    package_registry.register_package(CoreUiPackage); // Slab is registered, but not NonExistent
    let processor = Processor::new();

    // 3. Processing should currently pass through
    let processor_output_result = processor.process(compiled_ops, &package_registry);
    assert!(processor_output_result.is_ok()); // For now, it's ok, as it just passes through
    // In future, this would probably return an error or a special placeholder component.
}
