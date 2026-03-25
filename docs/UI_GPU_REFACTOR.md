# UI GPU Refactor — SDF Compute Pipeline + CR-Based Opcodes

## Context

matter-stream has two rendering paths that need unification:
1. **CPU softbuffer** (`matterstream-ui-soft`) — scan-line rasterizer, the current default
2. **GPU SDF pipeline** (cardgpu5 + existing `shader_compute.wgsl`/`shader_render.wgsl` in vm) — compute shader executes bytecode VM, builds SDF node tree, fragment shader does per-pixel evaluation

The GPU path already exists in `crates/matterstream-vm/src/` (shader_compute.wgsl, shader_render.wgsl, gpu.rs, host.rs) but isn't wired up as the primary rendering path. Meanwhile, UI opcodes are stuck in the universal 0x40 range from the old flat design instead of living in the CR-dependent MTUI page.

**Goals:**
1. Move all UI draw ops from 0x40-0x4D to CR-dependent MTUI page at 0x70+
2. Deprecate the old 0x40 opcode numbers (transition period, then remove)
3. Wire up the GPU SDF pipeline as the primary renderer via `Rasterizer` trait
4. Get run-tsx working so any .tsx file can be tested end-to-end
5. The compiler must work for this pipeline

## Why no UI ops should remain universal

**Every current 0x40-0x4D opcode is MTUI-specific:**
- `UiSetColor` — sets draw state color (MTUI concept)
- `UiBox/Slab/Circle/Line/Text/TextStr/Action` — emit draw commands (MTUI output)
- `UiPushState/PopState` — manage MTUI draw state stack
- `UiApplyOffset/Matrix/ReplaceOffset/ReplaceMatrix` — MTUI transform stack

None of these have meaning outside the MTUI CR mode. A VQL query doesn't need `UiSetColor`. A Skill definition doesn't push draw commands. The only reason they're at 0x40 is historical — the flat opcode list predates the CR system.

**Defense of universality would require:** an opcode that's used by multiple CR modes. None of the UI ops qualify. Even `UiPushState`/`UiPopState` are specific to the MTUI draw state stack, not general-purpose state management.

## Naming clarification: RpnOp vs UiOp

**RpnOp** — the math/evaluation stack machine opcodes (0x00-0x6F). Arithmetic, control flow, bank access, OID imports. These are RPN operations.

**UiOp** — the MTUI CR-dependent page (0x70+). Draw operations that produce `SdfDrawCmd`. These are UI operations, not RPN math. Currently misnamed as `RpnOp::UiSetColor` etc. — they belong in a separate `UiOp` enum or as a CR-dispatched sub-table.

The VM step() function dispatches:
- 0x00-0x6F → `RpnOp` handler (always available)
- 0x70-0x7F when CR[0]==MTUI → `UiOp` handler (produces SdfDrawCmd)
- 0x70-0x7F when CR[0]==VQL0 → `VqlOp` handler
- 0x70-0x7F when CR[0]==SKLL → `SkillOp` handler
- etc.

## Opcode Migration: 0x40 → 0x70 (MTUI CR page)

### New MTUI opcode table (when CR[0] == FOURCC_MTUI)

| Old (0x4x) | New (0x7x) | Name | Stack effect |
|-----------|-----------|------|-------------|
| 0x40 | 0x70 | SetColor | [rgba] → |
| 0x41 | 0x71 | Box | [x,y,w,h] → |
| 0x42 | 0x72 | Slab | [x,y,w,h,r] → |
| 0x43 | 0x73 | Circle | [cx,cy,r] → |
| 0x44 | 0x74 | Text | [x,y,size,slot] → |
| 0x45 | 0x75 | PushState | → |
| 0x46 | 0x76 | PopState | → |
| 0x47 | 0x77 | ApplyOffset | [dx,dy] → |
| 0x48 | 0x78 | Line | [x1,y1,x2,y2] → |
| 0x49 | 0x79 | TextStr | [x,y,size,str_idx] → |
| 0x4A | 0x7A | Action | [x,y,w,h,str_idx] → |
| 0x4B | 0x7B | ApplyMatrix | [16 floats] → |
| 0x4C | 0x7C | ReplaceOffset | [dx,dy] → |
| 0x4D | 0x7D | ReplaceMatrix | [16 floats] → |

