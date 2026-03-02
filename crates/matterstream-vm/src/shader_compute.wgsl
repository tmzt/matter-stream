// Compute shader: VM interpreter for render bytecode → DrawCmd output.
//
// Reads GpuUniforms (Tier 0 globals, Tier 1 banks, Tier 2 ZeroPage)
// and render bytecode. Each workgroup thread processes a range of
// bytecode instructions, emitting DrawCmd entries to an output buffer.
//
// Security model: This is a fixed-function interpreter — it cannot
// execute arbitrary GPU code. Only the defined render_ops are supported.

// ── Uniforms (matches host.rs GpuUniforms layout) ──

struct GpuUniforms {
    // Tier 0: System Globals
    time_delta: vec4<f32>,     // [time_s, delta_ms, frame_count, 0]
    resolution: vec4<f32>,     // [width, height, scale_factor, 0]
    mouse: vec4<f32>,          // [mouse_x, mouse_y, button_state, 0]
    theme: vec4<f32>,          // [is_dark, accent_r, accent_g, accent_b]

    // Tier 1: App-Specific Typed Register Banks
    vec4_bank: array<vec4<f32>, 16>,
    vec3_bank: array<vec4<f32>, 16>,   // padded to vec4
    scalar_bank: array<vec4<f32>, 4>,  // 16 f32s packed
    int_bank: array<vec4<i32>, 4>,     // 16 i32s packed

    // Tier 2: ZeroPage (256 bytes as 16 uvec4s)
    zero_page: array<vec4<u32>, 16>,
};

// ── DrawCmd output (matches gpu.rs DrawCmd layout) ──

struct DrawCmd {
    pos: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
    params: vec4<f32>,   // [ty, radius, softness, slot]
};

// ── Render bytecode header ──

