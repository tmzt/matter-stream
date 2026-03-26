# Ribbon View + Texture Bank + Scroll Physics

## Context

Adding a horizontally scrollable card carousel (ribbon view) that requires three new capabilities:

1. **texture_bank** — general DRAW_TYPE_TEXTURE primitive for sampling offscreen textures
2. **Ribbon view** — container that scrolls child cards, clips to viewport, shows adjacent card edges
3. **Scroll physics** — momentum + friction computed in the compute stage, snap to nearest card

Key constraint: **card positioning must be instantaneous**. The scroll offset lives in `scalar_bank`, the compute stage runs physics math each frame, the shader reads the computed offset and applies it. No VM re-execution.

The active card renders live via SDF. Adjacent (partially visible) cards can optionally be offscreen textures for complex content, but for MVP all cards render via SDF with clipping.

## Architecture

```
Event loop: mouse drag → writes drag state to scalar_bank [position, velocity, target]
     ↓
prepare_frame(): reads drag state, runs momentum+friction math, writes final scroll offset
     ↓
RenderFrame.scalar_bank[N] = computed scroll position
     ↓
Shader: reads scalar_bank[N], offsets ribbon children, clips to viewport
```

The user sees: `[partial left card | full center card | partial right card]`

---

## Part 1: texture_bank infrastructure

### GpuTexture descriptor (in matterstream-common)

`crates/matterstream-common/src/sdf.rs`:
```rust
pub const DRAW_TYPE_TEXTURE: f32 = 5.0;
pub const MAX_TEXTURES: usize = 8;

/// GPU texture descriptor — stored in texture_bank, 16 bytes.
#[repr(C)]
pub struct GpuTexture {
    pub width: u32,
    pub height: u32,
    pub layer: u32,      // index into texture_2d_array
    pub flags: u32,      // format, filtering mode, etc.
}
```

### GPU bindings

Add to `crates/matterstream-ui-gpu/src/lib.rs` at binding slots 6-8:
- **@binding(6)**: `texture_2d_array<f32>` — texture array (up to MAX_TEXTURES layers)
- **@binding(7)**: `sampler` — shared linear/nearest sampler
- **@binding(8)**: `storage<read> texture_bank: array<GpuTexture>` — texture descriptors

### Shader DRAW_TYPE_TEXTURE (case 5u)

`crates/matterstream-ui-gpu/src/shader_render.wgsl`:
```wgsl
case 5u: { // Texture
    let tex_idx = u32(cmd.params.y);  // texture_bank index
    let tex = texture_bank[tex_idx];
    let half = cmd.size * 0.5;
    if abs(p.x) < half.x && abs(p.y) < half.y {
        let uv = (p + half) / cmd.size;
        let color_sample = textureSample(tex_array, tex_sampler, uv, tex.layer);
        d = -1.0;  // inside
        // blend color_sample with cmd.color tint
    }
}
```

### Offscreen texture creation

`GpuSdfRenderer` gets new methods:
- `create_offscreen_texture(device, width, height) -> (Texture, TextureView, layer_index)`
- `render_to_texture(device, queue, view, draws, ...)` — renders a subset of draw commands to an offscreen target
- Internal texture array managed by the renderer

### RenderFrame additions

`crates/matterstream-common/src/pipeline.rs`:
```rust
pub struct RenderFrame {
    // ... existing fields ...
    pub texture_bank: Vec<GpuTexture>,  // texture descriptors
}
```

### Files to modify

- `crates/matterstream-common/src/sdf.rs` — add DRAW_TYPE_TEXTURE, GpuTexture struct, MAX_TEXTURES
- `crates/matterstream-common/src/pipeline.rs` — add texture_bank to RenderFrame
- `crates/matterstream-common/src/lib.rs` — re-exports
- `crates/matterstream-ui-gpu/src/lib.rs` — texture array, sampler, binding 6-8, RTT methods
- `crates/matterstream-ui-gpu/src/shader_render.wgsl` — texture bindings, case 5u
- `crates/matterstream-ui-soft/src/lib.rs` — DRAW_TYPE_TEXTURE handling (skip or software sample)
- `crates/matterstream-ui-cpu-compute/src/lib.rs` — pass through texture_bank

