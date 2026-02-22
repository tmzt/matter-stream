//! Example: Build MatterStream ops by hand using StreamBuilder and execute them.
//!
//! Run with: cargo run -p matterstream --example execute-ops

use matterstream::{
    MatterStream, OpsHeader, Primitive, RsiPointer, BankId, StreamBuilder,
};

fn main() {
    println!("=== MatterStream Execute Ops Example ===\n");

    // Build an op sequence using the StreamBuilder API.
    // This creates 3 colored slabs at different positions — no TSX parsing needed.
    let ops = StreamBuilder::new()
        // Red slab at (0.2, -0.3)
        .set_trans([0.2, -0.3, 0.0])
        .draw(Primitive::Slab, 0)
        // Green slab at (-0.5, 0.5) — push/pop state to isolate color
        .push_state()
        .set_trans([-0.5, 0.5, 0.0])
        .draw(Primitive::Slab, 0)
        .pop_state()
        // Blue slab at (0.8, 0.8)
        .set_trans([0.8, 0.8, 0.0])
        .draw(Primitive::Slab, 0)
        .build();

    println!("Built {} ops via StreamBuilder:\n", ops.len());
    for (i, op) in ops.iter().enumerate() {
        println!("  [{:>2}] {:?}", i, op);
    }
    println!();

    // Create an RSI pointer that maps position_rsi=0 to Vec3 bank register 0.
    let header = OpsHeader::new(
        vec![RsiPointer::new(1, BankId::Vec3 as u8, 0)],
        false,
    );

    // Execute the ops on a MatterStream instance.
    let mut stream = MatterStream::new();
    smol::block_on(async {
        match stream.execute(&header, &ops).await {
            Ok(()) => {
                println!("--- Draw Results ({} draws) ---\n", stream.draws.len());
                for (i, draw) in stream.draws.iter().enumerate() {
                    println!("  Draw #{}", i);
                    println!("    Position:   [{:.2}, {:.2}, {:.2}]", draw.position[0], draw.position[1], draw.position[2]);
                    println!("    Color:      [{:.2}, {:.2}, {:.2}, {:.2}]", draw.color[0], draw.color[1], draw.color[2], draw.color[3]);
                    println!("    Fast path:  {}", draw.used_fast_path);
                    println!("    Xform bytes: {}", draw.transform_bytes);
                    println!();
                }
            }
            Err(errors) => {
                eprintln!("Execution errors:");
                for e in errors {
                    eprintln!("  {:?}", e);
                }
            }
        }
    });
}
