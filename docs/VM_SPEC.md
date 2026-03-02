# MTSM-RPN VM Specification

## Overview

The MatterStream RPN VM (MTSM-RPN-Bincode) is a stack-based bytecode virtual machine designed for safe, metered execution of UI programs. It provides:

- **Stack-based execution** with typed values (U32, U64, FQA, OVA, Map)
- **Per-opcode gas metering** with configurable budgets and backward-jump limiting
- **Typed register banks** (Tier 1 Scalar/Int/Vec3/Vec4, Tier 2 ZeroPage) that persist between execution frames
- **Event queue** for input handling (keyboard, mouse, tick)
- **UI draw opcodes** for 2D rendering primitives
- **String table** for text rendering

## Value Types

| Type | Description | Stack representation |
|------|-------------|---------------------|
| U32 | 32-bit unsigned integer | `RpnValue::U32(u32)` |
| U64 | 64-bit unsigned integer | `RpnValue::U64(u64)` |
| Fqa | 128-bit Fully Qualified Address | `RpnValue::Fqa(Fqa)` |
| Ova | 32-bit Object Virtual Address | `RpnValue::Ova(Ova)` |
| Map | Hash map (u64 → RpnValue) | `RpnValue::Map(HashMap)` |

Arithmetic operations coerce U32 to U64 via `as_u64()`.

## Opcode Table

### Core Operations (0x00–0x06)

| Byte | Mnemonic | Payload | Stack effect | Description |
|------|----------|---------|-------------|-------------|
| 0x00 | Nop | - | (→) | No operation |
| 0x01 | Push32 | 4 bytes | (→ val) | Push 32-bit immediate |
| 0x02 | Push64 | 8 bytes | (→ val) | Push 64-bit immediate |
| 0x03 | PushFqa | 16 bytes | (→ fqa) | Push 128-bit FQA |
| 0x04 | Dup | - | (a → a a) | Duplicate top of stack |
| 0x05 | Drop | - | (a →) | Discard top of stack |
| 0x06 | Swap | - | (a b → b a) | Swap top two values |

### Arithmetic (0x07–0x0A, 0x16)

| Byte | Mnemonic | Stack effect | Description |
|------|----------|-------------|-------------|
| 0x07 | Add | (a b → a+b) | Wrapping addition (u64) |
| 0x08 | Sub | (a b → a-b) | Wrapping subtraction (u64) |
| 0x09 | Mul | (a b → a*b) | Wrapping multiplication (u64) |
| 0x0A | Div | (a b → a/b) | Integer division (errors on b=0) |
| 0x16 | Mod | (a b → a%b) | Integer modulo (errors on b=0) |

### Memory (0x0B–0x0C)

| Byte | Mnemonic | Stack effect | Description |
|------|----------|-------------|-------------|
| 0x0B | Load | (ova → val) | Load 4 bytes from arena at OVA |
| 0x0C | Store | (val ova →) | Store 4 bytes to arena at OVA |

### Control Flow (0x0D–0x0F, 0x13–0x15)

| Byte | Mnemonic | Payload | Stack effect | Description |
|------|----------|---------|-------------|-------------|
| 0x0D | Call | - | (target →) | Call: push return addr, jump to target |
| 0x0E | Ret | - | (→) | Return: pop call stack, jump back |
| 0x0F | Sync | - | (→) | Sync arena (ping-pong swap) |
| 0x13 | Jmp | 8 bytes | (→) | Unconditional jump to inline u64 target |
| 0x14 | JmpIf | 8 bytes | (cond →) | Jump if cond ≠ 0 |
| 0x15 | Halt | - | (→) | Stop execution |

### Maps (0x10–0x12)

| Byte | Mnemonic | Stack effect | Description |
|------|----------|-------------|-------------|
| 0x10 | MapNew | (→ map) | Create empty map |
| 0x11 | MapSet | (map key val → map) | Insert key-value pair |
| 0x12 | MapGet | (map key → val) | Look up key (0 if missing) |

### Comparisons (0x17–0x19, 0x22–0x24)

| Byte | Mnemonic | Stack effect | Description |
|------|----------|-------------|-------------|
| 0x17 | CmpEq | (a b → a==b) | Equal (pushes 1 or 0) |
| 0x18 | CmpLt | (a b → a<b) | Less than |
| 0x19 | CmpGt | (a b → a>b) | Greater than |
| 0x22 | CmpGe | (a b → a>=b) | Greater or equal |
| 0x23 | CmpLe | (a b → a<=b) | Less or equal |
| 0x24 | CmpNe | (a b → a!=b) | Not equal |

### Bitwise (0x1A–0x1F)

| Byte | Mnemonic | Stack effect | Description |
|------|----------|-------------|-------------|
| 0x1A | And | (a b → a&b) | Bitwise AND |
| 0x1B | Or | (a b → a\|b) | Bitwise OR |
| 0x1C | Xor | (a b → a^b) | Bitwise XOR |
| 0x1D | Shl | (a n → a<<n) | Shift left |
| 0x1E | Shr | (a n → a>>n) | Shift right |
| 0x1F | Not | (a → !a) | Logical NOT (0→1, nonzero→0) |