---

## Part 2: Ribbon view

### Draw types

```rust
pub const DRAW_TYPE_RIBBON_BEGIN: f32 = 6.0;
pub const DRAW_TYPE_RIBBON_END: f32 = 7.0;
```

**RIBBON_BEGIN** SdfDrawCmd encoding:
- `pos` = viewport origin (x, y)
- `size` = viewport dimensions (w, h)
- `params[0]` = 6.0 (RIBBON_BEGIN)
- `params[1]` = scalar_bank slot for scroll offset (computed by physics)
- `params[2]` = direction: 0.0 = horizontal, 1.0 = vertical
- `params[3]` = card_width (for snap-point calculation in physics)

**RIBBON_END**: `params[0]` = 7.0, everything else zero.

### MTUI opcodes

```
MtuiOp::RibbonBegin = 0x0E  (wire: 0x8E), pops 7: x, y, w, h, scroll_slot, dir, card_width
MtuiOp::RibbonEnd   = 0x0F  (wire: 0x8F), pops 0
```

### Shader ribbon logic

In the fragment shader main loop, track ribbon state:
```wgsl
var in_ribbon: bool = false;
var ribbon_clip_min: vec2<f32>;
var ribbon_clip_max: vec2<f32>;
var ribbon_scroll: vec2<f32>;
```

- `case 6u` (RIBBON_BEGIN): read scroll from scalar_bank[slot], set clip + scroll state, continue
- `case 7u` (RIBBON_END): clear ribbon state, continue
- Cases 0-5 when `in_ribbon`: clip pixel to viewport, offset by scroll, eval SDF

```wgsl
var effective_pixel = pixel;
if in_ribbon {
    if pixel.x < ribbon_clip_min.x || pixel.x >= ribbon_clip_max.x ||
       pixel.y < ribbon_clip_min.y || pixel.y >= ribbon_clip_max.y {
        continue;
    }
    effective_pixel = pixel - ribbon_scroll;
}
let center = cmd.pos + cmd.size * 0.5;
let p = effective_pixel - center;
```

### TSX syntax

```tsx
<RibbonView x={0} y={60} w={360} h={400} scrollBank={0} cardWidth={360}>
  {/* Card 1 at x=0 */}
  <Slab x={10} y={10} w={340} h={380} radius={12} color="#1E1E2EFF" />
  <Text x={30} y={30} size={20} label="Card 1" color="#EEEEEEFF" />

  {/* Card 2 at x=360 */}
  <Slab x={370} y={10} w={340} h={380} radius={12} color="#1E1E2EFF" />
  <Text x={390} y={30} size={20} label="Card 2" color="#EEEEEEFF" />

  {/* Card 3 at x=720 */}
  <Slab x={730} y={10} w={340} h={380} radius={12} color="#1E1E2EFF" />
  <Text x={750} y={30} size={20} label="Card 3" color="#EEEEEEFF" />
</RibbonView>
```

User sees ~360px viewport at y=60. Cards at x=0, 360, 720. Scroll offset 0 shows card 1 centered with edges of card 2 peeking from the right. Dragging left reveals card 2.

### Compiler

`crates/matterstream-compiler/src/asm_compiler.rs`:
- `"RibbonView"` case in `emit_node()`:
  - Props: x, y, w, h, scrollBank, direction, cardWidth
  - Emits: ribbon_begin → push_state → apply_offset(x,y) → emit children → pop_state → ribbon_end

---

## Part 3: Scroll physics (momentum + friction)

### Scalar bank layout for ribbon scroll state

Each ribbon uses 4 consecutive scalar_bank slots:
- `[N+0]` = **scroll_position** (current pixel offset, written by physics)
- `[N+1]` = **scroll_velocity** (px/ms, set on drag release by event loop)
- `[N+2]` = **snap_target** (target position for nearest card, set by event loop)
- `[N+3]` = **physics_state** (0=idle, 1=dragging, 2=decelerating, 3=snapping)

### Physics in cpu-compute

`crates/matterstream-ui-cpu-compute/src/lib.rs` — add `update_ribbon_physics()`:

