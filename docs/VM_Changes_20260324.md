# OID-Based Import System

## Context

matter-stream currently has no import/module system. All TSX examples are self-contained single files, and the MTSM archive format packages bytecode but cannot reference other packages. The goal is to add an OID (Object Identifier) addressing scheme that enables fast runtime lookup of inter-package imports — Components, Symbols, Concepts, and Hooks (both TS and native Rust).

The OID system runs **parallel** to the existing FQA→Ordinal→ASLR→OVA resolution path. FQA handles intra-package identity; OID handles inter-package imports.

---

## OID Address Layout

- **u128** stored as two u64 halves (`hi`, `lo`)
- **MSB of each u64 is reserved** (always 0) for VDBE/SQLite varint compatibility
- **63 usable bits per half** → 126 total usable bits → **63 two-bit segments**
- Each segment = 2 bits = **4-way branching** at each level
- Lookup: binary search on sorted u128s — no hashing, no trie needed

```
hi (u64): [0 reserved][seg_0:2][seg_1:2]...[seg_30:2][1 unused bit]
lo (u64): [0 reserved][seg_31:2][seg_32:2]...[seg_61:2][1 unused bit]
```

## OID Tree Structure

Well-known roots define the namespace hierarchy:

```
1           OID_ROOT
1.1         OID_PKG_ROOT
1.1.1       OID_PKG_ROOT_CHT         (@chitin/)
1.1.1.1     OID_PKG_ROOT_CHT_PUBLIC  (@chitin/[pkgpath])
1.1.1.2     OID_PKG_ROOT_CHT_INTERNAL (@chitin/internal)
1.1.1.3     OID_PKG_ROOT_CHT_SYSTEM  (@chitin/system)
1.1.2       OID_PKG_ROOT_PUBLIC      (public package tree)
```

### Security modes by subtree

| Subtree | VM-escape | Full CR | Notes |
|---------|-----------|---------|-------|
| `1.1.1.3` (system) | Yes | Yes | Full privileges |
| `1.1.1.2` (internal) | Yes | No | Can call native hooks, no CR switching |
| `1.1.1.1` (public CHT) | No | No | Sandboxed |
| `1.1.2` (public packages) | No | No | Sandboxed |

### Public package encoding (`1.1.2.*`)

Format: `1.1.2.[octal count × 2][3-bit aligned hash of a-z-. reverse DNS name][custom package OID]`

The reverse DNS name (e.g., `com.example.widgets`) is hashed and packed into 3-bit aligned segments. The octal count prefix encodes the hash length. This format may change if it would overflow the 63×2 address scheme.

## Import Kind (separate type tag)

```rust
#[repr(u8)]
pub enum ImportKind {
    Component = 0x01,  // TSX component
    Symbol    = 0x02,  // Named symbol (function, constant)
    Concept   = 0x03,  // Embedding concept (semantic/vector)
    Hook      = 0x04,  // React-like hook (TS or native)
}
```

Not encoded in the OID — carried alongside as a tag.

## Hook Return Values & Bind Model

Hooks return **N values** (e.g., `useState` → 2: a read-only bind + setter). The compiler and VM handle these with an SSA-like ordinal slot system:

- **Ordinal slots**: Each bound value from a hook return gets an ordinal within the component/subpackage boundary. These ordinals are the canonical identity of the bound value.
- **Bitmap destructuring**: When a caller destructures a hook return (`const [x, _, z] = useMyHook()`), a bitmap marks which values are used (1) vs discarded (0). Used values are allocated in order into the subpackage's slot space.
- **Pass-by-reference**: Passing a bound value as a parameter references the same ordinal slot — no copying. UI ops also reference these same slots for their data source.
- **SSA semantics**: Each bound value has exactly one definition point. The rules act like SSA, making the compiler→VM mapping very efficient.

This same ordinal + bitmap model applies to **object values (TKV)**: each property/subproperty is sorted lexicographically and addressed as an ordinal within the object. When a caller accesses an object-value, a bitmap marks which properties are used — unused properties are skipped, and the used ones are bound in ordinal order through the subpackage boundary. Same SSA-like efficiency as hook returns.

## InstanceRef: 3×u64 Object Memory Address

For OIDs representing object types in the memory system, a third u64 component acts as the index (array index, object-value property ordinal, or rowid).

