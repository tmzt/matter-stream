//! CPU compute stage for MatterStream render pipeline.
//!
//! Takes raw VM output (SdfDrawCmd[], banks, string_table) and produces
//! a `RenderFrame` ready for any render backend (GPU or CPU).
//!
//! This is the CPU implementation of pipeline stage 3 (COMPUTE).
//! A GPU compute shader could replace this with identical output.

use matterstream_common::{
    RenderFrame, SdfDrawCmd, Anim, GpuFont, GpuTexture,
    pack_bitmap, pack_strings, DRAW_TYPE_TEXT,
};

/// Prepare a `RenderFrame` from VM output.
///
/// Packs strings into char_buffer, encodes offsets into text SdfDrawCmd params[3],
/// and packs the font bitmap — everything the render stage needs.
pub fn prepare_frame(
    draws: &[SdfDrawCmd],
    string_table: &[String],
    font: &GpuFont,
    font_bitmap: &[u8],
    anim_bank: &[Anim],
    scalar_bank: &[f32; 16],
    int_bank: &[i32; 16],
    time_ms: f32,
    width: u32,
    height: u32,
    scale: f32,
) -> RenderFrame {
    let (char_buffer, str_offsets) = pack_strings(string_table);

    let mut draws = draws.to_vec();
    for cmd in &mut draws {
        if cmd.params[0] as u32 == DRAW_TYPE_TEXT as u32 {
            let str_idx = cmd.params[3] as u32 as usize;
            if str_idx < str_offsets.len() {
                let so = &str_offsets[str_idx];
                cmd.params[3] = f32::from_bits((so.start << 16) | so.len);
            }
        }
    }

    // Run ribbon scroll physics before building the frame
    let mut scalar_bank = *scalar_bank;
    update_ribbon_physics(&mut scalar_bank, time_ms);

    RenderFrame {
        draws,
        char_buffer,
        anim_bank: anim_bank.to_vec(),
        texture_bank: Vec::new(),
        font: *font,
        glyph_bitmap: pack_bitmap(font_bitmap),
        scalar_bank,
        int_bank: *int_bank,
        time_ms,
        width,
        height,
        scale,
    }
}

// ── Ribbon scroll physics ───────────────────────────────────────────────

/// Physics states for ribbon scroll.
const PHYSICS_IDLE: f32 = 0.0;
const PHYSICS_DRAGGING: f32 = 1.0;
const PHYSICS_DECELERATING: f32 = 2.0;
const PHYSICS_SNAPPING: f32 = 3.0;

/// Friction factor per millisecond (velocity multiplier).
const FRICTION: f32 = 0.995;
/// Velocity threshold (px/ms) below which we snap.
const SNAP_VELOCITY_THRESHOLD: f32 = 0.05;
/// Spring constant for snap animation.
const SNAP_SPRING_K: f32 = 0.008;
/// Distance threshold (px) to consider snapped.
const SNAP_DISTANCE_THRESHOLD: f32 = 0.5;