```rust
pub fn update_ribbon_physics(scalar_bank: &mut [f32; 16], dt_ms: f32) {
    // For each ribbon (scan for physics_state != 0):
    // state=1 (dragging): position set directly by event loop, no physics
    // state=2 (decelerating): position += velocity * dt; velocity *= friction
    //   when |velocity| < threshold → compute nearest snap target, state=3
    // state=3 (snapping): spring toward snap_target
    //   position = lerp(position, target, 1 - e^(-k*dt))
    //   when |position - target| < 0.5 → position = target, state=0
}
```

Called from `prepare_frame()` before building the RenderFrame. The shader only reads `scalar_bank[N+0]` (the computed position).

### Event loop mouse handling

`crates/matterstream/examples/run-tsx.rs`:

```rust
// State outside event loop:
let mut drag_active = false;
let mut drag_start_x: f32 = 0.0;
let mut drag_start_scroll: f32 = 0.0;
let mut last_mouse_x: f32 = 0.0;
let mut last_drag_time: f32 = 0.0;

// MouseInput Pressed → drag_active=true, capture start position
// CursorMoved while dragging → scalar_bank[N+0] = drag_start_scroll + delta_x
//   also track velocity = delta_x / delta_time
// MouseInput Released → drag_active=false
//   scalar_bank[N+1] = computed velocity
//   scalar_bank[N+2] = nearest card snap point
//   scalar_bank[N+3] = 2.0 (decelerating)
```

---

## Files to create

- `crates/matterstream/examples/cards/ribbon_demo.tsx` — test TSX with 3+ cards in a RibbonView

## Files to modify

| File | Changes |
|------|---------|
| `crates/matterstream-common/src/sdf.rs` | DRAW_TYPE_TEXTURE/RIBBON_BEGIN/RIBBON_END, GpuTexture, MAX_TEXTURES |
| `crates/matterstream-common/src/pipeline.rs` | texture_bank field on RenderFrame |
| `crates/matterstream-common/src/lib.rs` | re-exports |
| `crates/matterstream-vm-asm/src/lib.rs` | mtui::RIBBON_BEGIN/END constants, Asm helpers |
| `crates/matterstream-vm/src/rpn.rs` | MtuiOp::RibbonBegin/End + dispatch |
| `crates/matterstream-ui-gpu/src/lib.rs` | texture array, sampler, bindings 6-8, RTT |
| `crates/matterstream-ui-gpu/src/shader_render.wgsl` | texture sampling, ribbon clip+scroll |
| `crates/matterstream-ui-soft/src/lib.rs` | ribbon clip+scroll in render_frame() |
| `crates/matterstream-ui-cpu-compute/src/lib.rs` | update_ribbon_physics(), texture_bank passthrough |
| `crates/matterstream-compiler/src/asm_compiler.rs` | RibbonView tag |
| `crates/matterstream/examples/run-tsx.rs` | mouse events, physics state init |

## Implementation order

1. **texture_bank types** in common (GpuTexture, DRAW_TYPE_TEXTURE, constants)
2. **ribbon types** in common (DRAW_TYPE_RIBBON_BEGIN/END)
3. **VM opcodes** — MtuiOp + asm helpers for ribbon
4. **Shader** — texture bindings + ribbon state + clip/scroll logic
5. **GPU renderer** — texture array + sampler + bind group update + RTT methods
6. **CPU soft renderer** — ribbon clip/scroll
7. **cpu-compute** — physics math, texture_bank passthrough
8. **Compiler** — RibbonView tag
9. **Event loop** — mouse drag + physics state
10. **Demo TSX** — ribbon_demo.tsx

## Limitations (MVP)

- No nested ribbons
- texture_bank limited to 8 layers
- Physics is simple momentum+friction+snap (no overscroll bounce)
- Soft renderer skips DRAW_TYPE_TEXTURE (renders placeholder)

## Verification

1. `cargo test --workspace` — existing tests pass
2. `cargo build --features compiler,ui-gpu --example run-tsx` — builds clean
3. `cargo run --features compiler,ui-gpu --example run-tsx -- cards/ribbon_demo.tsx` — renders ribbon, drag scrolls, physics snaps
4. `cargo run --features compiler,ui-softbuffer --example run-tsx -- cards/ribbon_demo.tsx` — same on CPU
