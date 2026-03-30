//! Example: Parse TSX source into a MatterStream AST.
//!
//! Run with: cargo run -p matterstream --example parse-tsx

use matterstream::Parser;

fn main() {
    let source = r##"
import { Slab } from '@mtsm/ui/core';

<>
  <Slab x={0.2} y={-0.3} color="#FF0000FF" />
  <Slab x={-0.5} y={0.5} color="#00FF00FF" />
  <Slab x={0.8} y={0.8} color="#0000FFFF" />
</>
"##;

    println!("=== MatterStream Parser Example ===\n");
    println!("Input TSX:\n{}", source.trim());
    println!();

    let parsed = match Parser::parse(source) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            return;
        }
    };

    println!("--- Parsed AST ---");
    println!("Root fragment has {} element(s):\n", parsed.root_fragment.elements.len());

    for (i, element) in parsed.root_fragment.elements.iter().enumerate() {
        println!("  Element #{} (id={})", i, element.id);
        println!("    Kind: {:?}", element.kind);

        // Print attributes
        for entry in element.attributes.attributes.iter() {
            println!("    Attr '{}' = {:?}", entry.key(), entry.value());
        }

        // Print children count
        if let Some(children) = &element.children {
            println!("    Children: {} element(s)", children.elements.len());
        } else {
            println!("    Children: none (self-closing)");
        }
        println!();
    }

    println!("--- MtsmObject Data ---");
    println!("  Entries: {}", parsed.mtsm_data.data.len());
    for entry in parsed.mtsm_data.data.iter() {
        println!("    '{}'", entry.key());
    }
}
