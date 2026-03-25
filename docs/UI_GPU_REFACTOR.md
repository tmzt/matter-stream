# UI GPU Refactor — SDF Pipeline + msm1 Opcode Layout

## Bytecode format: msm1

All crates bumped to 0.2.x. Bytecode magic: `msm1`. Clean break, no backwards compatibility.

## msm1 opcode layout

```
── UNIVERSAL (0x00-0x5F) ─────────────────────────────────────

0x00-0x0F  Stack, memory, control
  0x00  Nop
  0x01  Push32          [u32]
  0x02  Push64          [u64]
  0x03  Push128         [u128]
  0x04  Dup
  0x05  Drop
  0x06  Swap
  0x07  Load
  0x08  Store
  0x09  Call
  0x0A  Ret
  0x0B  Jmp             [u64 target]
  0x0C  JmpIf           [u64 target]
  0x0D  Halt

0x10-0x1F  Integer arithmetic + bitwise
  0x10  Add
  0x11  Sub
  0x12  Mul
  0x13  Div
  0x14  Mod
  0x15  And
  0x16  Or
  0x17  Xor
  0x18  Shl
  0x19  Shr
  0x1A  Not

0x20-0x2F  Comparison (int + float)
  0x20  CmpEq
  0x21  CmpLt
  0x22  CmpGt
  0x23  CmpGe
  0x24  CmpLe
  0x25  CmpNe
  0x26  FCmpGt
  0x27  FCmpLt
  0x28  FCmpEq

0x30-0x3F  Float arithmetic (+ room for matrix ops)
  0x30  FAdd
  0x31  FSub
  0x32  FMul
  0x33  FDiv
  0x34  FNeg
  0x35  FAbs
  0x36  I2F
  0x37  F2I
  (0x38-0x3F reserved for matrix ops)

0x40-0x4F  Data: banks, dict, destructure
  0x40  LoadBank
  0x41  StoreBank
  0x42  LoadZpI32
  0x43  StoreZpI32
  0x44  LoadBankComp
  0x45  StoreBankComp
  0x48  DictNew
  0x49  DictSet
  0x4A  DictGet
  0x4C  Explode
  0x4D  ExplodeMapped

0x50-0x5F  Blocks + components
  0x50  DefineBlock     — register callable block (anonymous, no ordinals)
  0x51  CallBlock       — call a defined block (shares caller's ordinal space)
  0x52  LoopOver        [n, block_idx] — call block for each, no return values
  0x53  MapOver         [n, block_idx] — call block for each, push results
  0x54  DefineComponent — like DefineBlock but creates subpackage (own zeroeth ordinal)
  0x55  ExecComponent   — enter component's ordinal scope (CR[0] set by definer)
  (0x56-0x5F reserved for local slot table ops)

  Blocks: anonymous code at a PC offset. No ordinals. Shares caller's namespace.
  Components: subpackage with own zeroeth ordinal. Own import/export scope.

── USER ESCAPE (0x60-0x6F) — unprivileged ────────────────────

0x60  UserCall [u64 action_op] [u64 data]
        0x00  EvPoll
        0x01  EvHasEvent
        0x02  FrameCount
        0x03  Rand
        0x10  OidImport
        0x11  OidCall
        0x12  OidCosineMatch

0x61  CoprocessorCall [u64 action] [u64 length] [u64 data]
        (reserved — coprocessor system TBD)

── SYSTEM ESCAPE (0x70-0x7F) — privileged ────────────────────

0x70  SetCR [u8 cr_idx] [u64 value]
        CR[1] write checks $EXEC_PKG — violation = hard termination

0x71  SystemCall [u64 action_op] [u64 data]
        Requires CR[1] >= INTERNAL
        0x00  AtomicRead (stub)
        0x01  AtomicWrite (stub)
        0x02  AtomicRmw (stub)
        0x03  NativeHook
        0x04  CopyList
        0x05  Sync
        0x06  DefineBlock (privileged variant)
        0x07  SetOutputMode
        0x10  OidExec

── OUTPUT REGISTER PAGE (0x80-0xDF) — dispatched by CR[0] ────

  MTUI: 0x80 SetColor, 0x81 Box, 0x82 Slab, 0x83 Circle,
        0x84 Text, 0x85 PushState, 0x86 PopState,
        0x87 ApplyOffset, 0x88 Line, 0x89 TextStr,
        0x8A Action, 0x8B ApplyMatrix, 0x8C ReplaceOffset,
        0x8D ReplaceMatrix
  VQL0: 0x80 BeginQuery, 0x81 EndQuery, 0x82 Bind,
        0x83 SetField, 0x84 SetFieldStr, 0x85 Filter,
        0x86 Project, 0x87 Param
  SKLL: 0x80 SkillBegin, 0x81 SkillEnd, 0x82 SkillStep, ...
  OBJT: 0x80 ObjTypeBegin, 0x81 ObjTypeEnd, ...
  CARD: 0x80 CardBegin, 0x81 CardEnd, ...

── RESERVED (0xE0-0xFF) ──────────────────────────────────────
```

## Security model

Ring-based via CR[1] (SECURITY_REGISTER):

- **SetCR (0x70)**: always privileged. Writing CR[1] checks `$EXEC_PKG` (atomic register, immutable per-package). Violation = immediate termination.
- **SystemCall (0x71)**: requires `CR[1] >= INTERNAL`.
- **UserCall (0x60)**: always available. Read-only observation + OID lookups through import mapping.
- A `@chitin/internal` component sets `CR[1] = INTERNAL` before invoking user code, restores after. Privilege comes from the call chain.

```
CR[0] = Output Register (MTUI, VQL0, SKLL, OBJT, CARD, etc.)
CR[1] = Security Register (SANDBOXED=0x01, INTERNAL=0x02, SYSTEM=0x03)
```

## SDF pipeline

VM (CPU) always runs bytecode → emits SdfDrawCmd via OR page MTUI ops.

SdfDrawCmd is the unified repr(C) type in matterstream-common:
```rust
#[repr(C)]
pub struct SdfDrawCmd {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub params: [f32; 4],  // [type, radius, softness, slot]
}
```

No Rust code constructs draw commands — only the VM via MTUI OR ops. Rust interacts through VM-exit (NativeHook) and atomic registers.

Pipeline stages (4 combinations, all valid):

| Compute | Render | Use case |
|---------|--------|----------|
| GPU (shader_compute.wgsl) | GPU (shader_render.wgsl) | Default |
| CPU (RPN VM) | GPU (shader_render.wgsl) | Hybrid test |
| CPU (RPN VM) | CPU (SDF eval → softbuffer) | Full software fallback |
| GPU | CPU | Unlikely but valid |

## Implementation phases

### Phase A: msm1 opcode enum + dispatch
1. New RpnOp enum matching msm1 layout
2. Dispatch: universal (0x00-0x5F), user (0x60), system (0x70-0x71), OR page (0x80+)
3. OR page checks CR[0] FourCC
4. Update compiler to emit msm1
5. Bump crates to 0.2.x

### Phase B: SdfDrawCmd + pipeline
1. SdfDrawCmd in matterstream-common
2. VM emits SdfDrawCmd in MTUI OR handlers
3. SDF eval in matterstream-ui-soft
4. matterstream-ui-gpu (wgpu)
5. Restore run-tsx

### Phase C: Validation
1. make test-all
2. run-tsx renders login_form.tsx
3. GPU/CPU produce equivalent output
