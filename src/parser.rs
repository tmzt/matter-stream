//! A parser for a simple text-based representation of `Op` sequences.

use crate::ops::{Op, Primitive};
use crate::builder::StreamBuilder;

/// A parser for a simple text-based representation of `Op` sequences.
pub struct Parser;

impl Parser {
    /// Parses a string into a `Vec<Op>`.
    pub fn parse(input: &str) -> Result<Vec<Op>, String> {
        let mut builder = StreamBuilder::new();
        for line in input.lines() {
            let parts: Vec<&str> = line.trim().split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }
            let op = parts[0];
            match op {
                "draw" => {
                    if parts.len() != 3 {
                        return Err(format!("Invalid draw op: {}", line));
                    }
                    let primitive = match parts[1] {
                        "slab" => Primitive::Slab,
                        _ => return Err(format!("Invalid primitive: {}", parts[1])),
                    };
                    let position_rsi = parts[2].parse().map_err(|e| format!("Invalid position_rsi: {}", e))?;
                    builder = builder.draw(primitive, position_rsi);
                }
                "set_trans" => {
                    if parts.len() != 4 {
                        return Err(format!("Invalid set_trans op: {}", line));
                    }
                    let x = parts[1].parse().map_err(|e| format!("Invalid x: {}", e))?;
                    let y = parts[2].parse().map_err(|e| format!("Invalid y: {}", e))?;
                    let z = parts[3].parse().map_err(|e| format!("Invalid z: {}", e))?;
                    builder = builder.set_trans([x, y, z]);
                }
                "push" => {
                    if parts.len() < 2 {
                        return Err(format!("Invalid push op: {}", line));
                    }
                    let data: Result<Vec<u8>, _> = parts[1..].iter().map(|s| s.parse()).collect();
                    builder = builder.push(data.map_err(|e| format!("Invalid data: {}", e))?);
                }
                _ => return Err(format!("Unknown op: {}", op)),
            }
        }
        Ok(builder.build())
    }

    /// Parses a file into a `Vec<Op>`.
    pub fn parse_file(file_path: &str) -> Result<Vec<Op>, String> {
        let input = std::fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;
        Self::parse(&input)
    }
}
