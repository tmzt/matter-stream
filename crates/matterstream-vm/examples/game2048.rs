//! 2048 — tile-sliding puzzle game using the MTSM RPN VM with softbuffer rendering.
//!
//! State (hooks):
//! - board: 4×4 grid in ZeroPage (16 cells, tile values: 0, 2, 4, 8, ..., 2048)
//! - score: current score in Int bank
//! - game_over: 0=playing, 1=game over in Int bank

use matterstream_vm::hooks::HookContext;
use matterstream_vm::host::VmHost;
use matterstream_vm::rpn::SimpleRng;
use matterstream_vm::ui_vm;

const GRID: u32 = 4;
const CELL_SIZE: u32 = 100;
const GAP: u32 = 10;
const BOARD_X: u32 = 30;
const BOARD_Y: u32 = 80;
const WIDTH: u32 = GRID * CELL_SIZE + (GRID + 1) * GAP + BOARD_X * 2;
const HEIGHT: u32 = GRID * CELL_SIZE + (GRID + 1) * GAP + BOARD_Y + 50;

fn tile_color(val: i32) -> u32 {
    match val {
        2 => ui_vm::rgba(238, 228, 218, 255),
        4 => ui_vm::rgba(237, 224, 200, 255),
        8 => ui_vm::rgba(242, 177, 121, 255),
        16 => ui_vm::rgba(245, 149, 99, 255),
        32 => ui_vm::rgba(246, 124, 95, 255),
        64 => ui_vm::rgba(246, 94, 59, 255),
        128 => ui_vm::rgba(237, 207, 114, 255),
        256 => ui_vm::rgba(237, 204, 97, 255),
        512 => ui_vm::rgba(237, 200, 80, 255),
        1024 => ui_vm::rgba(237, 197, 63, 255),
        2048 => ui_vm::rgba(237, 194, 46, 255),
        _ => ui_vm::rgba(205, 193, 180, 255),
    }
}

fn main() {
    let mut hooks = HookContext::new();
    let board = hooks.use_state_grid(GRID * GRID, 0);
    let score = hooks.use_state_i32(0);
    let _game_over = hooks.use_state_i32(0);

    let logic_bytecode = Vec::new();
    let render_bytecode = Vec::new();
    let mut host = VmHost::new(logic_bytecode, render_bytecode, hooks);

    let mut rng = SimpleRng::new(42);

    // Place two initial tiles
    spawn_tile(&mut host.vm.zero_page, board.index, &mut rng);
    spawn_tile(&mut host.vm.zero_page, board.index, &mut rng);

    let timeout_frames: Option<u64> = std::env::args()
        .position(|a| a == "--timeout")
        .and_then(|i| std::env::args().nth(i + 1))
        .and_then(|s| s.parse::<u64>().ok())
        .map(|secs| secs * 60);

    println!("2048 — Use arrow keys to slide tiles. Merge equal tiles to reach 2048!");

    let mut buf = vec![0u32; (WIDTH * HEIGHT) as usize];
    let mut frame = 0u64;

    // Simulate some moves for the demo
    let moves = [0u8 /*right*/, 1 /*down*/, 0, 2 /*left*/, 1, 0, 3 /*up*/, 1];

    loop {
        frame += 1;
        if let Some(max) = timeout_frames {
            if frame > max {
                println!("Timeout reached after {} frames", frame);
                break;
            }
        }

        // Apply a move every 30 frames
        if frame <= (moves.len() as u64) * 30 && frame % 30 == 1 {
            let move_idx = (frame / 30) as usize;
            if move_idx < moves.len() {
                let changed = slide(&mut host.vm.zero_page, board.index, moves[move_idx], score.index, &mut host.vm.int_bank);
                if changed {
                    spawn_tile(&mut host.vm.zero_page, board.index, &mut rng);
                }
            }
        }

        // Clear buffer
        for p in buf.iter_mut() {
            *p = 0xFAF8EF; // 2048 beige background
        }

        // Draw board background
        let board_w = GRID * CELL_SIZE + (GRID + 1) * GAP;
        let board_h = board_w;
        ui_vm::draw_rounded_rect(
            &mut buf, WIDTH, HEIGHT,
            BOARD_X as i32, BOARD_Y as i32,
            board_w, board_h, 8,
            ui_vm::rgba(187, 173, 160, 255),
        );

        // Draw tiles
        for row in 0..GRID {
            for col in 0..GRID {
                let cell_idx = (row * GRID + col) as usize;
                let zp_offset = board.index as usize + cell_idx * 4;
                let val = read_zp_i32(&host.vm.zero_page, zp_offset);

                let tx = BOARD_X + GAP + col * (CELL_SIZE + GAP);
                let ty = BOARD_Y + GAP + row * (CELL_SIZE + GAP);

                let color = tile_color(val);
                ui_vm::draw_rounded_rect(
                    &mut buf, WIDTH, HEIGHT,
                    tx as i32, ty as i32,
                    CELL_SIZE, CELL_SIZE, 6,
                    color,
                );
            }
        }

        // Draw score
        let _current_score = host.vm.int_bank[score.index as usize];
        // Score placeholder rect
        ui_vm::draw_rounded_rect(
            &mut buf, WIDTH, HEIGHT,
            BOARD_X as i32, 10,
            120, 50, 6,
            ui_vm::rgba(187, 173, 160, 255),
        );

        if frame > 300 {
            break;
        }
    }

    let final_score = host.vm.int_bank[score.index as usize];
    println!("2048 example completed. {} frames, score: {}", frame, final_score);
}

