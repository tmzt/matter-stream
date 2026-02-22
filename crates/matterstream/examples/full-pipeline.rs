//! Example: Full MatterStream pipeline — parse, compile, process, execute.
//!
//! Run with: cargo run -p matterstream --example full-pipeline

use matterstream::{
    Compiler, MatterStream, OpsHeader, Parser, Processor,
    PackageRegistry, CoreUiPackage,
    RsiPointer, BankId,
};

fn main() {
    let source = r##"
import { Slab } from '@mtsm/ui/core';

<>
  <Slab x={0.0} y={0.6} color="#CCCCCCFF" />
  <Slab x={0.0} y={0.2} color="#444444FF" />
  <Slab x={0.0} y={-0.2} color="#444444FF" />
  <Slab x={0.0} y={-0.6} color="#007BFFFF" />
</>
"##;

    println!("=== MatterStream Full Pipeline Example ===\n");

    // --- Stage 1: Parse ---
    println!("[1/4] Parsing TSX...");
    let parsed = Parser::parse(source).expect("parse failed");
    println!("       {} root element(s)", parsed.root_fragment.elements.len());

    // --- Stage 2: Compile ---
    println!("[2/4] Compiling to ops...");
    let compiled = Compiler::compile(source).expect("compile failed");
    println!("       {} ops generated", compiled.ops.len());

    // --- Stage 3: Process ---
    println!("[3/4] Processing with package registry...");
    let mut registry = PackageRegistry::new();
    registry.register_package(CoreUiPackage);
    let processor = Processor::new();
    let output = processor.process(compiled, &registry).expect("process failed");
    println!("       {} ops after processing", output.ops.ops.len());

    // --- Stage 4: Execute ---
    println!("[4/4] Executing on MatterStream...\n");
    let header = OpsHeader::new(
        vec![RsiPointer::new(1, BankId::Vec3 as u8, 0)],
        false,
    );
    let mut stream = MatterStream::new();
    smol::block_on(async {
        stream.execute(&header, &output.ops.ops).await.expect("execute failed");
    });

    // --- Results ---
    println!("--- Pipeline Results ---\n");
    println!("Draws: {}", stream.draws.len());
    for (i, draw) in stream.draws.iter().enumerate() {
        let r = (draw.color[0] * 255.0) as u8;
        let g = (draw.color[1] * 255.0) as u8;
        let b = (draw.color[2] * 255.0) as u8;
        let a = (draw.color[3] * 255.0) as u8;
        println!(
            "  [{}] pos=({:>5.2}, {:>5.2})  color=#{:02X}{:02X}{:02X}{:02X}  fast_path={}",
            i,
            draw.position[0], draw.position[1],
            r, g, b, a,
            draw.used_fast_path,
        );
    }
}