### Transition strategy
- Old 0x40-0x4D opcodes remain in `from_u8` during transition (mapped to same handler)
- New bytecode emitted by compiler uses SetCR(MTUI) + 0x70+ opcodes
- The VM step() function checks: if CR[0]==MTUI and opcode in 0x70-0x7F, dispatch to MTUI handler
- Eventually remove 0x40-0x4D from `from_u8`

## GPU SDF Pipeline Integration

### Architecture (from cardgpu5)

```
TSX source
  → compiler (asm_compiler.rs) → bytecode + string_table
  → GPU compute shader (shader_compute.wgsl) → SdfDrawCmd buffer (SDF nodes)
  → GPU fragment shader (shader_render.wgsl) → per-pixel SDF evaluation → pixels
  → present pass → screen
```

### What already exists in matter-stream
- `crates/matterstream-vm/src/shader_compute.wgsl` — bytecode VM interpreter on GPU
- `crates/matterstream-vm/src/shader_render.wgsl` — SDF fragment shader
- `crates/matterstream-vm/src/gpu.rs` — `SdfDrawCmd` struct, `build_draw_list_from_ui_draws()`
- `crates/matterstream-vm/src/host.rs` — `GpuUniforms`, `VmHost` with tick loop

### Architecture: VM → SDF DrawList → Renderer

The drawing primitives ARE SDF primitives (Slab = rounded box SDF, Circle = circle SDF, etc.). Both renderers evaluate the same SDFs — the GPU does it per-pixel in a fragment shader, the CPU does it per-pixel in a loop.

```
TSX → compiler → bytecode
                    ↓
              CPU RPN VM (always)
                    ↓
         SetCR(MTUI) → MTUI ops emit SdfDrawCmd entries
                    ↓
              Vec<SdfDrawCmd> (SDF primitives, repr(C))
              /              \
    GPU pipeline           CPU pipeline
    (upload buffer,        (iterate pixels,
     frag shader evals      eval same SDF math,
     SDF per-pixel)         write to softbuffer)
```

### Unified type: SdfDrawCmd

