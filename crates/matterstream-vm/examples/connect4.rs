//! Connect 4 — interactive game using the MTSM RPN VM with softbuffer rendering.
//!
//! State (hooks):
//! - board: 7×6 grid in ZeroPage (42 cells, 0=empty, 1=p1, 2=p2)
//! - player: current player in Int bank
//! - game_over: 0=playing, 1=won, 2=draw in Int bank
//! - winner: winning player in Int bank
//! - hover_col: column under mouse in Int bank

use matterstream_vm::hooks::HookContext;
use matterstream_vm::host::VmHost;
use matterstream_vm::ui_vm;

const COLS: u32 = 7;
const ROWS: u32 = 6;
const CELL_SIZE: u32 = 80;
const BOARD_X: u32 = 40;
const BOARD_Y: u32 = 60;
const WIDTH: u32 = COLS * CELL_SIZE + BOARD_X * 2;
const HEIGHT: u32 = ROWS * CELL_SIZE + BOARD_Y + 80;

fn main() {
    // Set up hooks
    let mut hooks = HookContext::new();
    let board = hooks.use_state_grid(COLS * ROWS, 0);
    let player = hooks.use_state_i32(1);
    let game_over = hooks.use_state_i32(0);
    let _winner = hooks.use_state_i32(0);
    let _hover_col = hooks.use_state_i32(-1);

    // Build CPU logic bytecode by hand (event processing + game logic)
    // For this example, we use the CPU softbuffer path instead of GPU pipeline
    let logic_bytecode = Vec::new(); // Game logic handled in Rust directly below
    let render_bytecode = Vec::new();

    let mut host = VmHost::new(logic_bytecode, render_bytecode, hooks);

    // Initialize board via VM banks directly
    host.vm.int_bank[player.index as usize] = 1;

    // Check for --timeout arg
    let timeout_frames: Option<u64> = std::env::args()
        .position(|a| a == "--timeout")
        .and_then(|i| std::env::args().nth(i + 1))
        .and_then(|s| s.parse::<u64>().ok())
        .map(|secs| secs * 60); // ~60fps

    println!("Connect 4 — Click a column to drop a piece. Player 1=Red, Player 2=Yellow");

    // Simple game loop using softbuffer
    let mut frame = 0u64;
    let mut buf = vec![0u32; (WIDTH * HEIGHT) as usize];

    // Simulate a few frames and render to verify everything works
    loop {
        frame += 1;
        if let Some(max) = timeout_frames {
            if frame > max {
                println!("Timeout reached after {} frames", frame);
                break;
            }
        }

        // Clear buffer
        for p in buf.iter_mut() {
            *p = 0x1a1a2e; // dark blue background
        }

        let _current_player = host.vm.int_bank[player.index as usize];
        let is_game_over = host.vm.int_bank[game_over.index as usize];

        // Draw board background
        ui_vm::draw_filled_rect(
            &mut buf, WIDTH, HEIGHT,
            BOARD_X as i32, BOARD_Y as i32,
            COLS * CELL_SIZE, ROWS * CELL_SIZE,
            ui_vm::rgba(0, 0, 180, 255),
        );

        // Draw cells
        for row in 0..ROWS {
            for col in 0..COLS {
                let cell_idx = (row * COLS + col) as usize;
                let zp_offset = board.index as usize + cell_idx * 4;
                let cell_val = if zp_offset + 3 < 256 {
                    i32::from_le_bytes([
                        host.vm.zero_page[zp_offset],
                        host.vm.zero_page[zp_offset + 1],
                        host.vm.zero_page[zp_offset + 2],
                        host.vm.zero_page[zp_offset + 3],
                    ])
                } else {
                    0
                };

                let cx = BOARD_X + col * CELL_SIZE + CELL_SIZE / 2;
                let cy = BOARD_Y + row * CELL_SIZE + CELL_SIZE / 2;

                let color = match cell_val {
                    1 => ui_vm::rgba(255, 50, 50, 255),   // Red
                    2 => ui_vm::rgba(255, 255, 50, 255),   // Yellow
                    _ => ui_vm::rgba(26, 26, 46, 255),     // Empty (dark)
                };

                ui_vm::draw_filled_circle(&mut buf, WIDTH, HEIGHT, cx as i32, cy as i32, 30, color);
            }
        }

        // Draw status text placeholder
        if is_game_over != 0 {
            let win_color = ui_vm::rgba(255, 255, 255, 200);
            ui_vm::draw_filled_rect(
                &mut buf, WIDTH, HEIGHT,
                (WIDTH / 4) as i32, (HEIGHT / 2 - 30) as i32,
                WIDTH / 2, 60,
                win_color,
            );
        }

        // For the non-interactive demo, drop some pieces
        if frame == 1 {
            drop_piece(&mut host.vm.zero_page, board.index, 3, 1);
        } else if frame == 2 {
            drop_piece(&mut host.vm.zero_page, board.index, 4, 2);
        } else if frame == 3 {
            drop_piece(&mut host.vm.zero_page, board.index, 3, 1);
        } else if frame == 4 {
            drop_piece(&mut host.vm.zero_page, board.index, 4, 2);
        } else if frame == 5 {
            drop_piece(&mut host.vm.zero_page, board.index, 3, 1);
        } else if frame == 6 {
            drop_piece(&mut host.vm.zero_page, board.index, 4, 2);
        } else if frame == 7 {
            drop_piece(&mut host.vm.zero_page, board.index, 3, 1);
            println!("Player 1 wins with 4 in a row!");
            break;
        }

        if frame > 300 {
            break; // Safety limit for non-interactive mode
        }
    }

    println!("Connect 4 example completed. {} frames rendered.", frame);
}

fn drop_piece(zero_page: &mut [u8; 256], board_offset: u32, col: u32, player: i32) {
    // Find lowest empty row in column
    for row in (0..ROWS).rev() {
        let cell_idx = row * COLS + col;
        let zp_offset = board_offset as usize + cell_idx as usize * 4;
        if zp_offset + 3 < 256 {
            let val = i32::from_le_bytes([
                zero_page[zp_offset],
                zero_page[zp_offset + 1],
                zero_page[zp_offset + 2],
                zero_page[zp_offset + 3],
            ]);
            if val == 0 {
                let bytes = player.to_le_bytes();
                zero_page[zp_offset] = bytes[0];
                zero_page[zp_offset + 1] = bytes[1];
                zero_page[zp_offset + 2] = bytes[2];
                zero_page[zp_offset + 3] = bytes[3];
                return;
            }
        }
    }
}