struct RenderHeader {
    cmd_count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

// ── Bindings ──

@group(0) @binding(0) var<uniform> uniforms: GpuUniforms;
@group(0) @binding(1) var<storage, read> bytecode: array<u32>;
@group(0) @binding(2) var<storage, read_write> draw_cmds: array<DrawCmd>;
@group(0) @binding(3) var<storage, read_write> header: RenderHeader;

// ── Render ops (matching gpu.rs render_ops constants) ──

const OP_NOP: u32 = 0u;
const OP_SET_COLOR: u32 = 1u;
const OP_BOX: u32 = 2u;
const OP_SLAB: u32 = 3u;
const OP_CIRCLE: u32 = 4u;
const OP_TEXT: u32 = 5u;
const OP_LINE: u32 = 6u;
const OP_PUSH_IMM: u32 = 7u;
const OP_LOAD_SCALAR: u32 = 8u;
const OP_LOAD_INT: u32 = 9u;
const OP_LOAD_VEC4: u32 = 10u;
const OP_LOAD_ZP: u32 = 11u;
const OP_JMP: u32 = 12u;
const OP_JMP_IF: u32 = 13u;
const OP_CMP_GT: u32 = 14u;
const OP_ADD: u32 = 15u;
const OP_HALT: u32 = 16u;
const OP_DUP: u32 = 17u;
const OP_DROP: u32 = 18u;
const OP_PUSH_STATE: u32 = 19u;
const OP_POP_STATE: u32 = 20u;
const OP_SET_OFFSET: u32 = 21u;
const OP_MOD: u32 = 22u;
const OP_DIV: u32 = 23u;
const OP_MUL: u32 = 24u;
const OP_SUB: u32 = 25u;
const OP_CMP_EQ: u32 = 26u;

const MAX_STACK: u32 = 64u;
const MAX_STATE_STACK: u32 = 8u;
const MAX_DRAW_CMDS: u32 = 4096u;

// ── Compute shader entry point ──

@compute @workgroup_size(1, 1, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    // Single-threaded interpreter — process all bytecode sequentially.
    // Future: partition bytecode across threads for parallel emit.

    var stack: array<u32, 64>;
    var sp: u32 = 0u;

    // Draw state
    var color: vec4<f32> = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    var offset_x: f32 = 0.0;
    var offset_y: f32 = 0.0;

    // State stack for push/pop
    var saved_color: array<vec4<f32>, 8>;
    var saved_ox: array<f32, 8>;
    var saved_oy: array<f32, 8>;
    var state_sp: u32 = 0u;

    var cmd_count: u32 = 0u;
    let bc_len = arrayLength(&bytecode);
    var pc: u32 = 0u;

    loop {
        if pc >= bc_len {
            break;
        }

        let op = bytecode[pc];
        pc = pc + 1u;

        switch op {
            case 0u: { // OP_NOP
                // no-op
            }
            case 16u: { // OP_HALT
                pc = bc_len;
            }
            case 7u: { // OP_PUSH_IMM
                if pc < bc_len && sp < MAX_STACK {
                    stack[sp] = bytecode[pc];
                    sp = sp + 1u;
                    pc = pc + 1u;
                }
            }
            case 17u: { // OP_DUP
                if sp > 0u && sp < MAX_STACK {
                    stack[sp] = stack[sp - 1u];
                    sp = sp + 1u;
                }
            }
            case 18u: { // OP_DROP
                if sp > 0u {
                    sp = sp - 1u;
                }
            }
            case 8u: { // OP_LOAD_SCALAR
                if pc < bc_len && sp < MAX_STACK {
                    let idx = bytecode[pc];
                    pc = pc + 1u;
                    let pack_idx = idx / 4u;
                    let sub_idx = idx % 4u;
                    if pack_idx < 4u {
                        stack[sp] = bitcast<u32>(uniforms.scalar_bank[pack_idx][sub_idx]);
                        sp = sp + 1u;
                    }
                }
            }
            case 9u: { // OP_LOAD_INT
                if pc < bc_len && sp < MAX_STACK {
                    let idx = bytecode[pc];
                    pc = pc + 1u;
                    let pack_idx = idx / 4u;
                    let sub_idx = idx % 4u;
                    if pack_idx < 4u {
                        stack[sp] = u32(uniforms.int_bank[pack_idx][sub_idx]);
                        sp = sp + 1u;
                    }
                }
            }
            case 10u: { // OP_LOAD_VEC4
                if pc < bc_len && sp < MAX_STACK {
                    let idx = bytecode[pc];
                    pc = pc + 1u;
                    if idx < 16u {
                        // Push first component only (use as f32 bits)
                        stack[sp] = bitcast<u32>(uniforms.vec4_bank[idx][0]);
                        sp = sp + 1u;
                    }
                }
            }
            case 11u: { // OP_LOAD_ZP
                if pc < bc_len && sp < MAX_STACK {
                    let byte_offset = bytecode[pc];
                    pc = pc + 1u;
                    let word_idx = byte_offset / 16u;
                    let sub_idx = (byte_offset % 16u) / 4u;
                    if word_idx < 16u {
                        stack[sp] = uniforms.zero_page[word_idx][sub_idx];
                        sp = sp + 1u;
                    }
                }
            }
            case 15u: { // OP_ADD
                if sp >= 2u {
                    sp = sp - 1u;
                    let b = stack[sp];
                    sp = sp - 1u;
                    let a = stack[sp];
                    stack[sp] = a + b;
                    sp = sp + 1u;
                }
            }
            case 25u: { // OP_SUB
                if sp >= 2u {
                    sp = sp - 1u;
                    let b = stack[sp];
                    sp = sp - 1u;
                    let a = stack[sp];
                    stack[sp] = a - b;
                    sp = sp + 1u;
                }
            }
            case 24u: { // OP_MUL
                if sp >= 2u {
                    sp = sp - 1u;
                    let b = stack[sp];
                    sp = sp - 1u;
                    let a = stack[sp];
                    stack[sp] = a * b;
                    sp = sp + 1u;
                }
            }
            case 23u: { // OP_DIV
                if sp >= 2u {
                    sp = sp - 1u;
                    let b = stack[sp];
                    sp = sp - 1u;
                    let a = stack[sp];
                    if b != 0u {
                        stack[sp] = a / b;
                    } else {
                        stack[sp] = 0u;
                    }
                    sp = sp + 1u;
                }
            }
            case 22u: { // OP_MOD
                if sp >= 2u {
                    sp = sp - 1u;
                    let b = stack[sp];
                    sp = sp - 1u;
                    let a = stack[sp];
                    if b != 0u {
                        stack[sp] = a % b;
                    } else {
                        stack[sp] = 0u;
                    }
                    sp = sp + 1u;
                }
            }
            case 14u: { // OP_CMP_GT
                if sp >= 2u {
                    sp = sp - 1u;
                    let b = stack[sp];
                    sp = sp - 1u;
                    let a = stack[sp];
                    if a > b {
                        stack[sp] = 1u;
                    } else {
                        stack[sp] = 0u;
                    }
                    sp = sp + 1u;
                }
            }
            case 26u: { // OP_CMP_EQ
                if sp >= 2u {
                    sp = sp - 1u;
                    let b = stack[sp];
                    sp = sp - 1u;
                    let a = stack[sp];
                    if a == b {
                        stack[sp] = 1u;
                    } else {
                        stack[sp] = 0u;
                    }
                    sp = sp + 1u;
                }
            }
            case 12u: { // OP_JMP
                if pc < bc_len {
                    pc = bytecode[pc];
                }
            }
            case 13u: { // OP_JMP_IF
                if pc < bc_len && sp > 0u {
                    let target = bytecode[pc];
                    pc = pc + 1u;
                    sp = sp - 1u;
                    let cond = stack[sp];
                    if cond != 0u {
                        pc = target;
                    }
                }
            }
            case 1u: { // OP_SET_COLOR
                if pc < bc_len {
                    let rgba = bytecode[pc];
                    pc = pc + 1u;
                    color = vec4<f32>(
                        f32((rgba >> 24u) & 0xFFu) / 255.0,
                        f32((rgba >> 16u) & 0xFFu) / 255.0,
                        f32((rgba >> 8u) & 0xFFu) / 255.0,
                        f32(rgba & 0xFFu) / 255.0
                    );
                }
            }
            case 19u: { // OP_PUSH_STATE
                if state_sp < MAX_STATE_STACK {
                    saved_color[state_sp] = color;
                    saved_ox[state_sp] = offset_x;
                    saved_oy[state_sp] = offset_y;
                    state_sp = state_sp + 1u;
                }
            }
            case 20u: { // OP_POP_STATE
                if state_sp > 0u {
                    state_sp = state_sp - 1u;
                    color = saved_color[state_sp];
                    offset_x = saved_ox[state_sp];
                    offset_y = saved_oy[state_sp];
                }
            }
            case 21u: { // OP_SET_OFFSET
                if sp >= 2u {
                    sp = sp - 1u;
                    let dy = bitcast<f32>(stack[sp]);
                    sp = sp - 1u;
                    let dx = bitcast<f32>(stack[sp]);
                    offset_x = dx;
                    offset_y = dy;
                }
            }
            case 2u: { // OP_BOX
                if sp >= 4u && cmd_count < MAX_DRAW_CMDS {
                    sp = sp - 1u;
                    let h = bitcast<f32>(stack[sp]);
                    sp = sp - 1u;
                    let w = bitcast<f32>(stack[sp]);
                    sp = sp - 1u;
                    let y = bitcast<f32>(stack[sp]) + offset_y;
                    sp = sp - 1u;
                    let x = bitcast<f32>(stack[sp]) + offset_x;
                    var cmd: DrawCmd;
                    cmd.pos = vec2<f32>(x, y);
                    cmd.size = vec2<f32>(w, h);
                    cmd.color = color;
                    cmd.params = vec4<f32>(0.0, 0.0, 0.0, 0.0);
                    draw_cmds[cmd_count] = cmd;
                    cmd_count = cmd_count + 1u;
                }
            }
            case 3u: { // OP_SLAB
                if sp >= 5u && cmd_count < MAX_DRAW_CMDS {
                    sp = sp - 1u;
                    let radius = bitcast<f32>(stack[sp]);
                    sp = sp - 1u;
                    let h = bitcast<f32>(stack[sp]);
                    sp = sp - 1u;
                    let w = bitcast<f32>(stack[sp]);
                    sp = sp - 1u;
                    let y = bitcast<f32>(stack[sp]) + offset_y;
                    sp = sp - 1u;
                    let x = bitcast<f32>(stack[sp]) + offset_x;
                    var cmd: DrawCmd;
                    cmd.pos = vec2<f32>(x, y);
                    cmd.size = vec2<f32>(w, h);
                    cmd.color = color;
                    cmd.params = vec4<f32>(1.0, radius, 0.0, 0.0);
                    draw_cmds[cmd_count] = cmd;
                    cmd_count = cmd_count + 1u;
                }
            }
            case 4u: { // OP_CIRCLE
                if sp >= 3u && cmd_count < MAX_DRAW_CMDS {
                    sp = sp - 1u;
                    let r = bitcast<f32>(stack[sp]);
                    sp = sp - 1u;
                    let cy = bitcast<f32>(stack[sp]) + offset_y;
                    sp = sp - 1u;
                    let cx = bitcast<f32>(stack[sp]) + offset_x;
                    let d = r * 2.0;
                    var cmd: DrawCmd;
                    cmd.pos = vec2<f32>(cx - r, cy - r);
                    cmd.size = vec2<f32>(d, d);
                    cmd.color = color;
                    cmd.params = vec4<f32>(2.0, r, 0.0, 0.0);
                    draw_cmds[cmd_count] = cmd;
                    cmd_count = cmd_count + 1u;
                }
            }
            case 5u: { // OP_TEXT
                if sp >= 4u && cmd_count < MAX_DRAW_CMDS {
                    sp = sp - 1u;
                    let slot = bitcast<f32>(stack[sp]);
                    sp = sp - 1u;
                    let size = bitcast<f32>(stack[sp]);
                    sp = sp - 1u;
                    let y = bitcast<f32>(stack[sp]) + offset_y;
                    sp = sp - 1u;
                    let x = bitcast<f32>(stack[sp]) + offset_x;
                    var cmd: DrawCmd;
                    cmd.pos = vec2<f32>(x, y);
                    cmd.size = vec2<f32>(size * 4.0, size);
                    cmd.color = color;
                    cmd.params = vec4<f32>(4.0, 0.0, 0.0, slot);
                    draw_cmds[cmd_count] = cmd;
                    cmd_count = cmd_count + 1u;
                }
            }
            case 6u: { // OP_LINE
                if sp >= 4u && cmd_count < MAX_DRAW_CMDS {
                    sp = sp - 1u;
                    let y2 = bitcast<f32>(stack[sp]) + offset_y;
                    sp = sp - 1u;
                    let x2 = bitcast<f32>(stack[sp]) + offset_x;
                    sp = sp - 1u;
                    let y1 = bitcast<f32>(stack[sp]) + offset_y;
                    sp = sp - 1u;
                    let x1 = bitcast<f32>(stack[sp]) + offset_x;
                    let cx = (x1 + x2) * 0.5;
                    let cy = (y1 + y2) * 0.5;
                    let dx = x2 - x1;
                    let dy = y2 - y1;
                    let len = sqrt(dx * dx + dy * dy);
                    var cmd: DrawCmd;
                    cmd.pos = vec2<f32>(cx - len * 0.5, cy - 0.5);
                    cmd.size = vec2<f32>(len, 1.0);
                    cmd.color = color;
                    cmd.params = vec4<f32>(3.0, 0.0, 0.0, 0.0);
                    draw_cmds[cmd_count] = cmd;
                    cmd_count = cmd_count + 1u;
                }
            }
            default: {
                // Unknown opcode — skip
            }
        }
    }

    header.cmd_count = cmd_count;
}