One type everywhere. Lives in `matterstream-common` (zero deps, GPU-uploadable):

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SdfDrawCmd {
    pub pos: [f32; 2],      // x, y
    pub size: [f32; 2],     // w, h (or x2,y2 for lines)
    pub color: [f32; 4],    // RGBA 0.0-1.0
    pub params: [f32; 4],   // [type, radius, softness, slot]
}
// type: 0=Box, 1=Slab, 2=Circle, 3=Line, 4=Text
```

**`UiSdfDrawCmd` enum is removed.** The VM emits `SdfDrawCmd` directly from MTUI opcode handlers. No Rust code ever constructs draw commands — UI is authored in TSX, compiled to bytecode, executed by the VM. The `SdfDrawCmd::box_cmd()` etc. constructors are removed.

Rust interacts with the VM world only through:
- **VM-exit** (native hooks via OID) — push/pop stack values, never construct UI
- **Atomic read** via jump table — renderer reads SdfDrawCmd buffer pointer+length without VM cooperation
- **Atomic write** (semaphore) — VM signals "draw list ready", renderer polls on it

This gives lock-free producer/consumer: VM fills SdfDrawCmd buffer, atomically publishes, renderer reads and renders. No mutex, no message passing.

**Required for this refactor** — the prompt bar needs sub-frame latency for voice input control. Can't wait for a full VM tick.

### Shared draw buffer design

```rust
// In matterstream-common
pub struct DrawBuffer {
    pub cmds: *mut SdfDrawCmd,          // pre-allocated fixed-size buffer
    pub count: AtomicU32,            // number of valid commands
    pub ready: AtomicBool,           // semaphore: VM sets true when frame is complete
    pub capacity: u32,               // MAX_DRAW_CMDS (4096)
}
```

- **VM thread**: fills `cmds[0..n]`, stores `count`, sets `ready = true`
- **Render thread**: spins/polls on `ready`, reads `count` + `cmds[0..count]`, renders, sets `ready = false`
- **Input thread** (prompt bar): reads/writes voice state atomics directly via jump table — bypasses VM cycle entirely

The jump table is a flat array of `AtomicU64` slots that both VM bytecode and Rust code can access. Voice toggle, recording state, waveform data are all jump table entries.

**Action regions** (hit testing metadata) are not `SdfDrawCmd`s — they go in a separate `Vec<ActionRegion>` on the VM.

Draw type constants:
```rust
pub const DRAW_TYPE_BOX: f32 = 0.0;
pub const DRAW_TYPE_SLAB: f32 = 1.0;
pub const DRAW_TYPE_CIRCLE: f32 = 2.0;
pub const DRAW_TYPE_LINE: f32 = 3.0;
pub const DRAW_TYPE_TEXT: f32 = 4.0;
```

### Rasterizer trait goes away

The `Rasterizer` trait (`draw_rect`, `draw_circle`, etc.) is replaced. Both renderers evaluate SDFs directly. The CPU softbuffer rasterizer is rewritten to do per-pixel SDF evaluation matching the shader:

```rust
// For each pixel (px, py):
//   for each SdfDrawCmd:
//     d = sdf_eval(cmd, px, py)  // same math as shader
//     if d < 0: blend color
```

This produces pixel-identical output between GPU and CPU paths.

### UiPipeline trait

```rust
// In matterstream-common
pub trait UiPipeline {
    fn render(&mut self, draws: &[SdfDrawCmd], string_table: &[String]);
    fn resize(&mut self, width: u32, height: u32);
}
```

### Pipeline stage matrix

Each stage has a GPU and CPU implementation. All four combinations are valid:

| Compute (bytecode → SdfDrawCmd) | Render (SdfDrawCmd → pixels) | Use case |
|-----|-----|------|
| GPU (shader_compute.wgsl) | GPU (shader_render.wgsl) | Default, full GPU |
| CPU (RPN VM) | GPU (shader_render.wgsl) | Compute fallback, GPU render |
| CPU (RPN VM) | CPU (SDF eval → softbuffer) | Full software fallback (CI, old HW) |
| GPU (shader_compute.wgsl) | CPU (SDF eval → softbuffer) | Unlikely but valid |

**Compute stage:**
- GPU: shader_compute.wgsl — bytecode VM interpreter on GPU
- CPU: RPN VM (`matterstream-vm`) — same bytecode, same SdfDrawCmd output

**Render stage (vertex + fragment):**
- GPU: shader_render.wgsl — per-pixel SDF evaluation in fragment shader
- CPU: per-pixel SDF evaluation loop, writes to softbuffer (`matterstream-ui-soft`)

The CPU compute path (RPN VM) already exists. The GPU compute path exists in shader_compute.wgsl. Both produce `SdfDrawCmd` buffer — the render stage doesn't care which produced it.

### Hybrid pipeline test

The critical integration test: CPU compute → GPU render. This validates SdfDrawCmd format correctness across the CPU/GPU boundary.

```
Test: hybrid_cpu_compute_gpu_render
1. Compile TSX → bytecode
2. Execute on CPU RPN VM → Vec<SdfDrawCmd>
3. Upload SdfDrawCmd buffer to GPU
4. Run GPU fragment shader (SDF eval) → pixels
5. Read back pixels, verify non-black output at expected positions
```

This is sufficient to prove the format is correct — if CPU-produced SdfDrawCmds render correctly on GPU, both compute paths are compatible. Additional test configs:

| Test | Compute | Render | Purpose |
|------|---------|--------|---------|
| `test_full_gpu` | GPU | GPU | Default pipeline |
| `test_hybrid` | CPU | GPU | SdfDrawCmd format validation |
| `test_full_cpu` | CPU | CPU | Software fallback |
| `test_cpu_compute_only` | CPU | (none) | SdfDrawCmd count/content assertions |

### What needs to happen
1. **Move `SdfDrawCmd` to `matterstream-common`** (with bytemuck Pod/Zeroable)
2. **Add `UiPipeline` trait to `matterstream-common`**
3. **Update VM** to emit `SdfDrawCmd` directly in MTUI mode (replace `UiSdfDrawCmd`)
4. **Add SDF evaluation functions to `matterstream-common`** (pure math: `sd_rounded_box`, `sd_circle`, `sd_segment` — shared between CPU and shader)
5. **Rewrite `matterstream-ui-soft`** to do per-pixel SDF evaluation (replaces scan-line Rasterizer)
6. **Create `matterstream-ui-gpu` crate** implementing `UiPipeline` via wgpu
7. **Move gpu.rs, host.rs, shader WGSL files from vm to `matterstream-ui-gpu`**
8. **Update shader_render.wgsl** to match new MTUI 0x70+ opcode numbers
9. **Remove `Rasterizer` trait** from `matterstream-common`
10. **Update run-tsx example** to use `UiPipeline` trait

### SdfDrawCmd (GPU buffer format, from gpu.rs)
```rust
#[repr(C)]
struct SdfDrawCmd {
    pos: [f32; 2],      // x, y
    size: [f32; 2],      // w, h
    color: [f32; 4],     // r, g, b, a
    params: [f32; 4],    // [type, radius, softness, slot]
}
// type: 0=Box, 1=Slab, 2=Circle, 3=Line, 4=Text
```

## Compiler Updates

The compiler (`asm_compiler.rs`) currently emits 0x40-range opcodes directly. It needs to:
1. Emit `SetCR(0, FOURCC_MTUI)` before UI blocks
2. Use 0x70+ opcode numbers for all MTUI ops
3. Emit `SetCR(0, previous_mode)` when switching back

The `Asm` builder in `matterstream-vm-asm` needs new methods for the 0x70+ opcodes, or the existing methods updated to emit the new numbers.

## run-tsx Pipeline

The restored run-tsx example should:
1. Read .tsx file from CLI arg
2. Compile via `compile_to_asm()`
3. Execute on VM (produces `Vec<UiSdfDrawCmd>`)
4. Render via `Rasterizer` trait:
   - `--renderer gpu` (default if wgpu available): GPU SDF pipeline
   - `--renderer soft`: CPU softbuffer fallback
5. Display in winit window with HiDPI scaling

## Implementation Phases

### Phase A: Opcode migration (VM + compiler)
1. Add MTUI 0x70+ variants to `RpnOp` enum
2. Add to `from_u8` and `payload_size`
3. Add CR-aware dispatch in `step()`: if CR[0]==MTUI and opcode in 0x70-0x7F, same handler as 0x40+
4. Update `asm_compiler.rs` to emit 0x70+ with SetCR
5. Update `matterstream-vm-asm` builder methods
6. Keep old 0x40+ working during transition

### Phase B: GPU renderer crate
1. Create `crates/matterstream-ui-gpu/`
2. Move `gpu.rs`, `host.rs`, shader WGSL files from vm to ui-gpu
3. Implement `Rasterizer` trait for GPU backend
4. Update shader_compute.wgsl opcodes to match 0x70+ numbers
5. Add wgpu dependency

### Phase C: Restore run-tsx with TSX UI tests
1. Move `run-tsx.rs` from `_disabled/` back to `examples/`
2. Update imports to use `UiPipeline` trait (GPU default, soft fallback)
3. Restore `login_form.tsx` and other UI TSX files as test fixtures
4. Games (connect4, flappy_bird, game2048) stay in `_disabled/` — they use Rust-side rendering patterns that need a separate refactor
5. Test: `cargo run --example run-tsx -- login_form.tsx` renders correctly

### Phase D: Validation
1. `make test-all` passes (all three feature configs)
2. `cargo run --example run-tsx -- examples/login_form.tsx` renders correctly
3. GPU and softbuffer produce visually equivalent output
4. Compiler emits new 0x70+ opcodes
5. Old 0x40+ opcodes still work (transition compatibility)

## Files to create
- `docs/UI_GPU_REFACTOR.md` — this document
- `crates/matterstream-ui-gpu/Cargo.toml`
- `crates/matterstream-ui-gpu/src/lib.rs`
- `crates/matterstream-ui-gpu/src/renderer.rs`
- `crates/matterstream-ui-gpu/src/shaders/` (moved from vm)

## Files to modify
- `crates/matterstream-vm/src/rpn.rs` — add 0x70+ opcodes, CR dispatch
- `crates/matterstream-vm-asm/src/lib.rs` — update opcode emission
- `crates/matterstream-core/src/asm_compiler.rs` — emit SetCR + 0x70+ opcodes
- `crates/matterstream-vm/src/shader_compute.wgsl` — update opcode numbers
- `crates/matterstream/examples/` — restore run-tsx

## Verification
1. `make test-all` — all three feature configs pass
2. `cargo run -p matterstream --example run-tsx -- --timeout 3 crates/matterstream/examples/login_form.tsx`
3. Changing a .tsx file and re-running produces updated output
4. GPU and softbuffer renderers both work
5. OID import demo still works
