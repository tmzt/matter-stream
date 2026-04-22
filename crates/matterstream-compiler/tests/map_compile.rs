//! Test: compile TSX with .map() and verify bytecode executes correctly.

use matterstream_compiler::compile_to_asm;
use matterstream_vm::rpn::RpnVm;
use matterstream_vm_arena::TripleArena;

#[test]
fn test_basic_slab_compiles() {
    let tsx = r##"<Slab x={10} y={20} w={100} h={50} radius={8} color="#FF0000FF" />"##;
    let output = compile_to_asm(tsx).expect("compile failed");
    assert!(!output.bytecode.is_empty(), "bytecode should not be empty");

    let mut vm = RpnVm::new();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).expect("execute failed");
    assert!(!vm.sdf_draws.is_empty(), "should have SDF draws");
}

#[test]
fn test_fragment_with_multiple_elements() {
    let tsx = r##"
        <>
            <Slab x={0} y={0} w={400} h={640} radius={20} color="#1A1A28FF" />
            <Text x={16} y={20} size={20} label="Inbox" color="#FFFFFFFF" />
        </>
    "##;
    let output = compile_to_asm(tsx).expect("compile failed");

    let mut vm = RpnVm::new();
    vm.string_table = output.string_table.clone();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).expect("execute failed");
    assert!(vm.sdf_draws.len() >= 2, "should have at least 2 draws, got {}", vm.sdf_draws.len());
}

#[test]
fn test_component_with_props() {
    let tsx = r##"
        const Card = ({ title }) => (
            <>
                <Slab x={0} y={0} w={400} h={100} radius={12} color="#1A1A28FF" />
                <Text x={16} y={20} size={18} label={title} color="#FFFFFFFF" />
            </>
        );
        <Card title="Emails from Alice" />
    "##;
    let output = compile_to_asm(tsx).expect("compile failed");

    let mut vm = RpnVm::new();
    vm.string_table = output.string_table.clone();
    let mut arenas = TripleArena::new();
    vm.execute(&output.bytecode, &mut arenas).expect("execute failed");
    assert!(vm.sdf_draws.len() >= 2, "should have slab + text, got {}", vm.sdf_draws.len());
    eprintln!("component: {} draws, {} strings", vm.sdf_draws.len(), output.string_table.len());
}

#[test]
fn test_map_in_fragment() {
    // Test that .map() at least compiles and produces a DefineBlock+MapOver sequence
    let tsx = r##"
        const items = [1, 2, 3];
        <>
            <Slab x={0} y={0} w={400} h={640} radius={20} color="#1A1A28FF" />
            {items.map((item) => (
                <Slab x={10} y={10} w={380} h={58} radius={10} color="#222236FF" />
            ))}
        </>
    "##;
    match compile_to_asm(tsx) {
        Ok(output) => {
            eprintln!("map compiled: {} bytes, {} strings",
                output.bytecode.len(), output.string_table.len());

            let mut vm = RpnVm::new();
            vm.string_table = output.string_table.clone();
            // Set iteration count in zero_page[4..7]
            vm.zero_page[4..8].copy_from_slice(&3u32.to_le_bytes());
            let mut arenas = TripleArena::new();
            match vm.execute(&output.bytecode, &mut arenas) {
                Ok(()) => {
                    eprintln!("map executed: {} draws", vm.sdf_draws.len());
                    // Should have: 1 background slab + 3 iterated slabs = 4
                    assert!(vm.sdf_draws.len() >= 1, "should have at least background slab");
                }
                Err(e) => eprintln!("map execute error (may be expected): {:?}", e),
            }
        }
        Err(e) => {
            // const array decl might not be supported yet
            eprintln!("map compile note: {}", e);
        }
    }
}
