//! TSX → RPN bytecode compiler.
//!
//! Converts MatterStream `Op`s (from the TSX compiler) into RPN bytecode
//! suitable for execution on the VM.

use matterstream_compiler::Compiler;
use matterstream_core::ops::{CompiledOps, Op, Primitive};

use crate::rpn::RpnOp;
use crate::ui_vm::rgba;

/// Compile TSX source to RPN bytecode (static layout only).
pub fn compile_to_bytecode(source: &str) -> Result<Vec<u8>, String> {
    let compiled = Compiler::compile(source).map_err(|e| format!("TSX compile error: {}", e))?;
    ops_to_bytecode(&compiled)
}

/// Convert compiled ops to RPN bytecode.
pub fn ops_to_bytecode(compiled: &CompiledOps) -> Result<Vec<u8>, String> {
    let mut bc = Vec::new();

    // Track current size for Draw ops
    let mut current_size: [f32; 2] = [100.0, 100.0];

    for op in &compiled.ops {
        match op {
            Op::SetColor(color) => {
                let r = (color[0] * 255.0) as u8;
                let g = (color[1] * 255.0) as u8;
                let b = (color[2] * 255.0) as u8;
                let a = (color[3] * 255.0) as u8;
                let packed = rgba(r, g, b, a);
                bc.push(RpnOp::Push32 as u8);
                bc.extend_from_slice(&packed.to_le_bytes());
                bc.push(RpnOp::UiSetColor as u8);
            }
            Op::SetTrans(trans) => {
                let dx = trans[0] as u32;
                let dy = trans[1] as u32;
                bc.push(RpnOp::Push32 as u8);
                bc.extend_from_slice(&dx.to_le_bytes());
                bc.push(RpnOp::Push32 as u8);
                bc.extend_from_slice(&dy.to_le_bytes());
                bc.push(RpnOp::UiSetOffset as u8);
            }
            Op::SetSize(size) => {
                current_size = *size;
            }
            Op::SetLabel(_label) => {
                // Label stored for text rendering; placeholder for now
            }
            Op::Draw {
                primitive,
                position_rsi: _,
            } => {
                let w = current_size[0] as u32;
                let h = current_size[1] as u32;
                match primitive {
                    Primitive::Slab => {
                        // x=0, y=0 (offset handles position), then w, h, radius
                        bc.push(RpnOp::Push32 as u8);
                        bc.extend_from_slice(&0u32.to_le_bytes());
                        bc.push(RpnOp::Push32 as u8);
                        bc.extend_from_slice(&0u32.to_le_bytes());
                        bc.push(RpnOp::Push32 as u8);
                        bc.extend_from_slice(&w.to_le_bytes());
                        bc.push(RpnOp::Push32 as u8);
                        bc.extend_from_slice(&h.to_le_bytes());
                        bc.push(RpnOp::Push32 as u8);
                        bc.extend_from_slice(&8u32.to_le_bytes()); // default radius
                        bc.push(RpnOp::UiSlab as u8);
                    }
                    Primitive::Text => {
                        bc.push(RpnOp::Push32 as u8);
                        bc.extend_from_slice(&0u32.to_le_bytes());
                        bc.push(RpnOp::Push32 as u8);
                        bc.extend_from_slice(&0u32.to_le_bytes());
                        let font_size = h.min(24);
                        bc.push(RpnOp::Push32 as u8);
                        bc.extend_from_slice(&font_size.to_le_bytes());
                        bc.push(RpnOp::Push32 as u8);
                        bc.extend_from_slice(&0u32.to_le_bytes()); // slot 0
                        bc.push(RpnOp::UiText as u8);
                    }
                }
            }
            Op::PushState => {
                bc.push(RpnOp::UiPushState as u8);
            }
            Op::PopState => {
                bc.push(RpnOp::UiPopState as u8);
            }
            _ => {
                // SetMatrix, PushProj, PopProj, BindZeroPage, BindResource,
                // SetPadding, SetTextColor, Push — skip for static layout
            }
        }
    }

    bc.push(RpnOp::Halt as u8);
    Ok(bc)
}