```rust
/// 3×u64 address: Oid (2×u64) + ordinal index (1×u64).
/// Same VDBE rule: MSB=0 on each u64 component → 3×63 = 189 usable bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceRef {
    pub oid: Oid,       // which object type (2×u64)
    pub index: u64,     // row/element/property ordinal (MSB always 0)
}
```

`Oid` remains 2×u64 for package/import addressing. `InstanceRef` extends it to 3×u64 for addressing into object memory. The third component's meaning depends on context: array index, property ordinal (lexicographic), or rowid.

---

## Implementation Phases

### Phase 0: RPN consolidation

`matterstream-core/src/rpn.rs` is a strict subset of `matterstream-vm/src/rpn.rs` — it's missing bitwise ops, typed bank access, float arithmetic, event polling, CR bank, VQL, skills, cards, and object types. Same for `ui_vm.rs`.

**Goal:** Delete core's stale copies and have core re-export from vm.

**Modified:**
- `crates/matterstream-core/src/rpn.rs` — delete, replace with `pub use matterstream_vm::rpn::*;` re-export
- `crates/matterstream-core/src/ui_vm.rs` — delete, replace with `pub use matterstream_vm::ui_vm::*;` re-export
- `crates/matterstream-core/src/lib.rs` — update module declarations
- `crates/matterstream-core/Cargo.toml` — add `matterstream-vm` as dependency (may need to be behind `compiler` feature if it creates a cycle; check dep graph: core already depends on vm-arena and vm-addressing, and vm depends on vm-arena and vm-addressing but NOT on core, so core→vm is safe)

**Verify:** `cargo test` passes across workspace.

### Phase 1: Oid type + OidIndex (`matterstream-vm-addressing`)

**New files:**
- `crates/matterstream-vm-addressing/src/oid.rs` — `Oid` struct, well-known root constants, segment extraction, u128 round-trip, VDBE validation, `SecurityMode`, `ImportKind`, `OidTarget`, `OidEntry`
- `crates/matterstream-vm-addressing/src/oid_index.rs` — `OidIndex<'a>` zero-copy wrapper over `&[u8]`, binary search, prefix range scan, serialization (writing sorted entries)

**Modified:**
- `crates/matterstream-vm-addressing/src/lib.rs` — re-export new modules
- `crates/matterstream-vm-addressing/src/fqa.rs` — add `Osym` and `Odat` FourCC variants

**Oid well-known constants:**
```rust
impl Oid {
    pub const ROOT: Oid = ...;                  // 1
    pub const PKG_ROOT: Oid = ...;              // 1.1
    pub const PKG_ROOT_CHT: Oid = ...;          // 1.1.1
    pub const PKG_ROOT_CHT_PUBLIC: Oid = ...;   // 1.1.1.1
    pub const PKG_ROOT_CHT_INTERNAL: Oid = ...; // 1.1.1.2
    pub const PKG_ROOT_CHT_SYSTEM: Oid = ...;   // 1.1.1.3
    pub const PKG_ROOT_PUBLIC: Oid = ...;        // 1.1.2
}
```

**SecurityMode derived from OID prefix:**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityMode {
    System,    // 1.1.1.3 — VM-escape + full CR
    Internal,  // 1.1.1.2 — VM-escape only
    Sandboxed, // everything else — no VM-escape, no CR
}

impl Oid {
    pub fn security_mode(&self) -> SecurityMode { ... }
}
```

**No in-memory trie.** The `.osym` binary IS the index. OIDs are fixed-width u128s with the hierarchy encoded in the bits, so a sorted flat array gives:
- **Point lookups:** binary search on u128 (two u64 compares per step)
- **Prefix queries:** range scan — e.g., "everything under `1.1.1.2`" = find first match on top N bits, scan forward
- **Security checks:** compare top bits against well-known prefixes, no lookup needed

**`.osym` binary format** (sorted by OID, operated on directly — zero deserialization):

- Header: `[count: u32][reserved: 4 bytes]` = 8 bytes
- Fixed-width entries sorted by OID, **48 bytes each**:

```
[oid_hi: u64][oid_lo: u64][kind: u8][pad: 7 bytes][val_hi: u64][val_lo: u64][val_idx: u64]
 ─── 16 bytes (OID key) ──  ── 8 bytes (tag) ──  ──── 24 bytes (3×u64 value, VDBE-safe) ────
