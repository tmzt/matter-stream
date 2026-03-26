# Component State + useState Design

## Context

The prompt bar needs `useState` for interactive state (mic toggle, text buffer). The banks (ScalarBank, IntBank, etc.) ARE the ComponentUniforms — the per-component GPU-uploadable state. Each component gets its own frame; frames are pushed/popped on component boundaries.

## Architecture

### Two uniform levels

**GlobalUniforms** (`@group(0)`) — shared, constant per frame:
- `time_delta`, `resolution`, `mouse`, `theme`

**ComponentUniforms** (`@group(1) @binding(0)`) — GPU-uploadable, `repr(C)`, pushed/popped:
- `scalar_bank: [[f32; 4]; 4]`       — 16 f32s
- `int_bank: [[i32; 4]; 4]`          — 16 i32s
- `vec3_bank: [[f32; 4]; 16]`        — 16 vec3s
- `vec4_bank: [[f32; 4]; 16]`        — 16 vec4s
- `zero_page: [[u32; 4]; 16]`        — 256 bytes
- `string_bank: [[u32; 2]; 256]`     — [char_offset, char_count] per slot

Same layout in Rust and WGSL. Uploaded to GPU every frame.

**ComponentLocal** — CPU-only staging, NOT uploaded:
- `strings: Vec<Option<String>>`      — 256 nullable Rust string slots
- No dirty tracking — commit is explicit, driven by bytecode
- The compiler emits a `CommitString(slot)` instruction before any text draw that references a StringBank slot
- CommitString converts ComponentLocal.strings[slot] → glyph indices → writes char_buffer → updates ComponentUniforms.string_bank[slot] offset/count
- Only referenced strings are committed — the compiler knows which ones

**char_buffer** (`@group(1) @binding(1)`, storage buffer):
- Flat `array<u32>` of glyph indices
- CPU writes after committing dirty strings from ComponentLocal
- Fragment shader reads when rendering text

Both ComponentUniforms and ComponentLocal are pushed/popped on component boundaries.

### Packed binding reference: u32 = u16 bank_type | u16 slot

Every binding (useState slot, prop, local variable) is addressed by a single u32:

```
[u16 bank_type][u16 slot_index]
```

Bank types:
- 0 = ScalarBank (f32)
- 1 = IntBank (i32)
- 2 = Vec3Bank
- 3 = Vec4Bank
- 4 = ZeroPage
- 5 = StringBank (CPU-only, not in GPU uniforms)

Example: `useState(false)` → allocates `0x0001_0000` (IntBank, slot 0).

### Component state stack

```
Enter ComponentA:
  → push current ComponentUniforms onto stack
  → zero/init new frame
  → execute ComponentA bytecode (writes to banks via useState, props)
  → SdfDrawCmds emitted read from current frame

  Enter ComponentB (child):
    → push ComponentA's frame
    → zero/init new frame for ComponentB
    → execute ComponentB
  Leave ComponentB:
    → pop → restore ComponentA's frame

Leave ComponentA:
  → pop → restore parent's frame
```

Opcodes: `PushComponentState` / `PopComponentState` (or reuse the 0x50 block ops — `DefineComponent`/`ExecComponent` already imply this).

### useState compilation

```tsx
const [listening, setListening] = useState(false);
```

Compiler:
1. Allocate next free IntBank slot in current scope → slot 0
2. Record binding: `listening` → ref `0x0001_0000`
3. Emit init: `Push32(0); StoreBank(ref)`
4. When `listening` used in ternary: `PushIfElse(ref, true_val, false_val)`
5. When `setListening(true)` called: `Push32(1); StoreBank(ref)`

PushIfElse updated to take a single packed u32 ref instead of separate bank_id + slot:

```
PushIfElse [ref: u32, true_val: u32, false_val: u32] → [result]
```

LoadBank/StoreBank also take packed ref:

```
LoadBank [ref: u32] → [value]
StoreBank [ref: u32, value] →
```

### Prompt bar with useState

```tsx
const [listening, setListening] = useState(false);

<MicButton
  color={listening ? "#4466FFFF" : "#2A2A3CFF"}
  action="toggle_listening"
/>
```

Compiles to:
```
// Init state
Push32(0)                          // false
Push32(0x00010000)                 // IntBank slot 0
StoreBank

// MicButton color
Push32(0x00010000)                 // ref
Push32(0x4466FFFF)                 // true color
Push32(0x2A2A3CFF)                 // false color
PushIfElse                         // pushes selected color
SetColor                           // MTUI op, reads from stack
```

### StringBank (GPU-uploadable via GpuString)

StringBank IS in GPU uniforms — as glyph index references, not Rust `String`s.

**ComponentUniforms** includes:
```wgsl
string_bank: array<vec2<u32>, 256>,  // [char_offset, char_count] per slot
```

**Separate storage buffer:**
```wgsl
@group(1) @binding(1) var<storage, read> char_buffer: array<u32>;
```

CPU side: writes string text → converts to glyph indices → fills char_buffer + updates string_bank offsets.

GPU side: text SDF draw command references StringBank slot via packed ref. Fragment shader reads `string_bank[slot]` → gets `(offset, count)` → iterates `char_buffer[offset..offset+count]` → looks up atlas glyph for each → draws.

The GPU doesn't validate which atlas — it just draws glyph indices. Wrong atlas = wrong glyphs, but no crash.

### Component definition split

- **DefineUserComponent** (0x54, universal, unprivileged) — creates a subcomponent scope for ordinal overflow. Same privilege as DefineBlock. No OID, no import boundary. Just a new ordinal namespace so you don't run out of 16 bank slots.

- **DefineComponent** (SystemCall 0x08, privileged) — creates a real component with OID, own security context, import/export boundary. Package-level operation.

- **ExecComponent** (0x55, universal) — enters any defined component. Pushes ComponentUniforms + ComponentLocal, executes, pops on return. Works for both user and system components.

ExecComponent handles the state push/pop — it's the component boundary. DefineUserComponent just registers the block with a fresh ordinal namespace.

TKV-style params (structured props) — future work. For now, props are stack values.

## Files to modify

- `crates/matterstream-vm/src/rpn.rs` — update LoadBank/StoreBank to accept packed ref, add PushComponentState/PopComponentState, update PushIfElse
- `crates/matterstream-vm/src/host.rs` — split GpuUniforms into GlobalUniforms + ComponentUniforms
- `crates/matterstream-vm-asm/src/lib.rs` — update bank access helpers for packed ref
- `crates/matterstream-compiler/src/asm_compiler.rs` — add useState parsing, state slot allocation
- `crates/matterstream-ui-gpu/src/shader_render.wgsl` — add `@group(1)` for ComponentUniforms
- `crates/matterstream-ui-gpu/src/lib.rs` — upload ComponentUniforms per component

## Verification

1. `prompt_bar_v2.tsx` with `useState(false)` for listening
2. MicButton color changes via PushIfElse based on IntBank slot
3. `cargo run --features compiler,ui-gpu --example run-tsx -- prompt_bar_v2.tsx`
4. `make test-all` passes all configurations