### Typed Bank Access (0x20–0x21)

| Byte | Mnemonic | Stack effect | Description |
|------|----------|-------------|-------------|
| 0x20 | LoadBank | (bank slot → value) | Load from typed register bank |
| 0x21 | StoreBank | (value bank slot →) | Store to typed register bank |

**Bank IDs:**
- 0 = Scalar (f32, 16 slots)
- 1 = Int (i32, 16 slots)
- 2 = Vec3 ([f32; 3], 16 slots)
- 3 = Vec4 ([f32; 4], 16 slots)
- 4 = ZeroPage (u8, 256 slots)

### UI Draw (0x40–0x49)

| Byte | Mnemonic | Stack effect | Description |
|------|----------|-------------|-------------|
| 0x40 | UiSetColor | (rgba →) | Set current draw color (0xRRGGBBAA) |
| 0x41 | UiBox | (x y w h →) | Draw filled rectangle |
| 0x42 | UiSlab | (x y w h radius →) | Draw rounded rectangle |
| 0x43 | UiCircle | (cx cy r →) | Draw filled circle |
| 0x44 | UiText | (x y size slot →) | Draw text (placeholder rect) |
| 0x45 | UiPushState | (→) | Save draw state (color + offset) |
| 0x46 | UiPopState | (→) | Restore draw state |
| 0x47 | UiSetOffset | (dx dy →) | Set translation offset |
| 0x48 | UiLine | (x1 y1 x2 y2 →) | Draw line segment |
| 0x49 | UiTextStr | (x y size str_idx →) | Draw text with string table index |

### Event & Runtime (0x50–0x53)

| Byte | Mnemonic | Stack effect | Description |
|------|----------|-------------|-------------|
| 0x50 | EvPoll | (→ data type) | Pop next event (type=0 if none) |
| 0x51 | EvHasEvent | (→ flag) | 1 if events pending, 0 otherwise |
| 0x52 | FrameCount | (→ count) | Push current frame number |
| 0x53 | Rand | (max → value) | Random u32 in [0, max) |

## Gas Metering

Every opcode consumes gas proportional to its cost category:

| Category | Default cost | Opcodes |
|----------|-------------|---------|
| Nop | 1 | Nop, Halt |
| Push | 1 | Push32, Push64, PushFqa |
| Stack | 1 | Dup, Drop, Swap |
| Arithmetic | 2 | Add, Sub, Mul, Div, Mod |
| Memory | 10 | Load, Store |
| Call | 5 | Call, Ret |
| Sync | 100 | Sync |
| Map | 5 | MapNew, MapSet, MapGet |
| Jump | 2 | Jmp, JmpIf |
| Compare | 2 | CmpEq, CmpLt, CmpGt, CmpGe, CmpLe, CmpNe |
| Bitwise | 2 | And, Or, Xor, Shl, Shr, Not |
| Bank | 3 | LoadBank, StoreBank |
| Event | 5 | EvPoll, EvHasEvent, FrameCount, Rand |
| UI | 5 | All UI opcodes |

Default budget: 10,000,000 gas units. Default backward jump limit: 10,000.

## Typed Register Banks (Tier 1/2 Memory)

Banks persist between `execute()` calls — they are NOT cleared. This enables frame-based state:

| Bank | Type | Slots | Use case |
|------|------|-------|----------|
| Scalar | f32 | 16 | Continuous values (opacity, size) |
| Int | i32 | 16 | Discrete values (score, player, state) |
| Vec3 | [f32; 3] | 16 | Positions, velocities |
| Vec4 | [f32; 4] | 16 | Colors, compound state |
| ZeroPage | u8 | 256 | Arrays, grids (board cells) |

## Event System

Events are queued and consumed via `EvPoll`. Event types:

| Code | Type | Data format |
|------|------|-------------|
| 0 | None | 0 |
| 1 | KeyDown | key_code as u64 |
| 2 | KeyUp | key_code as u64 |
| 3 | MouseDown | (x << 32) \| y |
| 4 | MouseUp | (x << 32) \| y |
| 5 | MouseMove | (x << 32) \| y |
| 6 | Tick | dt_ms as u64 |

## Execution Model

Frame-based re-entry via `VmHost::tick()`:

1. Push Tick event + transfer pending events to VM
2. Increment frame counter
3. Execute logic bytecode (CPU RPN VM)
4. Sync VM typed banks → GpuUniforms
5. Upload uniforms to GPU
6. GPU compute shader interprets render bytecode → draw list
7. GPU render pipeline draws instances
8. Present

## String Table

Populated before execution. `UiTextStr` references strings by index. The assembler allocates string IDs via `def_string()` and includes the table in `AsmOutput`.

## Limits

- Stack depth: 256
- Call stack depth: unlimited (gas-metered)
- Cycle limit: 1,000,000 (per execution)
- UI draw commands: 4,096
- UI state stack: 16
- Zero page: 256 bytes