```

Fixed 48-byte stride makes binary search trivial: entry `i` is at offset `8 + (i * 48)`.

**Value field interpretation by ImportKind:**

| Kind | val_hi + val_lo | val_idx |
|------|----------------|---------|
| Component | FQA (u128) | 0 |
| Symbol | FQA (u128) | 0 |
| Hook | FQA (u128) | 0 |
| Concept | FQA of `.odat` member | 0 |
| NativeHook | 0 | dispatch_id (system/internal only) |

The value is always a **u128 FQA** (val_hi + val_lo). `val_idx` is reserved (0) except for NativeHook. OID is the inter-package address; the FQA feeds into the existing FQA→ASLR→OVA resolution path. For Concepts, the FQA locates the `.odat` member; the `.odat` rows are self-contained (embedding + trailing OID per row) so no row index is needed in the `.osym`.

**`.odat` embedding format:**

Header includes a model FourCC that determines the embedding dimensions and stride:

```
[MTSM: 4][Odat: 4][model: 4 FourCC][format: 4 FourCC][count: u32][sort: u8][pad: zero to stride]
[row 0: stride bytes]
[row 1: stride bytes]
...
```

**Sort order flag** (`sort` byte):
- `0x00` — sorted by trailing OID (for forward lookup by OID)
- `0x01` — unsorted / insertion order (for sequential cosine scan)

Converting between sort orders is trivial for typical skill/concept tables (hundreds to low thousands of entries): sort fixed-stride rows in-place by either the trailing OID (16 bytes at end of row) or leave unsorted for scan. Runtime can re-sort on load if needed.

The header occupies exactly one stride's worth of space. `MTSM` + `Odat` (8 bytes fixed magic, validated on load) + model FourCC + format FourCC + count + zero-padding to fill the stride. Example: `MTSMOdatNOM1E768` followed by count and padding.

**Format FourCC** — determines stride and element type (model-agnostic):

Each row: `[f32 × dims][oid_hi: u64][oid_lo: u64]` — embedding first, trailing OID. Cosine matcher reads from offset 0 for `dims × 4` bytes (no offset math), OID is at `row_start + dims × 4`.

| Format | Dimensions | Stride (bytes) | Header pad |
|--------|-----------|----------------|------------|
| `E768` | 768 × f32 | 3088 (3072 + 16) | 3068 bytes |
| `E1K5` | 1536 × f32 | 6160 (6144 + 16) | 6140 bytes |

**Model FourCC** — identifies the specific model that produced the embeddings. Embeddings from different models are not interchangeable even at the same dimension. The model FourCC is checked at runtime to ensure queries use compatible embeddings.

| Model | FourCC | Format | Notes |
|-------|--------|--------|-------|
| Nomic Embed v1.5 | `NOM1` | `E768` | Current default |

Row lookup: `data_offset = stride + (val_idx × stride)`, or equivalently `(val_idx + 1) × stride`. Header is row 0, data rows start at row 1. Every row is stride-aligned — no special-case offset arithmetic. Unknown format or model FourCCs are a validation error.

The `ImportKind` + 3×u64 value uses the same shape as `InstanceRef` and follows the same VDBE rule (MSB=0 on each u64).

**`OidIndex`** — a thin wrapper over `&[u8]` that provides binary search and range scan directly on the archive member bytes:
```rust
pub struct OidIndex<'a> {
    data: &'a [u8],
    count: u32,
}

