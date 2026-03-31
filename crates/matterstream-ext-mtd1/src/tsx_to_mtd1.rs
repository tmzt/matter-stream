//! TSX-to-mtd1 compiler bridge.
//!
//! Walks a mock TSX AST and lowers it into mtd1 bytecode via `pretext_rs`
//! and `Command32` primitives.

use crate::mtd1_format::{BankedStyle, Command32, Mtd1Document};
use crate::pretext_rs::{FontMetrics, LayoutConfig, layout_table, layout_text, layout_text_with_token};

// ── Mock TSX AST ────────────────────────────────────────────────────────────

/// Simplified TSX node representation.
#[derive(Debug, Clone)]
pub enum TsxNode {
    /// `<TufteCard>` — container establishing cursor bounds.
    TufteCard {
        x: i16,
        y: i16,
        width: u32,
        children: Vec<TsxNode>,
    },
    /// `<Story>` — dense text paragraph.
    Story {
        text: String,
        /// Optional: (word_index, token_id) for interactive word
        token: Option<(usize, u32)>,
    },
    /// `<Spreadsheet>` — data table with column alignment.
    Spreadsheet {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        col_widths: Vec<u32>,
        zebra: bool,
    },
    /// `<Path>` — sparkline / shape primitives.
    Path {
        /// List of (height, width) shape segments
        segments: Vec<(u16, u16)>,
    },
}

// ── Compiler ────────────────────────────────────────────────────────────────

/// Compile a TSX node tree into an `Mtd1Document`.
pub fn compile_tsx(root: &[TsxNode], metrics: &FontMetrics) -> Mtd1Document {
    let mut doc = Mtd1Document::new();

    // Default style bank
    doc.styles.push(BankedStyle::new(0x1A1A2EFF, 0, 0, 0)); // 0: dark bg
    doc.styles.push(BankedStyle::new(0xE8E6DFFF, 0, 0, 0)); // 1: text fg
    doc.styles.push(BankedStyle::new(0xF5F0EBFF, 0, 0, 0)); // 2: zebra even
    doc.styles.push(BankedStyle::new(0xEDE8E0FF, 0, 0, 0)); // 3: zebra odd
    doc.styles.push(BankedStyle::new(0xC75233FF, 2, 0, 1)); // 4: sparkline stroke

    for node in root {
        compile_node(node, metrics, &mut doc, 0, 0, 600);
    }

    doc
}

fn compile_node(
    node: &TsxNode,
    metrics: &FontMetrics,
    doc: &mut Mtd1Document,
    parent_x: i16,
    parent_y: i16,
    parent_width: u32,
) {
    match node {
        TsxNode::TufteCard {
            x,
            y,
            width,
            children,
        } => {
            let abs_x = parent_x + x;
            let abs_y = parent_y + y;
            doc.instructions.push(Command32::set_cursor(abs_y, abs_x));

            for child in children {
                compile_node(child, metrics, doc, abs_x, abs_y, *width);
            }
        }

        TsxNode::Story { text, token } => {
            let config = LayoutConfig {
                max_width: parent_width,
                origin_x: parent_x,
                origin_y: parent_y,
            };

            doc.instructions.push(Command32::set_style(1)); // text style

            let cmds = if let Some((word_idx, token_id)) = token {
                layout_text_with_token(text.as_bytes(), metrics, &config, *word_idx, *token_id)
            } else {
                layout_text(text.as_bytes(), metrics, &config)
            };
            doc.instructions.extend(cmds);
        }

        TsxNode::Spreadsheet {
            headers,
            rows,
            col_widths,
            zebra,
        } => {
            let header_refs: Vec<&[u8]> = headers.iter().map(|s| s.as_bytes()).collect();
            let row_refs: Vec<Vec<&[u8]>> = rows
                .iter()
                .map(|r| r.iter().map(|s| s.as_bytes()).collect())
                .collect();

            let (z_even, z_odd) = if *zebra {
                (Some(2), Some(3))
            } else {
                (None, None)
            };

            let cmds = layout_table(
                &header_refs,
                &row_refs,
                col_widths,
                metrics,
                parent_x,
                parent_y,
                metrics.line_height,
                z_even,
                z_odd,
            );
            doc.instructions.extend(cmds);
        }

        TsxNode::Path { segments } => {
            doc.instructions.push(Command32::set_style(4)); // sparkline style
            for &(h, w) in segments {
                doc.instructions.push(Command32::draw_shape(h, w));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_tree() -> Vec<TsxNode> {
        vec![TsxNode::TufteCard {
            x: 20,
            y: 10,
            width: 500,
            children: vec![
                TsxNode::Story {
                    text: "The visual display of quantitative information.".into(),
                    token: Some((1, 1001)),
                },
                TsxNode::Spreadsheet {
                    headers: vec!["Year".into(), "Value".into()],
                    rows: vec![
                        vec!["2024".into(), "1200".into()],
                        vec!["2025".into(), "1580".into()],
                    ],
                    col_widths: vec![100, 100],
                    zebra: true,
                },
                TsxNode::Path {
                    segments: vec![(2, 40), (4, 40), (3, 40), (6, 40), (5, 40)],
                },
            ],
        }]
    }

    #[test]
    fn compile_produces_valid_document() {
        let fm = FontMetrics::monospace(8, 16);
        let tree = demo_tree();
        let doc = compile_tsx(&tree, &fm);

        assert!(!doc.styles.is_empty());
        assert!(!doc.instructions.is_empty());

        // Roundtrip through binary
        let bytes = doc.to_bytes();
        let parsed = Mtd1Document::from_bytes(&bytes).unwrap();
        assert_eq!(doc.instructions.len(), parsed.instructions.len());
    }

    #[test]
    fn debug_dump_readable() {
        let fm = FontMetrics::monospace(8, 16);
        let doc = compile_tsx(&demo_tree(), &fm);
        let dump = doc.debug_dump();
        assert!(dump.contains("SET_CURSOR"));
        assert!(dump.contains("DRAW_GLYPH"));
        assert!(dump.contains("SET_STYLE"));
    }
}
