⏺ It was just pushing a u128 immediate — no special FQA behavior. Push128 is the right name.
  ⎿  User rejected Claude's plan:
     ╭─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
     │ UI GPU Refactor — SDF Compute Pipeline + CR-Based Opcodes                                                                                                                                       │
     │                                                                                                                                                                                                 │
     │ Context                                                                                                                                                                                         │
     │                                                                                                                                                                                                 │
     │ matter-stream has two rendering paths that need unification:                                                                                                                                    │
     │ 1. CPU softbuffer (matterstream-ui-soft) — scan-line rasterizer, the current default                                                                                                            │
     │ 2. GPU SDF pipeline (cardgpu5 + existing shader_compute.wgsl/shader_render.wgsl in vm) — compute shader executes bytecode VM, builds SDF node tree, fragment shader does per-pixel evaluation   │
     │                                                                                                                                                                                                 │
     │ The GPU path already exists in crates/matterstream-vm/src/ (shader_compute.wgsl, shader_render.wgsl, gpu.rs, host.rs) but isn't wired up as the primary rendering path. Meanwhile, UI opcodes   │
     │ are stuck in the universal 0x40 range from the old flat design instead of living in the CR-dependent MTUI page.                                                                                 │
     │                                                                                                                                                                                                 │
     │ Goals:                                                                                                                                                                                          │
     │ 1. Move all UI draw ops from 0x40-0x4D to CR-dependent MTUI page at 0x70+                                                                                                                       │
     │ 2. Deprecate the old 0x40 opcode numbers (transition period, then remove)                                                                                                                       │
     │ 3. Wire up the GPU SDF pipeline as the primary renderer via Rasterizer trait                                                                                                                    │
     │ 4. Get run-tsx working so any .tsx file can be tested end-to-end                                                                                                                                │
     │ 5. The compiler must work for this pipeline                                                                                                                                                     │
     │                                                                                                                                                                                                 │
     │ Why no UI ops should remain universal                                                                                                                                                           │
     │                                                                                                                                                                                                 │
     │ Every current 0x40-0x4D opcode is MTUI-specific:                                                                                                                                                │
     │ - UiSetColor — sets draw state color (MTUI concept)                                                                                                                                             │
     │ - UiBox/Slab/Circle/Line/Text/TextStr/Action — emit draw commands (MTUI output)                                                                                                                 │
     │ - UiPushState/PopState — manage MTUI draw state stack                                                                                                                                           │
     │ - UiApplyOffset/Matrix/ReplaceOffset/ReplaceMatrix — MTUI transform stack                                                                                                                       │
     │                                                                                                                                                                                                 │
     │ None of these have meaning outside the MTUI CR mode. A VQL query doesn't need UiSetColor. A Skill definition doesn't push draw commands. The only reason they're at 0x40 is historical — the    │
     │ flat opcode list predates the CR system.                                                                                                                                                        │
     │                                                                                                                                                                                                 │
     │ Defense of universality would require: an opcode that's used by multiple CR modes. None of the UI ops qualify. Even UiPushState/UiPopState are specific to the MTUI draw state stack, not       │
     │ general-purpose state management.                                                                                                                                                               │
     │                                                                                                                                                                                                 │
     │ Naming clarification: RpnOp vs UiOp                                                                                                                                                             │
     │                                                                                                                                                                                                 │
     │ RpnOp — the math/evaluation stack machine opcodes (0x00-0x6F). Arithmetic, control flow, bank access, OID imports. These are RPN operations.                                                    │
     │                                                                                                                                                                                                 │
     │ UiOp — the MTUI CR-dependent page (0x70+). Draw operations that produce SdfDrawCmd. These are UI operations, not RPN math. Currently misnamed as RpnOp::UiSetColor etc. — they belong in a      │
     │ separate UiOp enum or as a CR-dispatched sub-table.                                                                                                                                             │
     │                                                                                                                                                                                                 │
     │ The VM step() function dispatches:                                                                                                                                                              │
     │ - 0x00-0x6F → RpnOp handler (always available, universal)                                                                                                                                       │
     │ - 0x70-0xEF → CR-dependent page (dispatched by CR[0])                                                                                                                                           │
     │ - 0xF0 → SystemVMAction (privileged, CR_SECURITY_MODE gated)                                                                                                                                    │
     │                                                                                                                                                                                                 │
     │ Bytecode format: msm1                                                                                                                                                                           │
     │                                                                                                                                                                                                 │
     │ All crates bumped to 0.2.x. Bytecode magic changes from MTSM to msm1 (matter-stream machine v1). No backwards compatibility — clean break.                                                      │
     │                                                                                                                                                                                                 │
     │ Full opcode layout                                                                                                                                                                              │
     │                                                                                                                                                                                                 │
     │ ── UNIVERSAL (always available) ──────────────────────────────                                                                                                                                  │
     │                                                                                                                                                                                                 │
     │ 0x00-0x06  Stack + immediates                                                                                                                                                                   │
     │   0x00 Nop                                                                                                                                                                                      │
     │   0x01 Push32          [u32]                                                                                                                                                                    │
     │   0x02 Push64          [u64]                                                                                                                                                                    │
     │   0x03 Push128         [u128]                                                                                                                                                                   │
     │   0x04 Dup                                                                                                                                                                                      │
     │   0x05 Drop                                                                                                                                                                                     │
     │   0x06 Swap                                                                                                                                                                                     │
     │                                                                                                                                                                                                 │
     │ 0x07-0x0B  Integer arithmetic                                                                                                                                                                   │
     │   0x07 Add                                                                                                                                                                                      │
     │   0x08 Sub                                                                                                                                                                                      │
     │   0x09 Mul                                                                                                                                                                                      │
     │   0x0A Div                                                                                                                                                                                      │
     │   0x0B Mod                                                                                                                                                                                      │
     │                                                                                                                                                                                                 │
     │ 0x0C-0x0D  Memory                                                                                                                                                                               │
     │   0x0C Load                                                                                                                                                                                     │
     │   0x0D Store                                                                                                                                                                                    │
     │                                                                                                                                                                                                 │
     │ 0x0E-0x14  Control flow                                                                                                                                                                         │
     │   0x0E Call                                                                                                                                                                                     │
     │   0x0F Ret                                                                                                                                                                                      │
     │   0x10 Jmp             [u64 target]                                                                                                                                                             │
     │   0x11 JmpIf           [u64 target]                                                                                                                                                             │
     │   0x12 Halt                                                                                                                                                                                     │
     │                                                                                                                                                                                                 │
     │ 0x13-0x18  Comparison                                                                                                                                                                           │
     │   0x13 CmpEq                                                                                                                                                                                    │
     │   0x14 CmpLt                                                                                                                                                                                    │
     │   0x15 CmpGt                                                                                                                                                                                    │
     │   0x16 CmpGe                                                                                                                                                                                    │
     │   0x17 CmpLe                                                                                                                                                                                    │
     │   0x18 CmpNe                                                                                                                                                                                    │
     │                                                                                                                                                                                                 │
     │ 0x19-0x1E  Bitwise                                                                                                                                                                              │
     │   0x19 And                                                                                                                                                                                      │
     │   0x1A Or                                                                                                                                                                                       │
     │   0x1B Xor                                                                                                                                                                                      │
     │   0x1C Shl                                                                                                                                                                                      │
     │   0x1D Shr                                                                                                                                                                                      │
     │   0x1E Not                                                                                                                                                                                      │
     │                                                                                                                                                                                                 │
     │ 0x20-0x27  Bank access                                                                                                                                                                          │
     │   0x20 LoadBank                                                                                                                                                                                 │
     │   0x21 StoreBank                                                                                                                                                                                │
     │   0x22 LoadZpI32                                                                                                                                                                                │
     │   0x23 StoreZpI32                                                                                                                                                                               │
     │   0x24 LoadBankComp                                                                                                                                                                             │
     │   0x25 StoreBankComp                                                                                                                                                                            │
     │   (0x26-0x27 reserved)                                                                                                                                                                          │
     │                                                                                                                                                                                                 │
     │ 0x30-0x3C  Float arithmetic                                                                                                                                                                     │
     │   0x30 FAdd                                                                                                                                                                                     │
     │   0x31 FSub                                                                                                                                                                                     │
     │   0x32 FMul                                                                                                                                                                                     │
     │   0x33 FDiv                                                                                                                                                                                     │
     │   0x34 FCmpGt                                                                                                                                                                                   │
     │   0x35 FCmpLt                                                                                                                                                                                   │
     │   0x36 FCmpEq                                                                                                                                                                                   │
     │   0x37 FNeg                                                                                                                                                                                     │
     │   0x38 FAbs                                                                                                                                                                                     │
     │   0x39 I2F                                                                                                                                                                                      │
     │   0x3A F2I                                                                                                                                                                                      │
     │   0x3B Explode                                                                                                                                                                                  │
     │   0x3C ExplodeMapped                                                                                                                                                                            │
     │                                                                                                                                                                                                 │
     │ 0x50-0x5F  Basic block operations                                                                                                                                                               │
     │   0x50 MapOver         [n, block_idx] → [...] (allocated, stub)                                                                                                                                 │
     │   (0x51-0x5F reserved)                                                                                                                                                                          │
     │                                                                                                                                                                                                 │
     │ 0x60-0x6F  OID/import ops                                                                                                                                                                       │
     │   0x60 OidPush          [u128 oid]                                                                                                                                                              │
     │   0x61 OidImport                                                                                                                                                                                │
     │   0x62 OidCall                                                                                                                                                                                  │
     │   0x63 OidExec                                                                                                                                                                                  │
     │   0x64 OidCosineMatch                                                                                                                                                                           │
     │   0x65 ExecContainer                                                                                                                                                                            │
     │   0x66 DefineComponent                                                                                                                                                                          │
     │   (0x67-0x6F reserved)                                                                                                                                                                          │
     │                                                                                                                                                                                                 │
     │ 0x70-0x72  Dictionary (HashMap) — universal, always available                                                                                                                                   │
     │   0x70 DictNew                                                                                                                                                                                  │
     │   0x71 DictSet                                                                                                                                                                                  │
     │   0x72 DictGet                                                                                                                                                                                  │
     │                                                                                                                                                                                                 │
     │   (0x73-0x7F reserved)                                                                                                                                                                          │
     │                                                                                                                                                                                                 │
     │ ── OUTPUT REGISTER PAGE (dispatched by CR[0]) ────────────────                                                                                                                                  │
     │                                                                                                                                                                                                 │
     │ 0x80-0xDF  OR-dependent — same byte means different op per mode                                                                                                                                 │
     │   MTUI: 0x80 SetColor, 0x81 Box, 0x82 Slab, 0x83 Circle,                                                                                                                                        │
     │         0x84 Text, 0x85 PushState, 0x86 PopState,                                                                                                                                               │
     │         0x87 ApplyOffset, 0x88 Line, 0x89 TextStr,                                                                                                                                              │
     │         0x8A Action, 0x8B ApplyMatrix, 0x8C ReplaceOffset,                                                                                                                                      │
     │         0x8D ReplaceMatrix                                                                                                                                                                      │
     │   VQL0: 0x80 BeginQuery, 0x81 EndQuery, 0x82 Bind,                                                                                                                                              │
     │         0x83 SetField, 0x84 SetFieldStr, 0x85 Filter,                                                                                                                                           │
     │         0x86 Project, 0x87 Param                                                                                                                                                                │
     │   SKLL: 0x80 SkillBegin, 0x81 SkillEnd, 0x82 SkillStep, ...                                                                                                                                     │
     │   OBJT: 0x80 ObjTypeBegin, 0x81 ObjTypeEnd, ...                                                                                                                                                 │
     │   CARD: 0x80 CardBegin, 0x81 CardEnd, ...                                                                                                                                                       │
     │                                                                                                                                                                                                 │
     │ ── UNPRIVILEGED ESCAPE ───────────────────────────────────────                                                                                                                                  │
     │                                                                                                                                                                                                 │
     │ 0xE0       UserCall [u64 action_op] [u64 data] — no ring check                                                                                                                                  │
     │              0x00 EvPoll                                                                                                                                                                        │
     │              0x01 EvHasEvent                                                                                                                                                                    │
     │              0x02 FrameCount                                                                                                                                                                    │
     │              0x03 Rand                                                                                                                                                                          │
     │                                                                                                                                                                                                 │
     │ ── PRIVILEGED ESCAPE ─────────────────────────────────────────                                                                                                                                  │
     │                                                                                                                                                                                                 │
     │ 0xF0       SetCR [u8 cr_idx] [u64 value]                                                                                                                                                        │
     │              Writes any CR register. Always privileged.                                                                                                                                         │
     │              Writing CR[1] (SECURITY_REGISTER) checks $EXEC_PKG:                                                                                                                                │
     │                - Setting to INTERNAL: $EXEC_PKG must be @chitin/internal+                                                                                                                       │
     │                - Setting to SYSTEM: $EXEC_PKG must be @chitin/system                                                                                                                            │
     │                - Violation = immediate termination (hard stop)                                                                                                                                  │
     │                                                                                                                                                                                                 │
     │ 0xF1       SystemCall [u64 action_op] [u64 data]                                                                                                                                                │
     │              Requires CR[1] >= INTERNAL.                                                                                                                                                        │
     │              0x00 AtomicRead                                                                                                                                                                    │
     │              0x01 AtomicWrite                                                                                                                                                                   │
     │              0x02 AtomicRmw                                                                                                                                                                     │
     │              0x03 NativeHook                                                                                                                                                                    │
     │              0x04 CopyList                                                                                                                                                                      │
     │              0x05 Sync (arena swap)                                                                                                                                                             │
     │              0x06 DefineBlock                                                                                                                                                                   │
     │              0x07 SetOutputMode (writes CR[0])                                                                                                                                                  │
     │                                                                                                                                                                                                 │
     │ Opcode Migration: 0x40 → 0x70 (MTUI CR page)                                                                                                                                                    │
     │                                                                                                                                                                                                 │
     │ New MTUI opcode table (when CR[0] == FOURCC_MTUI)                                                                                                                                               │
     │                                                                                                                                                                                                 │
     │ ┌────────────┬────────────┬───────────────┬──────────────────────┐                                                                                                                              │
     │ │ Old (0x4x) │ New (0x7x) │     Name      │     Stack effect     │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x40       │ 0x70       │ SetColor      │ [rgba] →             │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x41       │ 0x71       │ Box           │ [x,y,w,h] →          │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x42       │ 0x72       │ Slab          │ [x,y,w,h,r] →        │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x43       │ 0x73       │ Circle        │ [cx,cy,r] →          │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x44       │ 0x74       │ Text          │ [x,y,size,slot] →    │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x45       │ 0x75       │ PushState     │ →                    │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x46       │ 0x76       │ PopState      │ →                    │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x47       │ 0x77       │ ApplyOffset   │ [dx,dy] →            │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x48       │ 0x78       │ Line          │ [x1,y1,x2,y2] →      │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x49       │ 0x79       │ TextStr       │ [x,y,size,str_idx] → │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x4A       │ 0x7A       │ Action        │ [x,y,w,h,str_idx] →  │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x4B       │ 0x7B       │ ApplyMatrix   │ [16 floats] →        │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x4C       │ 0x7C       │ ReplaceOffset │ [dx,dy] →            │                                                                                                                              │
     │ ├────────────┼────────────┼───────────────┼──────────────────────┤                                                                                                                              │
     │ │ 0x4D       │ 0x7D       │ ReplaceMatrix │ [16 floats] →        │                                                                                                                              │
     │ └────────────┴────────────┴───────────────┴──────────────────────┘                                                                                                                              │
     │                                                                                                                                                                                                 │
     │ Transition strategy                                                                                                                                                                             │
     │                                                                                                                                                                                                 │
     │ - Old 0x40-0x4D opcodes remain in from_u8 during transition (mapped to same handler)                                                                                                            │
     │ - New bytecode emitted by compiler uses SetCR(MTUI) + 0x70+ opcodes                                                                                                                             │
     │ - The VM step() function checks: if CR[0]==MTUI and opcode in 0x70-0x7F, dispatch to MTUI handler                                                                                               │
     │ - Eventually remove 0x40-0x4D from from_u8                                                                                                                                                      │
     │                                                                                                                                                                                                 │
     │ GPU SDF Pipeline Integration                                                                                                                                                                    │
     │                                                                                                                                                                                                 │
     │ Architecture (from cardgpu5)                                                                                                                                                                    │
     │                                                                                                                                                                                                 │
     │ TSX source                                                                                                                                                                                      │
     │   → compiler (asm_compiler.rs) → bytecode + string_table                                                                                                                                        │
     │   → GPU compute shader (shader_compute.wgsl) → SdfDrawCmd buffer (SDF nodes)                                                                                                                    │
     │   → GPU fragment shader (shader_render.wgsl) → per-pixel SDF evaluation → pixels                                                                                                                │
     │   → present pass → screen                                                                                                                                                                       │
     │                                                                                                                                                                                                 │
     │ What already exists in matter-stream                                                                                                                                                            │
     │                                                                                                                                                                                                 │
     │ - crates/matterstream-vm/src/shader_compute.wgsl — bytecode VM interpreter on GPU                                                                                                               │
     │ - crates/matterstream-vm/src/shader_render.wgsl — SDF fragment shader                                                                                                                           │
     │ - crates/matterstream-vm/src/gpu.rs — SdfDrawCmd struct, build_draw_list_from_ui_draws()                                                                                                        │
     │ - crates/matterstream-vm/src/host.rs — GpuUniforms, VmHost with tick loop                                                                                                                       │
     │                                                                                                                                                                                                 │
     │ Architecture: VM → SDF DrawList → Renderer                                                                                                                                                      │
     │                                                                                                                                                                                                 │
     │ The drawing primitives ARE SDF primitives (Slab = rounded box SDF, Circle = circle SDF, etc.). Both renderers evaluate the same SDFs — the GPU does it per-pixel in a fragment shader, the CPU  │
     │ does it per-pixel in a loop.                                                                                                                                                                    │
     │                                                                                                                                                                                                 │
     │ TSX → compiler → bytecode                                                                                                                                                                       │
     │                     ↓                                                                                                                                                                           │
     │               CPU RPN VM (always)                                                                                                                                                               │
     │                     ↓                                                                                                                                                                           │
     │          SetCR(MTUI) → MTUI ops emit SdfDrawCmd entries                                                                                                                                         │
     │                     ↓                                                                                                                                                                           │
     │               Vec<SdfDrawCmd> (SDF primitives, repr(C))                                                                                                                                         │
     │               /              \                                                                                                                                                                  │
     │     GPU pipeline           CPU pipeline                                                                                                                                                         │
     │     (upload buffer,        (iterate pixels,                                                                                                                                                     │
     │      frag shader evals      eval same SDF math,                                                                                                                                                 │
     │      SDF per-pixel)         write to softbuffer)                                                                                                                                                │
     │                                                                                                                                                                                                 │
     │ Unified type: SdfDrawCmd                                                                                                                                                                        │
     │                                                                                                                                                                                                 │
     │ One type everywhere. Lives in matterstream-common (zero deps, GPU-uploadable):                                                                                                                  │
     │                                                                                                                                                                                                 │
     │ #[repr(C)]                                                                                                                                                                                      │
     │ #[derive(Clone, Copy)]                                                                                                                                                                          │
     │ pub struct SdfDrawCmd {                                                                                                                                                                         │
     │     pub pos: [f32; 2],      // x, y                                                                                                                                                             │
     │     pub size: [f32; 2],     // w, h (or x2,y2 for lines)                                                                                                                                        │
     │     pub color: [f32; 4],    // RGBA 0.0-1.0                                                                                                                                                     │
     │     pub params: [f32; 4],   // [type, radius, softness, slot]                                                                                                                                   │
     │ }                                                                                                                                                                                               │
     │ // type: 0=Box, 1=Slab, 2=Circle, 3=Line, 4=Text                                                                                                                                                │
     │                                                                                                                                                                                                 │
     │ UiSdfDrawCmd enum is removed. The VM emits SdfDrawCmd directly from MTUI opcode handlers. No Rust code ever constructs draw commands — UI is authored in TSX, compiled to bytecode, executed by │
     │  the VM. The SdfDrawCmd::box_cmd() etc. constructors are removed.                                                                                                                               │
     │                                                                                                                                                                                                 │
     │ Rust interacts with the VM world only through:                                                                                                                                                  │
     │ - VM-exit (native hooks via OID) — push/pop stack values, never construct UI                                                                                                                    │
     │ - Atomic read via atomic register file — renderer reads SdfDrawCmd buffer pointer+length without VM cooperation                                                                                 │
     │ - Atomic write (semaphore) — VM signals "draw list ready", renderer polls on it                                                                                                                 │
     │                                                                                                                                                                                                 │
     │ This gives lock-free producer/consumer: VM fills SdfDrawCmd buffer, atomically publishes, renderer reads and renders. No mutex, no message passing.                                             │
     │                                                                                                                                                                                                 │
     │ Required for this refactor — the prompt bar needs sub-frame latency for voice input control. Can't wait for a full VM tick.                                                                     │
     │                                                                                                                                                                                                 │
     │ 0x50 range redesign: UserVMAction + SystemVMAction                                                                                                                                              │
     │                                                                                                                                                                                                 │
     │ Two escape opcodes with the same format but different security levels. Security check is at the opcode level — no per-action checking needed.                                                   │
     │                                                                                                                                                                                                 │
     │ Format (both opcodes)                                                                                                                                                                           │
     │                                                                                                                                                                                                 │
     │ [opcode: u8] [action_op: u64] [data: u64]                                                                                                                                                       │
     │ Payload: 17 bytes. The action_op u64 gives unlimited sub-action space.                                                                                                                          │
     │                                                                                                                                                                                                 │
     │ UserVMAction (0x50) — always available, no security check                                                                                                                                       │
     │                                                                                                                                                                                                 │
     │ ┌───────────┬────────────┬──────┬────────────────┐                                                                                                                                              │
     │ │ action_op │    Name    │ data │     Stack      │                                                                                                                                              │
     │ ├───────────┼────────────┼──────┼────────────────┤                                                                                                                                              │
     │ │ 0x00      │ EvPoll     │ 0    │ → [data, type] │                                                                                                                                              │
     │ ├───────────┼────────────┼──────┼────────────────┤                                                                                                                                              │
     │ │ 0x01      │ EvHasEvent │ 0    │ → [bool]       │                                                                                                                                              │
     │ ├───────────┼────────────┼──────┼────────────────┤                                                                                                                                              │
     │ │ 0x02      │ FrameCount │ 0    │ → [u64]        │                                                                                                                                              │
     │ ├───────────┼────────────┼──────┼────────────────┤                                                                                                                                              │
     │ │ 0x03      │ Rand       │ 0    │ → [u32]        │                                                                                                                                              │
     │ └───────────┴────────────┴──────┴────────────────┘                                                                                                                                              │
     │                                                                                                                                                                                                 │
     │ UserVMAction is strictly read-only observation. No state mutation.                                                                                                                              │
     │                                                                                                                                                                                                 │
     │ SystemVMAction (0xF0) — requires CR_SECURITY_MODE >= INTERNAL, faults otherwise                                                                                                                 │
     │                                                                                                                                                                                                 │
     │ Highest range for most privileged ops. 0xFx = system/privileged.                                                                                                                                │
     │                                                                                                                                                                                                 │
     │ ┌───────────┬───────────────┬─────────────┬─────────────────────┬───────────────────────────────────────────────────────────────────┐                                                           │
     │ │ action_op │     Name      │    data     │        Stack        │                             Min level                             │                                                           │
     │ ├───────────┼───────────────┼─────────────┼─────────────────────┼───────────────────────────────────────────────────────────────────┤                                                           │
     │ │ 0x00      │ AtomicRead    │ slot_idx    │ → [u64]             │ Internal                                                          │                                                           │
     │ ├───────────┼───────────────┼─────────────┼─────────────────────┼───────────────────────────────────────────────────────────────────┤                                                           │
     │ │ 0x01      │ AtomicWrite   │ slot_idx    │ [val] →             │ Internal                                                          │                                                           │
     │ ├───────────┼───────────────┼─────────────┼─────────────────────┼───────────────────────────────────────────────────────────────────┤                                                           │
     │ │ 0x02      │ AtomicRmw     │ slot_idx    │ [xor_mask, shift] → │ Internal                                                          │                                                           │
     │ ├───────────┼───────────────┼─────────────┼─────────────────────┼───────────────────────────────────────────────────────────────────┤                                                           │
     │ │ 0x03      │ NativeHook    │ dispatch_id │ (varies)            │ Internal                                                          │                                                           │
     │ ├───────────┼───────────────┼─────────────┼─────────────────────┼───────────────────────────────────────────────────────────────────┤                                                           │
     │ │ 0x04      │ SetCR         │ cr_idx      │ [val] →             │ System — writes any CR including CR_SECURITY_MODE                 │                                                           │
     │ ├───────────┼───────────────┼─────────────┼─────────────────────┼───────────────────────────────────────────────────────────────────┤                                                           │
     │ │ 0x08      │ SetOutputMode │ fourcc      │ →                   │ Internal — sets OR (CR[0]) only. Switches between MTUI/VQL0/SKLL. │                                                           │
     │ ├───────────┼───────────────┼─────────────┼─────────────────────┼───────────────────────────────────────────────────────────────────┤                                                           │
     │ │ 0x05      │ CopyList      │ slot_idx    │ [len] →             │ Internal                                                          │                                                           │
     │ ├───────────┼───────────────┼─────────────┼─────────────────────┼───────────────────────────────────────────────────────────────────┤                                                           │
     │ │ 0x06      │ Sync          │ 0           │ →                   │ Internal                                                          │                                                           │
     │ └───────────┴───────────────┴─────────────┴─────────────────────┴───────────────────────────────────────────────────────────────────┘                                                           │
     │                                                                                                                                                                                                 │
     │ RpnOp::SetCR => {                                                                                                                                                                               │
     │     let cr_idx = /* from payload */;                                                                                                                                                            │
     │     let value = self.pop_u64()?;                                                                                                                                                                │
     │     if cr_idx == CR_SECURITY_REGISTER {                                                                                                                                                         │
     │         // Ring escalation — check $EXEC_PKG, not current ring                                                                                                                                  │
     │         let required = if value >= SECURITY_SYSTEM {                                                                                                                                            │
     │             SecurityMode::System                                                                                                                                                                │
     │         } else {                                                                                                                                                                                │
     │             SecurityMode::Internal                                                                                                                                                              │
     │         };                                                                                                                                                                                      │
     │         if self.exec_pkg_security < required {                                                                                                                                                  │
     │             // HARD TERMINATION — not a recoverable error                                                                                                                                       │
     │             return Err(RpnError::SecurityTermination);                                                                                                                                          │
     │         }                                                                                                                                                                                       │
     │     }                                                                                                                                                                                           │
     │     self.cr_bank[cr_idx] = value as u32;                                                                                                                                                        │
     │ }                                                                                                                                                                                               │
     │                                                                                                                                                                                                 │
     │ RpnOp::SystemCall => {                                                                                                                                                                          │
     │     if self.cr_bank[CR_SECURITY_REGISTER] < SECURITY_INTERNAL {                                                                                                                                 │
     │         return Err(RpnError::SecurityViolation);                                                                                                                                                │
     │     }                                                                                                                                                                                           │
     │     // dispatch action_op...                                                                                                                                                                    │
     │ }                                                                                                                                                                                               │
     │                                                                                                                                                                                                 │
     │ RpnOp::UserCall => {                                                                                                                                                                            │
     │     // no check, always allowed                                                                                                                                                                 │
     │     // dispatch action_op...                                                                                                                                                                    │
     │ }                                                                                                                                                                                               │
     │                                                                                                                                                                                                 │
     │ $EXEC_PKG is the security mode of the package that contains the currently executing bytecode, set by the loader based on OID prefix. It's immutable — cannot be changed by any opcode. Stored   │
     │ as vm.exec_pkg_security: SecurityMode.                                                                                                                                                          │
     │                                                                                                                                                                                                 │
     │ MapOver (0x52) — universal, not privileged                                                                                                                                                      │
     │                                                                                                                                                                                                 │
     │ ┌────────┬─────────┬────────────────────────┬───────────────────────────────────────────────────────────────────────────────────────────┐                                                       │
     │ │ Opcode │  Name   │      Stack effect      │                                        Description                                        │                                                       │
     │ ├────────┼─────────┼────────────────────────┼───────────────────────────────────────────────────────────────────────────────────────────┤                                                       │
     │ │ 0x52   │ MapOver │ [n, block_idx] → [...] │ Pop n values, execute basic block for each, push results. Allocated, not yet implemented. │                                                       │
     │ └────────┴─────────┴────────────────────────┴───────────────────────────────────────────────────────────────────────────────────────────┘                                                       │
     │                                                                                                                                                                                                 │
     │ MapOver itself is safe — it reads a pre-defined block from the block table and iterates. The block table is populated by the loader/compiler, not by guest bytecode.                            │
     │                                                                                                                                                                                                 │
     │ Basic block table                                                                                                                                                                               │
     │                                                                                                                                                                                                 │
     │ A fixed array of u32 PC offsets into current bytecode. Defining blocks is privileged (SystemVMAction sub-op) because it controls what code can be jumped to.                                    │
     │                                                                                                                                                                                                 │
     │ pub block_table: [u32; 256],  // bytecode PC offsets, populated by loader                                                                                                                       │
     │ pub block_count: u32,                                                                                                                                                                           │
     │                                                                                                                                                                                                 │
     │ ┌───────────────────┬─────────────┬───────────┬─────────────────────────────────────────────────────────────────┐                                                                               │
     │ │ SystemVMAction op │    Name     │   data    │                           Description                           │                                                                               │
     │ ├───────────────────┼─────────────┼───────────┼─────────────────────────────────────────────────────────────────┤                                                                               │
     │ │ 0x07              │ DefineBlock │ block_idx │ [pc_offset] → — register a basic block entry point. Privileged. │                                                                               │
     │ └───────────────────┴─────────────┴───────────┴─────────────────────────────────────────────────────────────────┘                                                                               │
     │                                                                                                                                                                                                 │
     │ The atomic register file is a fixed-size array of AtomicU64 slots on the VM, accessible from both bytecode and Rust (via &AtomicU64 references). All atomic ops are u64-only — no length-based  │
     │ access.                                                                                                                                                                                         │
     │                                                                                                                                                                                                 │
     │ CR_SECURITY_MODE register                                                                                                                                                                       │
     │                                                                                                                                                                                                 │
     │ VMExit operations are gated by CR[1] (CR_SECURITY_MODE). This is set by the package loader based on OID security mode and cannot be changed by sandboxed code.                                  │
     │                                                                                                                                                                                                 │
     │ ┌──────────────────┬──────────────────────┬────────────┬───────┬───────────────┬──────────┬────────────────────────┐                                                                            │
     │ │ CR_SECURITY_MODE │ AtomicRead/Write/Rmw │ NativeHook │ SetCR │ SetOutputMode │ CopyList │ EvPoll/Rand/FrameCount │                                                                            │
     │ ├──────────────────┼──────────────────────┼────────────┼───────┼───────────────┼──────────┼────────────────────────┤                                                                            │
     │ │ System (0x03)    │ Yes                  │ Yes        │ Yes   │ Yes           │ Yes      │ Yes                    │                                                                            │
     │ ├──────────────────┼──────────────────────┼────────────┼───────┼───────────────┼──────────┼────────────────────────┤                                                                            │
     │ │ Internal (0x02)  │ Yes                  │ Yes        │ No    │ Yes           │ Yes      │ Yes                    │                                                                            │
     │ ├──────────────────┼──────────────────────┼────────────┼───────┼───────────────┼──────────┼────────────────────────┤                                                                            │
     │ │ Sandboxed (0x01) │ No                   │ No         │ No    │ No            │ No       │ Yes                    │                                                                            │
     │ └──────────────────┴──────────────────────┴────────────┴───────┴───────────────┴──────────┴────────────────────────┘                                                                            │
     │                                                                                                                                                                                                 │
     │ Security model — ring-based via CR[1]:                                                                                                                                                          │
     │                                                                                                                                                                                                 │
     │ - CR[1] (SECURITY_REGISTER) determines the current ring. Most SystemVMAction ops just check CR[1] >= required_level.                                                                            │
     │ - SetCR is always privileged and is the only opcode with granular checks: writing CR[1] requires System level (ring escalation). Writing other CRs (including CR[0]/output mode) requires       │
     │ Internal.                                                                                                                                                                                       │
     │ - A @chitin/internal component sets CR[1] = INTERNAL before invoking user code, establishing the ring. The user code inherits that authorization for the call duration. The component restores  │
     │ CR[1] = SANDBOXED after return.                                                                                                                                                                 │
     │ - Most operations (atomics, SetOutputMode, NativeHook) are safe when called through a trusted @chitin component that implements proper prerequisites and invariants. The privilege comes from   │
     │ the call chain, not the opcode.                                                                                                                                                                 │
     │                                                                                                                                                                                                 │
     │ UserVMAction (0x50) is the exception — always available regardless of ring. Read-only observation only.                                                                                         │
     │                                                                                                                                                                                                 │
     │ Attempting a privileged escape op without the required security mode faults immediately (RpnError::SecurityViolation).                                                                          │
     │                                                                                                                                                                                                 │
     │ CR_SECURITY_MODE is set by the package loader, not by bytecode. Based on OID prefix:                                                                                                            │
     │ - @chitin/system (1.1.1.3) → SYSTEM                                                                                                                                                             │
     │ - @chitin/internal (1.1.1.2) → INTERNAL                                                                                                                                                         │
     │ - Everything else → SANDBOXED                                                                                                                                                                   │
     │                                                                                                                                                                                                 │
     │ SetCR on CR[1] itself requires System mode — sandboxed/internal code cannot escalate. The register is effectively host-controlled.                                                              │
     │                                                                                                                                                                                                 │
     │ pub const CR_SECURITY_MODE: usize = 1;                                                                                                                                                          │
     │ pub const SECURITY_SANDBOXED: u32 = 0x01;                                                                                                                                                       │
     │ pub const SECURITY_INTERNAL: u32 = 0x02;                                                                                                                                                        │
     │ pub const SECURITY_SYSTEM: u32 = 0x03;                                                                                                                                                          │
     │                                                                                                                                                                                                 │
     │ Shared draw buffer design                                                                                                                                                                       │
     │                                                                                                                                                                                                 │
     │ // In matterstream-common                                                                                                                                                                       │
     │ pub struct DrawBuffer {                                                                                                                                                                         │
     │     pub cmds: *mut SdfDrawCmd,          // pre-allocated fixed-size buffer                                                                                                                      │
     │     pub count: AtomicU32,            // number of valid commands                                                                                                                                │
     │     pub ready: AtomicBool,           // semaphore: VM sets true when frame is complete                                                                                                          │
     │     pub capacity: u32,               // MAX_DRAW_CMDS (4096)                                                                                                                                    │
     │ }                                                                                                                                                                                               │
     │                                                                                                                                                                                                 │
     │ - VM thread: fills cmds[0..n], stores count, sets ready = true                                                                                                                                  │
     │ - Render thread: spins/polls on ready, reads count + cmds[0..count], renders, sets ready = false                                                                                                │
     │ - Input thread (prompt bar): reads/writes voice state atomics directly via atomic register file — bypasses VM cycle entirely                                                                    │
     │                                                                                                                                                                                                 │
     │ The atomic register file is a flat array of AtomicU64 slots that both VM bytecode and Rust code can access. Voice toggle, recording state, waveform data are all atomic register file entries.  │
     │                                                                                                                                                                                                 │
     │ Action regions (hit testing metadata) are not SdfDrawCmds — they go in a separate Vec<ActionRegion> on the VM.                                                                                  │
     │                                                                                                                                                                                                 │
     │ Draw type constants:                                                                                                                                                                            │
     │ pub const DRAW_TYPE_BOX: f32 = 0.0;                                                                                                                                                             │
     │ pub const DRAW_TYPE_SLAB: f32 = 1.0;                                                                                                                                                            │
     │ pub const DRAW_TYPE_CIRCLE: f32 = 2.0;                                                                                                                                                          │
     │ pub const DRAW_TYPE_LINE: f32 = 3.0;                                                                                                                                                            │
     │ pub const DRAW_TYPE_TEXT: f32 = 4.0;                                                                                                                                                            │
     │                                                                                                                                                                                                 │
     │ Rasterizer trait goes away                                                                                                                                                                      │
     │                                                                                                                                                                                                 │
     │ The Rasterizer trait (draw_rect, draw_circle, etc.) is replaced. Both renderers evaluate SDFs directly. The CPU softbuffer rasterizer is rewritten to do per-pixel SDF evaluation matching the  │
     │ shader:                                                                                                                                                                                         │
     │                                                                                                                                                                                                 │
     │ // For each pixel (px, py):                                                                                                                                                                     │
     │ //   for each SdfDrawCmd:                                                                                                                                                                       │
     │ //     d = sdf_eval(cmd, px, py)  // same math as shader                                                                                                                                        │
     │ //     if d < 0: blend color                                                                                                                                                                    │
     │                                                                                                                                                                                                 │
     │ This produces pixel-identical output between GPU and CPU paths.                                                                                                                                 │
     │                                                                                                                                                                                                 │
     │ UiPipeline trait                                                                                                                                                                                │
     │                                                                                                                                                                                                 │
     │ // In matterstream-common                                                                                                                                                                       │
     │ pub trait UiPipeline {                                                                                                                                                                          │
     │     fn render(&mut self, draws: &[SdfDrawCmd], string_table: &[String]);                                                                                                                        │
     │     fn resize(&mut self, width: u32, height: u32);                                                                                                                                              │
     │ }                                                                                                                                                                                               │
     │                                                                                                                                                                                                 │
     │ Pipeline stage matrix                                                                                                                                                                           │
     │                                                                                                                                                                                                 │
     │ Each stage has a GPU and CPU implementation. All four combinations are valid:                                                                                                                   │
     │                                                                                                                                                                                                 │
     │ ┌─────────────────────────────────┬──────────────────────────────┬─────────────────────────────────────┐                                                                                        │
     │ │ Compute (bytecode → SdfDrawCmd) │ Render (SdfDrawCmd → pixels) │              Use case               │                                                                                        │
     │ ├─────────────────────────────────┼──────────────────────────────┼─────────────────────────────────────┤                                                                                        │
     │ │ GPU (shader_compute.wgsl)       │ GPU (shader_render.wgsl)     │ Default, full GPU                   │                                                                                        │
     │ ├─────────────────────────────────┼──────────────────────────────┼─────────────────────────────────────┤                                                                                        │
     │ │ CPU (RPN VM)                    │ GPU (shader_render.wgsl)     │ Compute fallback, GPU render        │                                                                                        │
     │ ├─────────────────────────────────┼──────────────────────────────┼─────────────────────────────────────┤                                                                                        │
     │ │ CPU (RPN VM)                    │ CPU (SDF eval → softbuffer)  │ Full software fallback (CI, old HW) │                                                                                        │
     │ ├─────────────────────────────────┼──────────────────────────────┼─────────────────────────────────────┤                                                                                        │
     │ │ GPU (shader_compute.wgsl)       │ CPU (SDF eval → softbuffer)  │ Unlikely but valid                  │                                                                                        │
     │ └─────────────────────────────────┴──────────────────────────────┴─────────────────────────────────────┘                                                                                        │
     │                                                                                                                                                                                                 │
     │ Compute stage:                                                                                                                                                                                  │
     │ - GPU: shader_compute.wgsl — bytecode VM interpreter on GPU                                                                                                                                     │
     │ - CPU: RPN VM (matterstream-vm) — same bytecode, same SdfDrawCmd output                                                                                                                         │
     │                                                                                                                                                                                                 │
     │ Render stage (vertex + fragment):                                                                                                                                                               │
     │ - GPU: shader_render.wgsl — per-pixel SDF evaluation in fragment shader                                                                                                                         │
     │ - CPU: per-pixel SDF evaluation loop, writes to softbuffer (matterstream-ui-soft)                                                                                                               │
     │                                                                                                                                                                                                 │
     │ The CPU compute path (RPN VM) already exists. The GPU compute path exists in shader_compute.wgsl. Both produce SdfDrawCmd buffer — the render stage doesn't care which produced it.             │
     │                                                                                                                                                                                                 │
     │ Hybrid pipeline test                                                                                                                                                                            │
     │                                                                                                                                                                                                 │
     │ The critical integration test: CPU compute → GPU render. This validates SdfDrawCmd format correctness across the CPU/GPU boundary.                                                              │
     │                                                                                                                                                                                                 │
     │ Test: hybrid_cpu_compute_gpu_render                                                                                                                                                             │
     │ 1. Compile TSX → bytecode                                                                                                                                                                       │
     │ 2. Execute on CPU RPN VM → Vec<SdfDrawCmd>                                                                                                                                                      │
     │ 3. Upload SdfDrawCmd buffer to GPU                                                                                                                                                              │
     │ 4. Run GPU fragment shader (SDF eval) → pixels                                                                                                                                                  │
     │ 5. Read back pixels, verify non-black output at expected positions                                                                                                                              │
     │                                                                                                                                                                                                 │
     │ This is sufficient to prove the format is correct — if CPU-produced SdfDrawCmds render correctly on GPU, both compute paths are compatible. Additional test configs:                            │
     │                                                                                                                                                                                                 │
     │ ┌───────────────────────┬─────────┬────────┬─────────────────────────────────────┐                                                                                                              │
     │ │         Test          │ Compute │ Render │               Purpose               │                                                                                                              │
     │ ├───────────────────────┼─────────┼────────┼─────────────────────────────────────┤                                                                                                              │
     │ │ test_full_gpu         │ GPU     │ GPU    │ Default pipeline                    │                                                                                                              │
     │ ├───────────────────────┼─────────┼────────┼─────────────────────────────────────┤                                                                                                              │
     │ │ test_hybrid           │ CPU     │ GPU    │ SdfDrawCmd format validation        │                                                                                                              │
     │ ├───────────────────────┼─────────┼────────┼─────────────────────────────────────┤                                                                                                              │
     │ │ test_full_cpu         │ CPU     │ CPU    │ Software fallback                   │                                                                                                              │
     │ ├───────────────────────┼─────────┼────────┼─────────────────────────────────────┤                                                                                                              │
     │ │ test_cpu_compute_only │ CPU     │ (none) │ SdfDrawCmd count/content assertions │                                                                                                              │
     │ └───────────────────────┴─────────┴────────┴─────────────────────────────────────┘                                                                                                              │
     │                                                                                                                                                                                                 │
     │ What needs to happen                                                                                                                                                                            │
     │                                                                                                                                                                                                 │
     │ 1. Move SdfDrawCmd to matterstream-common (with bytemuck Pod/Zeroable)                                                                                                                          │
     │ 2. Add UiPipeline trait to matterstream-common                                                                                                                                                  │
     │ 3. Update VM to emit SdfDrawCmd directly in MTUI mode (replace UiSdfDrawCmd)                                                                                                                    │
     │ 4. Add SDF evaluation functions to matterstream-common (pure math: sd_rounded_box, sd_circle, sd_segment — shared between CPU and shader)                                                       │
     │ 5. Rewrite matterstream-ui-soft to do per-pixel SDF evaluation (replaces scan-line Rasterizer)                                                                                                  │
     │ 6. Create matterstream-ui-gpu crate implementing UiPipeline via wgpu                                                                                                                            │
     │ 7. Move gpu.rs, host.rs, shader WGSL files from vm to matterstream-ui-gpu                                                                                                                       │
     │ 8. Update shader_render.wgsl to match new MTUI 0x70+ opcode numbers                                                                                                                             │
     │ 9. Remove Rasterizer trait from matterstream-common                                                                                                                                             │
     │ 10. Update run-tsx example to use UiPipeline trait                                                                                                                                              │
     │                                                                                                                                                                                                 │
     │ SdfDrawCmd (GPU buffer format, from gpu.rs)                                                                                                                                                     │
     │                                                                                                                                                                                                 │
     │ #[repr(C)]                                                                                                                                                                                      │
     │ struct SdfDrawCmd {                                                                                                                                                                             │
     │     pos: [f32; 2],      // x, y                                                                                                                                                                 │
     │     size: [f32; 2],      // w, h                                                                                                                                                                │
     │     color: [f32; 4],     // r, g, b, a                                                                                                                                                          │
     │     params: [f32; 4],    // [type, radius, softness, slot]                                                                                                                                      │
     │ }                                                                                                                                                                                               │
     │ // type: 0=Box, 1=Slab, 2=Circle, 3=Line, 4=Text                                                                                                                                                │
     │                                                                                                                                                                                                 │
     │ Compiler Updates                                                                                                                                                                                │
     │                                                                                                                                                                                                 │
     │ The compiler (asm_compiler.rs) currently emits 0x40-range opcodes directly. It needs to:                                                                                                        │
     │ 1. Emit SetCR(0, FOURCC_MTUI) before UI blocks                                                                                                                                                  │
     │ 2. Use 0x70+ opcode numbers for all MTUI ops                                                                                                                                                    │
     │ 3. Emit SetCR(0, previous_mode) when switching back                                                                                                                                             │
     │                                                                                                                                                                                                 │
     │ The Asm builder in matterstream-vm-asm needs new methods for the 0x70+ opcodes, or the existing methods updated to emit the new numbers.                                                        │
     │                                                                                                                                                                                                 │
     │ run-tsx Pipeline                                                                                                                                                                                │
     │                                                                                                                                                                                                 │
     │ The restored run-tsx example should:                                                                                                                                                            │
     │ 1. Read .tsx file from CLI arg                                                                                                                                                                  │
     │ 2. Compile via compile_to_asm()                                                                                                                                                                 │
     │ 3. Execute on VM (produces Vec<UiSdfDrawCmd>)                                                                                                                                                   │
     │ 4. Render via Rasterizer trait:                                                                                                                                                                 │
     │   - --renderer gpu (default if wgpu available): GPU SDF pipeline                                                                                                                                │
     │   - --renderer soft: CPU softbuffer fallback                                                                                                                                                    │
     │ 5. Display in winit window with HiDPI scaling                                                                                                                                                   │
     │                                                                                                                                                                                                 │
     │ Implementation Phases                                                                                                                                                                           │
     │                                                                                                                                                                                                 │
     │ Phase A0: VMExit opcode + atomic atomic register file                                                                                                                                           │
     │                                                                                                                                                                                                 │
     │ 1. Add VMExit = 0x50 to RpnOp with 17-byte payload (u64 escape_op + u64 data)                                                                                                                   │
     │ 2. Add atomic register file: pub jump_table: [AtomicU64; 256] on RpnVm                                                                                                                          │
     │ 3. Implement escape ops: AtomicRead, AtomicWrite, AtomicRmw                                                                                                                                     │
     │ 4. Migrate EvPoll/EvHasEvent/FrameCount/Rand/SetCR to VMExit escape ops                                                                                                                         │
     │ 5. Keep old 0x50-0x54 opcodes as aliases during transition                                                                                                                                      │
     │ 6. Allocate MapOver at 0x51 (stub — returns unimplemented error)                                                                                                                                │
     │ 7. Tests: atomic read/write roundtrip, RMW xor+shift, atomic register file visibility from Rust                                                                                                 │
     │                                                                                                                                                                                                 │
     │ Phase A1: Opcode migration (VM + compiler)                                                                                                                                                      │
     │                                                                                                                                                                                                 │
     │ 1. Add MTUI 0x70+ variants — CR-aware dispatch                                                                                                                                                  │
     │ 2. Add to from_u8 and payload_size                                                                                                                                                              │
     │ 3. step() checks CR[0] for 0x70-0x7F range, dispatches to MTUI/VQL/SKLL handler                                                                                                                 │
     │ 4. Update asm_compiler.rs to emit SetCR(MTUI) + 0x70+ opcodes                                                                                                                                   │
     │ 5. Update matterstream-vm-asm builder methods                                                                                                                                                   │
     │ 6. Keep old 0x40+ working during transition                                                                                                                                                     │
     │                                                                                                                                                                                                 │
     │ Phase B: GPU renderer crate                                                                                                                                                                     │
     │                                                                                                                                                                                                 │
     │ 1. Create crates/matterstream-ui-gpu/                                                                                                                                                           │
     │ 2. Move gpu.rs, host.rs, shader WGSL files from vm to ui-gpu                                                                                                                                    │
     │ 3. Implement Rasterizer trait for GPU backend                                                                                                                                                   │
     │ 4. Update shader_compute.wgsl opcodes to match 0x70+ numbers                                                                                                                                    │
     │ 5. Add wgpu dependency                                                                                                                                                                          │
     │                                                                                                                                                                                                 │
     │ Phase C: Restore run-tsx with TSX UI tests                                                                                                                                                      │
     │                                                                                                                                                                                                 │
     │ 1. Move run-tsx.rs from _disabled/ back to examples/                                                                                                                                            │
     │ 2. Update imports to use UiPipeline trait (GPU default, soft fallback)                                                                                                                          │
     │ 3. Restore login_form.tsx and other UI TSX files as test fixtures                                                                                                                               │
     │ 4. Games (connect4, flappy_bird, game2048) stay in _disabled/ — they use Rust-side rendering patterns that need a separate refactor                                                             │
     │ 5. Test: cargo run --example run-tsx -- login_form.tsx renders correctly                                                                                                                        │
     │                                                                                                                                                                                                 │
     │ Phase D: Validation                                                                                                                                                                             │
     │                                                                                                                                                                                                 │
     │ 1. make test-all passes (all three feature configs)                                                                                                                                             │
     │ 2. cargo run --example run-tsx -- examples/login_form.tsx renders correctly                                                                                                                     │
     │ 3. GPU and softbuffer produce visually equivalent output                                                                                                                                        │
     │ 4. Compiler emits new 0x70+ opcodes                                                                                                                                                             │
     │ 5. Old 0x40+ opcodes still work (transition compatibility)                                                                                                                                      │
     │                                                                                                                                                                                                 │
     │ Files to create                                                                                                                                                                                 │
     │                                                                                                                                                                                                 │
     │ - docs/UI_GPU_REFACTOR.md — this document                                                                                                                                                       │
     │ - crates/matterstream-ui-gpu/Cargo.toml                                                                                                                                                         │
     │ - crates/matterstream-ui-gpu/src/lib.rs                                                                                                                                                         │
     │ - crates/matterstream-ui-gpu/src/renderer.rs                                                                                                                                                    │
     │ - crates/matterstream-ui-gpu/src/shaders/ (moved from vm)                                                                                                                                       │
     │                                                                                                                                                                                                 │
     │ Files to modify                                                                                                                                                                                 │
     │                                                                                                                                                                                                 │
     │ - crates/matterstream-vm/src/rpn.rs — add 0x70+ opcodes, CR dispatch                                                                                                                            │
     │ - crates/matterstream-vm-asm/src/lib.rs — update opcode emission                                                                                                                                │
     │ - crates/matterstream-core/src/asm_compiler.rs — emit SetCR + 0x70+ opcodes                                                                                                                     │
     │ - crates/matterstream-vm/src/shader_compute.wgsl — update opcode numbers                                                                                                                        │
     │ - crates/matterstream/examples/ — restore run-tsx                                                                                                                                               │
     │                                                                                                                                                                                                 │
     │ Verification                                                                                                                                                                                    │
     │                                                                                                                                                                                                 │
     │ 1. make test-all — all three feature configs pass                                                                                                                                               │
     │ 2. cargo run -p matterstream --example run-tsx -- --timeout 3 crates/matterstream/examples/login_form.tsx                                                                                       │
     │ 3. Changing a .tsx file and re-running produces updated output                                                                                                                                  │
     │ 4. GPU and softbuffer renderers both work                                                                                                                                                       │
     │ 5. OID import demo still works                                                                                                                                                                  │
     ╰─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