impl<'a> OidIndex<'a> {
    pub fn from_bytes(data: &'a [u8]) -> Result<Self, OidError>;
    pub fn lookup(&self, oid: Oid) -> Option<OidEntry>;
    pub fn prefix_range(&self, prefix: Oid, depth: u8) -> impl Iterator<Item = OidEntry> + 'a;
    pub fn security_mode(oid: Oid) -> SecurityMode;  // pure bit comparison, no index needed
}
```

### Phase 2: Archive integration (`matterstream-packaging`)

**Modified:**
- `crates/matterstream-packaging/src/archive.rs` — add `oid_index()`, `oid_data_members()` to `MtsmArchive`

New FourCC types:
| FourCC | Extension | Purpose |
|--------|-----------|---------|
| `Osym` | `osym` | OID→FQA sorted symbol table (48-byte stride) |
| `Odat` | `odat` | Embedding store (model-specific stride, MTSMOdat header) |

FourCC extensions are all lowercase (matching existing `meta`, `caps`, `mrbc`, etc.). `FNTa` is the one exception. Magic bytes in `.odat` header use mixed case (`MTSMOdat`) for readability but this is internal to the format, not a file extension.

The `.osym` member is **optional** (unlike `.meta` and `.asym`).

**Sorting invariant:** The `.osym` FourCC requires entries to be sorted by OID (u128 ascending). This is enforced in `MtsmArchive::validate()` — an unsorted `.osym` member is a validation error. This guarantees that binary search and prefix range scans work correctly without any runtime fixup.

### Phase 3: VM opcodes (`matterstream-vm`)

**Modified:**
- `crates/matterstream-vm/src/rpn.rs` — add opcodes, OID import state, and `native_hooks` to `RpnVm`

**New RpnVm fields:**
```rust
pub struct RpnVm {
    // ... existing fields ...

