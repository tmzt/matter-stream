//! Example: Compile TSX source into MatterStream ops.
//!
//! Run with: cargo run -p matterstream --example compile-tsx

use matterstream::Compiler;

fn main() {
    let source = r##"
<>
  <Slab x={0.2} y={-0.3} color="#FF0000FF" />
  <Slab x={-0.5} y={0.5} color="#00FF00FF" />
  <Slab x={0.8} y={0.8} color="#0000FFFF" />
</>
"##;

    println!("=== MatterStream Compiler Example ===\n");
    println!("Input TSX:\n{}", source.trim());
    println!();

    let compiled = match Compiler::compile(source) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Compile error: {}", e);
            return;
        }
    };

    println!("--- Compiled Ops ---");
    println!("Header:");
    println!("  RSI pointers: {:?}", compiled.header.rsi_pointers);
    println!("  Translation-only: {}", compiled.header.translation_only);
    println!();

    println!("Op stream ({} ops):", compiled.ops.len());
    for (i, op) in compiled.ops.iter().enumerate() {
        println!("  [{:>2}] {:?}", i, op);
    }
}
