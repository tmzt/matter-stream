//! End-to-end test: compile components → package → load → execute with imports.

#[cfg(feature = "compiler")]
#[test]
fn component_import_roundtrip() {
    use std::collections::HashMap;
    use matterstream_compiler::{compile_to_asm, compile_to_asm_with_imports, ImportMap};
    use matterstream_packaging::builder::{PackageBuilder, CompiledComponent};
    use matterstream_packaging::loader::load_package;
    use matterstream_vm::rpn::{RpnVm, ComponentEntry};
    use matterstream_vm_addressing::oid::Oid;
    use matterstream::arena::TripleArena;

    // 1. Compile a simple component
    let status_bar_tsx = r##"
<>
  <Slab x={0} y={0} w={412} h={48} radius={0} color="#0A0A12FF" />
  <Text x={16} y={14} size={14} label="9:41" color="#FFFFFFFF" />
</>
    "##;
    let status_asm = compile_to_asm(status_bar_tsx).expect("compile status_bar");
    println!("StatusBar: {} bytes, {} strings", status_asm.bytecode.len(), status_asm.string_table.len());

    let inbox_tsx = r##"
<>
  <Slab x={10} y={10} w={392} h={200} radius={24} color="#1A1A28FF" />
  <Text x={28} y={30} size={22} label="Inbox" color="#FFFFFFFF" />
  <Text x={28} y={60} size={14} label="3 messages" color="#888888FF" />
</>
    "##;
    let inbox_asm = compile_to_asm(inbox_tsx).expect("compile inbox");
    println!("InboxCard: {} bytes, {} strings", inbox_asm.bytecode.len(), inbox_asm.string_table.len());

    // 2. Build package
    let status_oid = Oid::new(0, 0x0101);
    let inbox_oid = Oid::new(0, 0x0102);

    let mut pkg = PackageBuilder::new("test-ui-kit");
    pkg.add_with_fqa("StatusBar", status_oid, 0xAA01, CompiledComponent {
        bytecode: status_asm.bytecode.clone(),
        string_table: status_asm.string_table.clone(),
    });
    pkg.add_with_fqa("InboxCard", inbox_oid, 0xAA02, CompiledComponent {
        bytecode: inbox_asm.bytecode.clone(),
        string_table: inbox_asm.string_table.clone(),
    });
    let archive = pkg.build();

    // 3. Serialize and reload
    let ar_bytes = archive.to_ar_bytes();
    println!("Archive: {} bytes", ar_bytes.len());
    let parsed = matterstream_packaging::archive::MtsmArchive::from_ar_bytes(&ar_bytes)
        .expect("parse archive");

    // 4. Load into VM
    let loaded = load_package(&parsed).expect("load package");
    println!("Loaded: {} components, {} strings, {} bytecode bytes",
        loaded.components.len(), loaded.strings.len(), loaded.bytecode.len());

    let mut vm = RpnVm::new();

    // Register .osym for OID resolution
    if let Some(osym) = loaded.osym {
        vm.oid_indices.push(osym);
    }

    // Append strings
    let string_base = vm.string_table.len() as u32;
    vm.string_table.extend(loaded.strings);

    // Register bytecode
    let bytecode_id = vm.loaded_bytecodes.len() as u16;
    vm.loaded_bytecodes.push(loaded.bytecode);

    // Register components
    for comp in &loaded.components {
        vm.component_table.insert(comp.fqa, ComponentEntry {
            bytecode_id,
            offset: comp.offset,
            length: comp.length,
            string_base: string_base + comp.string_base,
        });
    }

    println!("VM component_table: {:?}", vm.component_table.keys().collect::<Vec<_>>());

    // 5. Compile consumer TSX with imports
    let consumer_tsx = r##"
import { StatusBar, InboxCard } from "@chitin/ui-kit"
<>
  <Slab x={0} y={0} w={412} h={915} radius={0} color="#0E0E16FF" />
  <StatusBar />
  <InboxCard />
</>
    "##;

    let mut import_map = ImportMap::new();
    let mut pkg_imports = HashMap::new();
    pkg_imports.insert("StatusBar".to_string(), status_oid);
    pkg_imports.insert("InboxCard".to_string(), inbox_oid);
    import_map.packages.insert("@chitin/ui-kit".to_string(), pkg_imports);

    let consumer_asm = compile_to_asm_with_imports(consumer_tsx, &import_map)
        .expect("compile consumer");
    println!("Consumer: {} bytes bytecode", consumer_asm.bytecode.len());

    // 6. Execute
    vm.string_table.extend(consumer_asm.string_table);
    vm.cr_bank[1] = matterstream_vm::rpn::SECURITY_INTERNAL as u32;

    let mut arenas = TripleArena::new();
    vm.execute(&consumer_asm.bytecode, &mut arenas).expect("execute consumer");

    // 7. Verify: should have SDF draw commands from:
    //    - consumer's background Slab (1)
    //    - StatusBar's 2 draws (Slab + Text)
    //    - InboxCard's 3 draws (Slab + 2 Text)
    //    = 6 total
    println!("SDF draws: {}", vm.sdf_draws.len());
    for (i, cmd) in vm.sdf_draws.iter().enumerate() {
        println!("  [{}] ty={}", i, cmd.params[0] as u32);
    }
    assert!(vm.sdf_draws.len() >= 6, "expected at least 6 SDF draws, got {}", vm.sdf_draws.len());

    // Verify the background slab is from the consumer
    assert_eq!(vm.sdf_draws[0].size[0], 412.0); // consumer's 412-wide background
    assert_eq!(vm.sdf_draws[0].size[1], 915.0);
}
