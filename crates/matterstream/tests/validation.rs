//! Architectural validation tests A–D from AGENTS.md, using smol as the async reactor.

use matterstream::*;
use matterstream::ops::{RsiPointer, Primitive};
use matterstream::tier1::BankId;

/// Test A: The "6502 Efficiency" Check
///
/// A DRAW_SLAB must resolve `position` in O(1).
/// Instruction uses direct register indices.
#[test]
fn test_a_6502_efficiency() {
    smol::block_on(async {
        let mut stream = MatterStream::new();

        // Write a known position into Vec3 bank, register 0
        let expected_pos = [10.0, 20.0, 30.0];
        stream.registers.vec3.write(0, expected_pos);

        // RSI points directly to Tier 1, Vec3 bank (2), register 0
        let header = OpsHeader::new(
            vec![RsiPointer::new(1, BankId::Vec3 as u8, 0)],
            false,
        );

        stream
            .execute(
                &header,
                &[Op::Draw {
                    primitive: Primitive::Slab,
                    position_rsi: 0,
                }],
            )
            .await
            .unwrap();

        assert_eq!(stream.draws.len(), 1);
        let result = &stream.draws[0];

        // Position resolved via direct register index — O(1)
        assert_eq!(result.position, expected_pos);
    });
}

/// Test B: The "State Leaking" Check
///
/// PUSH_STATE -> POP_STATE must leave registers identical to the pre-push state.
#[test]
fn test_b_state_leaking() {
    smol::block_on(async {
        let mut stream = MatterStream::new();

        // Set up known register state across all banks
        stream.registers.mat4.write(0, [
            2.0, 0.0, 0.0, 0.0,
            0.0, 2.0, 0.0, 0.0,
            0.0, 0.0, 2.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);
        stream.registers.vec4.write(0, [1.0, 0.5, 0.25, 1.0]);
        stream.registers.vec3.write(0, [100.0, 200.0, 300.0]);
        stream.registers.scalar.write(0, 42.0);
        stream.registers.int.write(0, 7);

        // Snapshot expected values
        let expected_mat4 = *stream.registers.mat4.read(0);
        let expected_vec4 = *stream.registers.vec4.read(0);
        let expected_vec3 = *stream.registers.vec3.read(0);
        let expected_scalar = stream.registers.scalar.read(0);
        let expected_int = stream.registers.int.read(0);

        let header = OpsHeader::new(vec![], false);

        // Execute: push, mutate, pop
        stream.execute(&header, &[
            Op::PushState,
            // Mutate everything
            Op::SetMatrix([
                9.0, 0.0, 0.0, 0.0,
                0.0, 9.0, 0.0, 0.0,
                0.0, 0.0, 9.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            Op::SetTrans([999.0, 888.0, 777.0]),
            Op::PopState,
        ]).await.unwrap();

        // All registers must be identical to pre-push state
        assert_eq!(*stream.registers.mat4.read(0), expected_mat4);
        assert_eq!(*stream.registers.vec4.read(0), expected_vec4);
        assert_eq!(*stream.registers.vec3.read(0), expected_vec3);
        assert_eq!(stream.registers.scalar.read(0), expected_scalar);
        assert_eq!(stream.registers.int.read(0), expected_int);
    });
}

/// Test C: The "Matrix Churn" Test
///
/// PUSH_PROJ must not trigger a re-upload of VEC4 (Color) or SCL (Scalar) banks.
/// Only MAT4 should be dirty after POP_PROJ.
#[test]
fn test_c_matrix_churn() {
    smol::block_on(async {
        let mut stream = MatterStream::new();

        // Set up state
        stream.registers.mat4.write(0, [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);
        stream.registers.vec4.write(0, [1.0, 0.0, 0.0, 1.0]);
        stream.registers.scalar.write(0, 1.0);

        let header = OpsHeader::new(vec![], false);

        // Execute: push proj, mutate mat4, pop proj
        stream.execute(&header, &[
            Op::PushProj,
            Op::SetMatrix([
                5.0, 0.0, 0.0, 0.0,
                0.0, 5.0, 0.0, 0.0,
                0.0, 0.0, 5.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ]),
            Op::PopProj,
        ]).await.unwrap();

        // Only MAT4 should be dirty
        assert!(stream.registers.dirty.is_dirty(BankId::Mat4),
            "MAT4 must be dirty after POP_PROJ");
        assert!(!stream.registers.dirty.is_dirty(BankId::Vec4),
            "VEC4 must NOT be dirty after POP_PROJ");
        assert!(!stream.registers.dirty.is_dirty(BankId::Scalar),
            "Scalar must NOT be dirty after POP_PROJ");
        assert!(!stream.registers.dirty.is_dirty(BankId::Vec3),
            "VEC3 must NOT be dirty after POP_PROJ");
        assert!(!stream.registers.dirty.is_dirty(BankId::Int),
            "INT must NOT be dirty after POP_PROJ");
    });
}

/// Test D: The "Translation Fast-Path" Test
///
/// A pure translation update must be 12 bytes (vec3) instead of 64 bytes (mat4).
#[test]
fn test_d_translation_fast_path() {
    smol::block_on(async {
        let mut stream = MatterStream::new();

        let pos = [5.0, 10.0, 15.0];
        stream.registers.vec3.write(0, pos);

        // Header with translation_only = true (compiler emitted SET_TRANS)
        let header_fast = OpsHeader::new(
            vec![RsiPointer::new(1, BankId::Vec3 as u8, 0)],
            true,  // translation-only fast path
        );

        stream
            .execute(
                &header_fast,
                &[Op::Draw {
                    primitive: Primitive::Slab,
                    position_rsi: 0,
                }],
            )
            .await
            .unwrap();

        let fast_result = &stream.draws[0];
        assert!(
            fast_result.used_fast_path,
            "must use fast path when translation_only"
        );
        assert_eq!(
            fast_result.transform_bytes, 12,
            "fast path must be 12 bytes (vec3)"
        );

        // Now test without fast path
        let header_full = OpsHeader::new(
            vec![RsiPointer::new(1, BankId::Vec3 as u8, 0)],
            false, // full matrix path
        );

        stream
            .execute(
                &header_full,
                &[Op::Draw {
                    primitive: Primitive::Slab,
                    position_rsi: 0,
                }],
            )
            .await
            .unwrap();

        let full_result = &stream.draws[0];
        assert!(
            !full_result.used_fast_path,
            "must NOT use fast path when translation_only is false"
        );
        assert_eq!(
            full_result.transform_bytes, 64,
            "full path must be 64 bytes (mat4)"
        );
    });
}