❯ we need to move Explode* let's move those to 0x8x

⏺ But 0x80+ is the OR page (CR[0] dispatched). Explode/ExplodeMapped are universal — they shouldn't be mode-dependent.

  Options in the universal range:
  - 0x26-0x2F (reserved after bank access)
  - 0x3B-0x4F (after float ops, legacy UI deleted)
  - 0x73-0x7F (after Dict)

⏺ User declined to answer questions

❯ actually, since we are fixing this let's fix it the way it was supposed to be. split it at MSB. move the user ops and system ops to the two blocks below the split

⏺ MSB split at 0x80:

  - 0x00-0x7F: universal ops (MSB=0)
  - 0x80-0xFF: everything else (MSB=1)
    - 0x80-0xBF: OR page (CR[0] dispatched)
    - 0xC0-0xDF: (reserved)
    - 0xE0: UserCall
    - 0xF0: SetCR
    - 0xF1: SystemCall

  But you said "two blocks below the split" — so UserCall and SystemCall move into the 0x00-0x7F range?

  Or did you mean: split at 0x80, and 0x80+ is divided into user block and system block?

  0x00-0x7F: computation (universal RPN ops)
  0x80-0xBF: OR page (output register, CR[0] dispatched)
  0xC0-0xDF: UserCall block (unprivileged escapes)
  0xE0-0xFF: SystemCall block (privileged escapes, SetCR)

  That puts the privilege escalation at the very top of the address space, with OR in the middle.

⏺ Updated plan
  ⎿  Error editing file

  Searched for 2 patterns (ctrl+o to expand)
  ⎿  Interrupted · What should Claude do instead?

❯ no, 0x6x for user and 0x7x for system

  Read 1 file (ctrl+o to expand)
