//! mtd1_render — Compile a Tufte-style TSX document into mtd1 bytecode,
//! write the binary, and print a debug disassembly.

use matterstream_mtd1::pretext_rs::FontMetrics;
use matterstream_mtd1::tsx_to_mtd1::{TsxNode, compile_tsx};

fn build_tufte_demo() -> Vec<TsxNode> {
    vec![TsxNode::TufteCard {
        x: 20,
        y: 10,
        width: 600,
        children: vec![
            // Dense paragraph with interactive token on "visual"
            TsxNode::Story {
                text: concat!(
                    "The visual display of quantitative information demands that we give ",
                    "the viewer the greatest number of ideas in the shortest time with ",
                    "the least ink in the smallest space. Data graphics should draw ",
                    "attention to the substance rather than to methodology, graphic ",
                    "design, or technology of graphic production."
                )
                .into(),
                token: Some((1, 1001)), // "visual" = word index 1
            },
            // Revenue spreadsheet with zebra striping
            TsxNode::Spreadsheet {
                headers: vec![
                    "Quarter".into(),
                    "Revenue".into(),
                    "Growth".into(),
                    "Margin".into(),
                ],
                rows: vec![
                    vec!["Q1 2024".into(), "$12.4M".into(), "+8.2%".into(), "34.1%".into()],
                    vec!["Q2 2024".into(), "$13.1M".into(), "+5.6%".into(), "35.8%".into()],
                    vec!["Q3 2024".into(), "$14.8M".into(), "+13.0%".into(), "36.2%".into()],
                    vec!["Q4 2024".into(), "$15.2M".into(), "+2.7%".into(), "37.0%".into()],
                    vec!["Q1 2025".into(), "$16.9M".into(), "+11.2%".into(), "38.4%".into()],
                    vec!["Q2 2025".into(), "$18.3M".into(), "+8.3%".into(), "39.1%".into()],
                ],
                col_widths: vec![120, 100, 100, 120],
                zebra: true,
            },
            // Revenue trend sparkline
            TsxNode::Path {
                segments: vec![
                    (2, 30),
                    (4, 30),
                    (3, 30),
                    (7, 30),
                    (5, 30),
                    (8, 30),
                    (6, 30),
                    (10, 30),
                    (9, 30),
                    (12, 30),
                    (11, 30),
                    (14, 30),
                ],
            },
        ],
    }]
}

fn main() {
    println!("=== mtd1 Renderer ===\n");

    // Initialize font metrics (8px monospace)
    let metrics = FontMetrics::monospace(8, 16);

    // Build the TSX tree (mirrors tufte_demo.tsx)
    let tree = build_tufte_demo();

    // Compile to mtd1
    let doc = compile_tsx(&tree, &metrics);

    // Serialize to binary
    let bytes = doc.to_bytes();

    // Write to file
    let out_path = "tufte_demo.mtd1";
    std::fs::write(out_path, &bytes).expect("failed to write .mtd1 file");

    // Read back and verify
    let read_back = std::fs::read(out_path).expect("failed to read .mtd1 file");
    let parsed =
        matterstream_mtd1::Mtd1Document::from_bytes(&read_back).expect("failed to parse .mtd1");

    // Print debug dump
    println!("{}", parsed.debug_dump());

    // Source TSX size estimate (the fixture text)
    let tsx_source_size = 1200; // approximate bytes of the TSX fixture
    println!("--- Efficiency ---");
    println!("Source TSX:    ~{} bytes", tsx_source_size);
    println!("Compiled mtd1:  {} bytes", bytes.len());
    println!(
        "Compression:    {:.1}x",
        tsx_source_size as f64 / bytes.len() as f64
    );
    println!("Instructions:   {}", parsed.instructions.len());
    println!("Styles:         {}", parsed.styles.len());
    println!(
        "Bytes/instr:    {:.1}",
        bytes.len() as f64 / parsed.instructions.len() as f64
    );

    // Cleanup
    let _ = std::fs::remove_file(out_path);

    println!("\nDone.");
}
