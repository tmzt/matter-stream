# UI VM Design: Dual-VM Architecture

## Overview

MatterStream uses a **dual-VM architecture** for UI rendering and game logic:

1. **CPU RPN VM** (`rpn.rs`) — Executes game logic, manages state, processes events
2. **GPU Compute Shader** (`shader_compute.wgsl`) — Interprets render bytecode into DrawCmd nodes
3. **GPU Fragment Shader** (`shader_render.wgsl`) — Renders DrawCmd nodes as SDF primitives

This separation provides security, predictability, and auditability while enabling GPU-accelerated rendering.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────┐
│  CPU Side                                           │
│                                                     │
│  ┌──────────────┐   ┌──────────────┐                │
│  │  Hooks       │──▶│  RPN VM      │                │
│  │  (State      │   │  (Logic      │                │
│  │   Allocation)│   │   Bytecode)  │                │
│  └──────────────┘   └──────┬───────┘                │
│                            │ sync                   │
│                     ┌──────▼───────┐                │
│                     │ GpuUniforms  │                │
│                     │ (384 bytes)  │────────────┐   │
│                     └──────────────┘            │   │
├─────────────────────────────────────────────────┼───┤
│  GPU Side                                       │   │
│                                                 ▼   │
│  ┌──────────────┐   ┌──────────────┐   ┌────────┐  │
│  │  Compute     │──▶│  DrawCmd     │──▶│Fragment│  │
│  │  Shader      │   │  Buffer      │   │ Shader │  │
│  │  (Render VM) │   │  (4096 max)  │   │ (SDF)  │  │
│  └──────────────┘   └──────────────┘   └────────┘  │
└─────────────────────────────────────────────────────┘
```

## Uniform Tiers

GpuUniforms is a 384-byte `repr(C)` struct uploaded to the GPU each frame:

### Tier 0: System Globals (64 bytes)

| Field       | Type       | Contents                                |
|-------------|------------|-----------------------------------------|
| time_delta  | vec4<f32>  | [time_s, delta_ms, frame_count, 0]      |
| resolution  | vec4<f32>  | [width, height, scale_factor, 0]        |
| mouse       | vec4<f32>  | [mouse_x, mouse_y, button_state, 0]     |
| theme       | vec4<f32>  | [is_dark, accent_r, accent_g, accent_b] |

### Tier 1: App-Specific Typed Banks (192 bytes)

| Bank        | Type            | Slots | Usage                        |
|-------------|-----------------|-------|------------------------------|
| vec4_bank   | [vec4<f32>; 16] | 16    | Colors, compound state, RGBA |
| vec3_bank   | [vec4<f32>; 16] | 16    | Positions, velocities (padded)|
| scalar_bank | [vec4<f32>; 4]  | 16    | f32s packed into 4 vec4s     |
| int_bank    | [ivec4; 4]      | 16    | i32s packed into 4 ivec4s    |

### Tier 2: ZeroPage (64 bytes)

| Field     | Type          | Size     | Usage                       |
|-----------|---------------|----------|-----------------------------|
| zero_page | [uvec4; 16]   | 256 bytes| Raw memory for grids/arrays |

## CPU RPN VM

The CPU VM (`RpnOp`, 0x00–0x53 + 0x30–0x3A) handles:

- **Integer arithmetic**: Add, Sub, Mul, Div, Mod (u64 wrapping)
- **Float arithmetic**: FAdd, FSub, FMul, FDiv, FNeg, FAbs (f32 via bit reinterpretation)
- **Float comparisons**: FCmpGt, FCmpLt, FCmpEq
- **Type conversion**: I2F (i32→f32), F2I (f32→i32)
- **Bank access**: LoadBank/StoreBank (slot-level), LoadBankComp/StoreBankComp (component-level)
- **ZeroPage**: LoadZpI32/StoreZpI32 (4-byte loads/stores for game grids)
- **Control flow**: Jmp, JmpIf, Halt, Call, Ret
- **Events**: EvPoll, EvHasEvent (event queue processing)
- **UI draws**: UiSetColor, UiBox, UiSlab, UiCircle, UiText, UiLine (CPU fallback path)

### Gas Metering

Every opcode has a gas cost (default budget: 10M). Backward jumps are tracked separately (limit: 10K). This prevents infinite loops and provides deterministic execution bounds.

## GPU Compute Shader

The compute shader (`shader_compute.wgsl`) is a **fixed-function interpreter**:

- Reads render bytecode (u32 words) from a storage buffer
- Reads GpuUniforms for state access
- Writes DrawCmd entries to an output buffer
- Supports 27 opcodes matching `gpu::render_ops` constants

### Why Fixed-Function

The compute shader does NOT execute arbitrary GPU code. It interprets a restricted opcode set:

1. **Security**: Untrusted bytecode cannot escape the interpreter sandbox
2. **Predictability**: Known opcode costs enable deterministic performance
3. **Auditability**: The opcode set is small enough to formally verify

## GPU Fragment Shader

The fragment shader (`shader_render.wgsl`) renders DrawCmd nodes as SDF primitives:

- **Full-screen triangle**: 3 vertices, no vertex buffer needed
- **SDF primitives**: `sd_box`, `sd_rounded_box`, `sd_circle`, `sd_segment`
- **Anti-aliasing**: `smoothstep(-0.5, 0.5, d)` edge smoothing
- **Shadows**: Subtle drop shadows with offset SDF evaluation
- **Compositing**: `blend_over` alpha blending front-to-back

## React Hooks Model

State is allocated declaratively via `HookContext`:

```rust
let board = hooks.use_state_grid(42, 0);   // ZeroPage: 42 i32 cells
let score = hooks.use_state_i32(0);         // Int bank slot
let bird = hooks.use_state_vec3([80.0, 300.0, 0.0]); // Vec3 bank slot
```

Each `use_state_*` call returns a `StateSlot` (bank kind + index + count). The assembler uses these slots directly:

```rust
asm.load(score);           // push score value
asm.push32(1);
asm.add();
asm.store(score);          // score += 1

asm.load_bank_comp(bird, 1);  // bird.y
asm.load_bank_comp(bird, 2);  // bird.velocity
asm.fadd();
asm.store_bank_comp(bird, 1); // bird.y += velocity
```

After each tick, `sync_vm_to_uniforms()` marshals VM banks → GpuUniforms for GPU upload.

## Comparison with cardgpu5

| Feature              | cardgpu5                    | MatterStream                    |
|----------------------|-----------------------------|---------------------------------|
| Memory layout        | `plane_data: array<vec4, 256>` | Typed banks (vec4, vec3, scalar, int, ZeroPage) |
| State model          | Generic planes              | React-style hooks with typed allocation |
| VM location          | Single GPU compute shader   | Dual: CPU logic + GPU render    |
| Float support        | Built-in (GPU native)       | FAdd/FSub/FMul/FDiv opcodes    |
| Component access     | plane_data[i][comp]         | LoadBankComp/StoreBankComp     |
| Grid/array storage   | Packed into planes          | Dedicated ZeroPage (256 bytes) |
| Safety               | WGSL sandboxing             | Gas metering + fixed-function GPU |

MatterStream's typed bank approach is more structured than cardgpu5's generic `plane_data` slots, providing type safety and clearer semantics at the cost of slightly more API surface.