/// Update ribbon scroll physics for all ribbons in scalar_bank.
///
/// Each ribbon uses 4 consecutive slots starting at N:
///   [N+0] = scroll_position (px offset)
///   [N+1] = scroll_velocity (px/ms)
///   [N+2] = snap_target (px)
///   [N+3] = physics_state (0=idle, 1=dragging, 2=decelerating, 3=snapping)
///
/// The event loop is responsible for setting state=1 during drag and
/// state=2 with velocity on release.
pub fn update_ribbon_physics(scalar_bank: &mut [f32; 16], _time_ms: f32) {
    // Scan for active ribbon physics (slots 0..16 in groups of 4)
    let mut slot = 0;
    while slot + 3 < 16 {
        let state = scalar_bank[slot + 3];
        if state == PHYSICS_DRAGGING {
            // Position set directly by event loop, nothing to do
        } else if state == PHYSICS_DECELERATING {
            let vel = scalar_bank[slot + 1];
            let pos = scalar_bank[slot + 0];

            // Apply velocity
            scalar_bank[slot + 0] = pos + vel;
            // Apply friction
            scalar_bank[slot + 1] = vel * FRICTION;

            // Check if velocity dropped below threshold
            if vel.abs() < SNAP_VELOCITY_THRESHOLD {
                // Snap target should already be set by event loop
                scalar_bank[slot + 3] = PHYSICS_SNAPPING;
            }
        } else if state == PHYSICS_SNAPPING {
            let pos = scalar_bank[slot + 0];
            let target = scalar_bank[slot + 2];
            let delta = target - pos;

            if delta.abs() < SNAP_DISTANCE_THRESHOLD {
                scalar_bank[slot + 0] = target;
                scalar_bank[slot + 3] = PHYSICS_IDLE;
            } else {
                // Exponential ease toward target
                scalar_bank[slot + 0] = pos + delta * SNAP_SPRING_K;
            }
        }
        slot += 4;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matterstream_common::{DRAW_TYPE_BOX, DRAW_TYPE_TEXT};

    #[test]
    fn prepare_frame_no_text() {
        let draws = vec![SdfDrawCmd {
            pos: [10.0, 20.0],
            size: [100.0, 50.0],
            color: [1.0, 0.0, 0.0, 1.0],
            params: [DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
        }];
        let frame = prepare_frame(
            &draws, &[], &GpuFont::NONE, &[],
            &[], &[0.0; 16], &[0; 16], 0.0, 800, 600, 1.0,
        );
        assert_eq!(frame.draws.len(), 1);
        assert_eq!(frame.char_buffer.len(), 0);
        assert_eq!(frame.width, 800);
        assert_eq!(frame.height, 600);
    }

    #[test]
    fn prepare_frame_packs_strings() {
        let draws = vec![SdfDrawCmd {
            pos: [0.0, 0.0],
            size: [100.0, 16.0],
            color: [1.0, 1.0, 1.0, 1.0],
            params: [DRAW_TYPE_TEXT, 0.0, 0.0, 0.0], // str_idx = 0
        }];
        let strings = vec!["Hello".to_string()];
        let frame = prepare_frame(
            &draws, &strings, &GpuFont::NONE, &[],
            &[], &[0.0; 16], &[0; 16], 0.0, 800, 600, 1.0,
        );
        assert_eq!(frame.char_buffer.len(), 5);
        assert_eq!(frame.char_buffer[0], b'H' as u32);
        let packed = f32::to_bits(frame.draws[0].params[3]);
        assert_eq!(packed >> 16, 0);
        assert_eq!(packed & 0xFFFF, 5);
    }

    #[test]
    fn prepare_frame_multiple_strings() {
        let draws = vec![
            SdfDrawCmd {
                pos: [0.0, 0.0], size: [100.0, 16.0],
                color: [1.0, 1.0, 1.0, 1.0],
                params: [DRAW_TYPE_TEXT, 0.0, 0.0, 0.0],
            },
            SdfDrawCmd {
                pos: [0.0, 20.0], size: [100.0, 16.0],
                color: [1.0, 1.0, 1.0, 1.0],
                params: [DRAW_TYPE_TEXT, 0.0, 0.0, 1.0],
            },
        ];
        let strings = vec!["AB".to_string(), "CDE".to_string()];
        let frame = prepare_frame(
            &draws, &strings, &GpuFont::NONE, &[],
            &[], &[0.0; 16], &[0; 16], 0.0, 800, 600, 1.0,
        );
        assert_eq!(frame.char_buffer.len(), 5);
        let p0 = f32::to_bits(frame.draws[0].params[3]);
        assert_eq!(p0 >> 16, 0);
        assert_eq!(p0 & 0xFFFF, 2);
        let p1 = f32::to_bits(frame.draws[1].params[3]);
        assert_eq!(p1 >> 16, 2);
        assert_eq!(p1 & 0xFFFF, 3);
    }

    #[test]
    fn prepare_frame_packs_font_bitmap() {
        let bitmap = vec![0xFFu8, 0x81, 0xFF];
        let frame = prepare_frame(
            &[], &[], &GpuFont::NONE, &bitmap,
            &[], &[0.0; 16], &[0; 16], 0.0, 800, 600, 1.0,
        );
        assert_eq!(frame.glyph_bitmap, vec![0xFF, 0x81, 0xFF]);
    }

    #[test]
    fn prepare_frame_preserves_banks() {
        let mut scalar = [0.0f32; 16];
        scalar[0] = 42.0;
        let mut int = [0i32; 16];
        int[3] = -7;
        let frame = prepare_frame(
            &[], &[], &GpuFont::NONE, &[],
            &[], &scalar, &int, 123.0, 400, 300, 2.0,
        );
        assert_eq!(frame.scalar_bank[0], 42.0);
        assert_eq!(frame.int_bank[3], -7);
        assert_eq!(frame.time_ms, 123.0);
        assert_eq!(frame.scale, 2.0);
    }

    #[test]
    fn physics_idle_does_nothing() {
        let mut bank = [0.0f32; 16];
        bank[0] = 100.0; // position
        bank[3] = PHYSICS_IDLE;
        update_ribbon_physics(&mut bank, 16.0);
        assert_eq!(bank[0], 100.0);
    }

    #[test]
    fn physics_deceleration() {
        let mut bank = [0.0f32; 16];
        bank[0] = 0.0;   // position
        bank[1] = 1.0;   // velocity (1 px/frame)
        bank[3] = PHYSICS_DECELERATING;
        update_ribbon_physics(&mut bank, 16.0);
        assert!(bank[0] > 0.0); // moved
        assert!(bank[1] < 1.0); // slowed
        assert!(bank[1] > 0.0); // still moving
    }

    #[test]
    fn physics_snap_converges() {
        let mut bank = [0.0f32; 16];
        bank[0] = 10.0;  // position
        bank[2] = 100.0; // snap target
        bank[3] = PHYSICS_SNAPPING;
        // Run many iterations
        for _ in 0..1000 {
            update_ribbon_physics(&mut bank, 16.0);
        }
        assert_eq!(bank[0], 100.0); // converged
        assert_eq!(bank[3], PHYSICS_IDLE); // settled
    }
}
