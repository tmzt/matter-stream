//! Flappy Bird — physics-based game using the MTSM RPN VM with softbuffer rendering.
//!
//! State (hooks):
//! - bird: Vec3 bank (x, y, velocity)
//! - pipes: Vec4 bank × 3 (pipe_x, gap_y, scored, 0)
//! - score: Int bank
//! - game_state: Int bank (0=waiting, 1=playing, 2=dead)

use matterstream_vm::hooks::HookContext;
use matterstream_vm::host::VmHost;
use matterstream_vm::rpn::SimpleRng;
use matterstream_vm::ui_vm;

const WIDTH: u32 = 400;
const HEIGHT: u32 = 600;
const BIRD_X: f32 = 80.0;
const BIRD_RADIUS: u32 = 15;
const PIPE_WIDTH: f32 = 60.0;
const PIPE_GAP: f32 = 150.0;
const GRAVITY: f32 = 0.5;
const FLAP_VEL: f32 = -8.0;
const PIPE_SPEED: f32 = 2.5;
const GROUND_Y: f32 = 550.0;

fn main() {
    let mut hooks = HookContext::new();
    let bird = hooks.use_state_vec3([BIRD_X, 300.0, 0.0]);
    let pipe0 = hooks.use_state_vec4([400.0, 250.0, 0.0, 0.0]);
    let pipe1 = hooks.use_state_vec4([600.0, 200.0, 0.0, 0.0]);
    let pipe2 = hooks.use_state_vec4([800.0, 300.0, 0.0, 0.0]);
    let score = hooks.use_state_i32(0);
    let game_state = hooks.use_state_i32(0);

    let logic_bytecode = Vec::new();
    let render_bytecode = Vec::new();
    let mut host = VmHost::new(logic_bytecode, render_bytecode, hooks);

    // Initialize bird position
    host.vm.vec3_bank[bird.index as usize] = [BIRD_X, 300.0, 0.0];
    host.vm.vec4_bank[pipe0.index as usize] = [400.0, 250.0, 0.0, 0.0];
    host.vm.vec4_bank[pipe1.index as usize] = [600.0, 200.0, 0.0, 0.0];
    host.vm.vec4_bank[pipe2.index as usize] = [800.0, 300.0, 0.0, 0.0];
    host.vm.int_bank[game_state.index as usize] = 1; // start playing

    let mut rng = SimpleRng::new(0xBEEF);

    let timeout_frames: Option<u64> = std::env::args()
        .position(|a| a == "--timeout")
        .and_then(|i| std::env::args().nth(i + 1))
        .and_then(|s| s.parse::<u64>().ok())
        .map(|secs| secs * 60);

    println!("Flappy Bird — Press Space to flap!");

    let mut buf = vec![0u32; (WIDTH * HEIGHT) as usize];
    let mut frame = 0u64;
    let mut auto_flap_timer = 0;

    loop {
        frame += 1;
        if let Some(max) = timeout_frames {
            if frame > max {
                println!("Timeout reached after {} frames", frame);
                break;
            }
        }

        let state = host.vm.int_bank[game_state.index as usize];
        if state == 2 {
            // Dead — stop
            break;
        }

        // Physics: gravity + velocity
        let bird_data = &mut host.vm.vec3_bank[bird.index as usize];
        bird_data[2] += GRAVITY; // velocity += gravity
        bird_data[1] += bird_data[2]; // y += velocity

        // Auto-flap every ~25 frames for demo
        auto_flap_timer += 1;
        if auto_flap_timer > 25 {
            auto_flap_timer = 0;
            bird_data[2] = FLAP_VEL;
        }

        // Move pipes
        let pipe_indices = [pipe0.index, pipe1.index, pipe2.index];
        for &pi in &pipe_indices {
            let pipe = &mut host.vm.vec4_bank[pi as usize];
            pipe[0] -= PIPE_SPEED;

            // Respawn pipe off-screen right
            if pipe[0] < -PIPE_WIDTH {
                pipe[0] = WIDTH as f32 + 20.0;
                pipe[1] = 100.0 + rng.next_bounded(300) as f32;
                pipe[2] = 0.0; // reset scored flag
            }

            // Score if bird passed pipe
            if pipe[0] < BIRD_X && pipe[2] == 0.0 {
                pipe[2] = 1.0;
                host.vm.int_bank[score.index as usize] += 1;
            }

            // Collision detection
            let bird_cx = BIRD_X;
            let bird_cy = host.vm.vec3_bank[bird.index as usize][1];
            let r = BIRD_RADIUS as f32;
            let px = pipe[0];
            let gap_y = pipe[1];

            // Check if bird overlaps pipe horizontally
            if bird_cx + r > px && bird_cx - r < px + PIPE_WIDTH {
                // Check if bird is outside the gap
                if bird_cy - r < gap_y - PIPE_GAP / 2.0 || bird_cy + r > gap_y + PIPE_GAP / 2.0
                {
                    host.vm.int_bank[game_state.index as usize] = 2; // dead
                }
            }
        }

        // Ground/ceiling collision
        let bird_y = host.vm.vec3_bank[bird.index as usize][1];
        if !(0.0..=GROUND_Y).contains(&bird_y) {
            host.vm.int_bank[game_state.index as usize] = 2;
        }

        // ── Render ──
        // Clear to sky blue
        for p in buf.iter_mut() {
            *p = 0x70C5CE;
        }

        // Draw ground
        ui_vm::draw_filled_rect(
            &mut buf, WIDTH, HEIGHT,
            0, GROUND_Y as i32, WIDTH, HEIGHT - GROUND_Y as u32,
            ui_vm::rgba(222, 184, 135, 255),
        );

        // Draw pipes
        for &pi in &pipe_indices {
            let pipe = host.vm.vec4_bank[pi as usize];
            let px = pipe[0] as i32;
            let gap_y = pipe[1];
            let pipe_color = ui_vm::rgba(50, 200, 50, 255);

            // Top pipe
            let top_h = (gap_y - PIPE_GAP / 2.0) as u32;
            if top_h > 0 {
                ui_vm::draw_filled_rect(
                    &mut buf, WIDTH, HEIGHT,
                    px, 0, PIPE_WIDTH as u32, top_h,
                    pipe_color,
                );
            }

            // Bottom pipe
            let bottom_y = (gap_y + PIPE_GAP / 2.0) as i32;
            let bottom_h = GROUND_Y as i32 - bottom_y;
            if bottom_h > 0 {
                ui_vm::draw_filled_rect(
                    &mut buf, WIDTH, HEIGHT,
                    px, bottom_y, PIPE_WIDTH as u32, bottom_h as u32,
                    pipe_color,
                );
            }
        }

        // Draw bird
        let bird_data = host.vm.vec3_bank[bird.index as usize];
        ui_vm::draw_filled_circle(
            &mut buf, WIDTH, HEIGHT,
            bird_data[0] as i32, bird_data[1] as i32,
            BIRD_RADIUS,
            ui_vm::rgba(255, 255, 0, 255),
        );

        if frame > 600 {
            break; // Safety limit
        }
    }

    let final_score = host.vm.int_bank[score.index as usize];
    println!(
        "Flappy Bird example completed. {} frames, score: {}",
        frame, final_score
    );
}