    /// Loaded OID indices — one per loaded package. Binary searched directly.
    pub oid_indices: Vec<Vec<u8>>,  // raw .osym bytes, kept sorted
    /// Address resolver for FQA→ASLR→OVA after OID resolves to FQA.
    pub resolver: AddressResolver,
    /// Native hook dispatch table (system/internal packages only).
    pub native_hooks: Vec<NativeHookFn>,
}
```

**New opcodes:**
| Opcode | Stack effect | Description |
|--------|-------------|-------------|
| `OidPush` | `[] → [hi, lo]` | Push 16-byte immediate OID |
| `OidImport` | `[oid_hi, oid_lo] → [fqa]` | Resolve OID→FQA via binary search across loaded `.osym` indices. Security checked. This IS the import operation. Takes only the OID; returns the FQA. |
| `OidCall` | `[oid_hi, oid_lo] → ...` | OidImport + Call in one step: resolve OID→FQA→OVA, dispatch to bytecode or VM-escape |

`OidImport` is the core primitive — it takes an OID off the stack and pushes the resolved FQA. The OID is the only input; the FQA is the only output. This lets bytecode do further work with the FQA (pass it around, store it, resolve it later) rather than forcing an immediate call. `OidCall` is sugar for the common case.

**Native hook VM-escape with security enforcement:**
```rust
pub type NativeHookFn = fn(vm: &mut RpnVm, arenas: &mut TripleArena) -> Result<(), RpnError>;
```

**OidImport/OidCall dispatch:**
1. Binary search across all loaded `oid_indices` for the OID → `OidEntry`
2. Security CR must be set before VM-escape or CR modification is allowed.
   This is the **faulting operation** — if the caller's security mode doesn't
   match, the VM faults immediately (not deferred):
   - `NativeHook` requires `System` or `Internal` security CR
   - CR modification requires `System` security CR
   - `Sandboxed` callers can only resolve to `Fqa`
3. For `OidImport`: push the resolved `Fqa` onto the stack
4. For `OidCall`: resolve FQA→OVA via `self.resolver`, then dispatch:
   - `Fqa(fqa)` → feed into AddressResolver (FQA→ASLR→OVA), load/execute bytecode at that OVA
   - `NativeHook { dispatch_id }` → call `native_hooks[dispatch_id]` (VM-escape)

**New RpnError variants:**
```rust
OidNotFound(Oid),
OidSecurityViolation { oid: Oid, required: SecurityMode, actual: SecurityMode },
```

This integrates with the existing `HookContext` in `crates/matterstream-vm/src/hooks.rs` which already has React-style `useState` hooks allocating into typed memory banks (ScalarBank, IntBank, Vec3Bank, Vec4Bank, ZeroPage).

### Phase 3b: Opcode namespace refactor (`matterstream-vm`)

The flat opcode list is frozen. New domain ops go in CR-dependent pages.

**Universal base (always available, all CR modes):**
```
0x00-0x0F  Base (Nop, Push32, Push64, PushFqa, Dup, Drop, Swap,
           Add, Sub, Mul, Div, Load, Store, Call, Ret, Sync)
0x10-0x12  Map (MapNew, MapSet, MapGet)
0x13-0x19  Control (Jmp, JmpIf, Halt, Mod, CmpEq, CmpLt, CmpGt)
0x1A-0x1F  Bitwise (And, Or, Xor, Shl, Shr, Not)
0x20-0x28  Bank access (LoadBank, StoreBank, CmpGe/Le/Ne,
           LoadZpI32, StoreZpI32, LoadBankComp, StoreBankComp)
0x30-0x3A  Float (FAdd-FDiv, FCmpGt/Lt/Eq, FNeg, FAbs, I2F, F2I)
0x3B       Explode (NEW — raw bitmap destructure for hook returns / TKV)
0x40-0x4D  UI draw (legacy, kept for backwards compat — these are MTUI ops
           that ended up in the universal range unintentionally)
0x50-0x53  Event/runtime (EvPoll, EvHasEvent, FrameCount, Rand)
0x54       SetCR (moved from 0x60)
0x55       ExplodeMapped (mapped destructure through ordinal/slot system,
           respects subpackage boundary, bind-by-bitmap semantics)
0x60-0x6F  OID/import ops:
           OidPush        — push 16-byte immediate OID
           OidImport      — [oid] → [fqa] (resolve OID to FQA)
           OidCall        — [oid] → resolve + execute (current CR mode)
           OidExec        — [oid, fourcc] → resolve + execute as target FourCC
           OidCosineMatch — [query_vec, odat_fqa] → [matched_oid]
           ExecContainer  — [fqa, fourcc] → push context, execute container
                            contents as FourCC. Scoped CR switch: the contained
                            code runs under the given FourCC opcode page, context
                            pops on return. Used by DefineCard (MTUI), ExecuteQuery (VQL).
           DefineComponent — [fqa, fourcc] → register container contents as a
                            new component with a temporary subpackage-scoped OID.
                            Returns the assigned OID. Scope is the current
                            subpackage boundary; OID is not visible outside it.
```

**CR-dependent range: 0x70-0xFF** — meaning changes based on active CR FourCC.

Existing ops that are currently in 0x70+ get reframed as their CR page:

| CR FourCC | 0x70+ ops | Notes |
|-----------|-----------|-------|
| `MTUI` (default) | (no 0x70+ ops currently) | UI ops at 0x40-0x4D are universal, not touched — already encoded in shaders |
| `VQL0` | VqlBeginQuery, VqlEndQuery, VqlBind, VqlSetField, VqlSetFieldStr, VqlFilter, VqlProject, VqlParam | Currently at 0x61-0x68, migrate to 0x70-0x77 |
| `SKLL` | SkillBegin, SkillEnd, SkillStep, SkillLlmStep, ... | Currently at 0x70-0x87, stays in range |
| `SKLL` (new ops) | SetTargetModel, AppendToPrompt, ExecuteAndReturnValue, Forward, ExecuteAction (by OID), ExecuteQuery (embedded VQL, CR switch), DefineCard (embedded MTUI, CR switch, deferred mode) | |
| `OBJT` (future) | ObjTypeBegin, ObjTypeEnd, ObjTypeField, ... | Currently at 0x7D-0x81, gets own CR page |
| `CARD` (future) | CardBegin, CardEnd, CardSetShortDesc, ... | Currently at 0x82-0x85, gets own CR page |

**Migration**: The current VQL/Skill/ObjType/Card ops at 0x61-0x87 continue to work as-is in existing bytecode. New bytecode emitted by the compiler uses SetCR + 0x70-range ops. Old bytecode is valid because the VM can recognize both layouts during a transition period.

**Skills ↔ Odat matching**: Skills are authored as TSX components/arrow functions, compiled to bytecode under the `SKLL` CR page. Each skill has a concept embedding in `.odat`. At runtime: user input → embed → cosine match against `.odat` → get OID from trailing bytes → resolve skill → execute.

### Phase 4: Compiler integration (`matterstream-core`, behind `compiler` feature)

**Modified:**
- `crates/matterstream-core/src/asm_compiler.rs` — import collection pass, OID emission

New compilation pass:
1. Walk `ImportDeclaration` AST nodes from oxc
2. Map import path to OID tree location:
   - `@chitin/...` → resolve under `1.1.1.1`
   - `@chitin/internal/...` → resolve under `1.1.1.2`
   - `@chitin/system/...` → resolve under `1.1.1.3`
   - Other packages → resolve under `1.1.2` using reverse-DNS hash encoding
3. Determine `ImportKind` from module path / naming convention
4. When encountering imported references in JSX/code, emit `OidPush <oid>; OidCall`
5. After compilation, serialize collected imports as `.osym` data

### Phase 5: End-to-end demo — `package_example` with actual imports

Evolve the existing `package_demo.rs` into a multi-package example with real OID imports:

1. **Library package** — archive with:
   - `.meta` manifest
   - `.asym` table
   - `.mrbc` bytecode for a component (e.g., a reusable Button that draws a slab)
   - `.osym` exporting the component under a CHT public OID (`1.1.1.1.*`)

2. **Consumer package** — archive with:
   - `.meta` manifest
   - `.asym` table
   - `.mrbc` bytecode that uses `OidPush` + `OidCall` to import and invoke the Button
   - `.osym` declaring the import dependency

3. **Runtime**:
   - Load both archives
   - Load the `.osym` bytes from both archives (binary searched directly, no trie)
   - Load the consumer's bytecode into the VM
   - `OidCall` resolves the Button's OID → FQA → ASLR → OVA → execute
   - Verify the Button's draw commands appear in output

4. **Security test**: attempt `OidCall` to a NativeHook from a sandboxed OID → verify `OidSecurityViolation` error

5. **Embedding test** (if time): add a `.odat` with a concept embedding, use `OidCosineMatch` against a query vector, verify correct OID returned

---

## Crate change summary

| Crate | Changes |
|-------|---------|
| `matterstream-core` | **Phase 0:** Delete stale `rpn.rs`/`ui_vm.rs`, re-export from vm. **Phase 4:** Import collection pass in asm_compiler. |
| `matterstream-vm-addressing` | New `oid.rs`, `oid_index.rs`. Add `Osym`/`Odat` to FourCC. Well-known OID constants. `SecurityMode`. |
| `matterstream-packaging` | Add `oid_index()`/`oid_data_members()` to MtsmArchive. |
| `matterstream-vm` | Add OidPush/OidCall/OidResolve opcodes. Add `oid_index`, `native_hooks` to RpnVm. Security enforcement in dispatch. |
| `matterstream-vm-asm` | Add OidPush/OidCall token variants. |
| `matterstream` | Re-export new public types. |

**Unchanged:** `matterstream-vm-arena`, `matterstream-vm-scl`

## Verification

1. **Phase 0:** `cargo test` passes after RPN consolidation, all existing examples still work
2. Unit tests for `Oid`: construction, segment extraction, u128 round-trip, VDBE invariant (MSBs always 0), well-known constants, `security_mode()` returns correct mode for each subtree
3. Unit tests for `OidIndex`: insert/lookup, serialization round-trip
4. Integration test: archive with `.osym` member → serialize → parse → binary search lookup
5. VM test: hand-assemble `OidPush + OidCall` bytecode with loaded `.osym` bytes → verify dispatch to native hook
6. **Security faulting tests**: verify sandboxed OID faults on VM-escape attempt; verify internal OID faults on CR modification; verify system OID passes both
7. **Embed/match resilience tests** (Rust unit tests):
   - Cosine matching with zero vectors, NaN, inf, denormals
   - Empty `.odat` (zero rows)
   - Single-row `.odat`
   - Matching returns correct trailing OID
   - Invalid/corrupt `.odat` header (wrong magic, unknown model/format FourCC)
8. **Symbol-like object tests**: test import resolution for Symbol (JS/TS native), Concept (`@chitin/`), PackageBoundary, Capability — verify each resolves correctly through `.osym` and respects security boundaries
9. **Fuzz targets** (extend existing fuzz infrastructure):
   - `fuzz_osym`: random `.osym` bytes → parse, binary search, validate sorting invariant
   - `fuzz_odat`: random `.odat` bytes → parse header, validate magic/format/model, row access
   - `fuzz_oid`: random u128 → Oid construction, segment extraction, security_mode(), round-trip
   - `fuzz_oid_vm`: random bytecode with OID ops → execute under gas budget, verify no panics
10. Compiler test: TSX with `import` statement → verify OidPush/OidCall in output bytecode, correct OID tree placement
11. Run `cargo test` across workspace, run existing examples to verify no regressions