fn read_zp_i32(zp: &[u8; 256], offset: usize) -> i32 {
    if offset + 3 < 256 {
        i32::from_le_bytes([zp[offset], zp[offset + 1], zp[offset + 2], zp[offset + 3]])
    } else {
        0
    }
}

fn write_zp_i32(zp: &mut [u8; 256], offset: usize, val: i32) {
    if offset + 3 < 256 {
        let bytes = val.to_le_bytes();
        zp[offset] = bytes[0];
        zp[offset + 1] = bytes[1];
        zp[offset + 2] = bytes[2];
        zp[offset + 3] = bytes[3];
    }
}

fn spawn_tile(zp: &mut [u8; 256], board_offset: u32, rng: &mut SimpleRng) {
    let mut empties = Vec::new();
    for i in 0..(GRID * GRID) {
        let offset = board_offset as usize + i as usize * 4;
        if read_zp_i32(zp, offset) == 0 {
            empties.push(i);
        }
    }
    if empties.is_empty() {
        return;
    }
    let idx = empties[rng.next_bounded(empties.len() as u32) as usize];
    let val = if rng.next_bounded(10) == 0 { 4 } else { 2 };
    write_zp_i32(zp, board_offset as usize + idx as usize * 4, val);
}

/// Slide tiles in direction: 0=right, 1=down, 2=left, 3=up.
/// Returns true if the board changed.
fn slide(zp: &mut [u8; 256], board_offset: u32, dir: u8, score_idx: u32, int_bank: &mut [i32; 16]) -> bool {
    let mut changed = false;

    for line in 0..GRID {
        let mut vals = [0i32; 4];
        // Extract line
        for i in 0..GRID {
            let (row, col) = match dir {
                0 => (line, i),           // right
                1 => (i, line),           // down
                2 => (line, GRID - 1 - i), // left
                3 => (GRID - 1 - i, line), // up
                _ => unreachable!(),
            };
            let offset = board_offset as usize + (row * GRID + col) as usize * 4;
            vals[i as usize] = read_zp_i32(zp, offset);
        }

        // Compact non-zeros
        let mut compacted = [0i32; 4];
        let mut ci = 0;
        for &v in &vals {
            if v != 0 {
                compacted[ci] = v;
                ci += 1;
            }
        }

        // Merge adjacent equals
        let mut merged = [0i32; 4];
        let mut mi = 0;
        let mut skip = false;
        for j in 0..ci {
            if skip {
                skip = false;
                continue;
            }
            if j + 1 < ci && compacted[j] == compacted[j + 1] {
                merged[mi] = compacted[j] * 2;
                int_bank[score_idx as usize] += compacted[j] * 2;
                mi += 1;
                skip = true;
            } else {
                merged[mi] = compacted[j];
                mi += 1;
            }
        }

        // Write back
        for i in 0..GRID {
            let (row, col) = match dir {
                0 => (line, i),
                1 => (i, line),
                2 => (line, GRID - 1 - i),
                3 => (GRID - 1 - i, line),
                _ => unreachable!(),
            };
            let offset = board_offset as usize + (row * GRID + col) as usize * 4;
            let new_val = merged[i as usize];
            if read_zp_i32(zp, offset) != new_val {
                changed = true;
            }
            write_zp_i32(zp, offset, new_val);
        }
    }

    changed
}
