# TKV Arena Object Format

TKV (Type-Key-Value) arena objects are fixed-size-entry sorted tables stored in the TripleArena. They provide typed, hierarchically-keyed property bags for VM-allocated structured data — used by external components (e.g., `<Param>`, `<Trigger>`) to pass typed props through OID dispatch.

## Layout

```
┌────────────────────────────────────┐
│ Header (4 bytes)                   │
│   [u32 LE] entry count             │
├────────────────────────────────────┤
│ Entry 0 (16 bytes)                 │
│ Entry 1 (16 bytes)                 │
│ ...                                │
│ Entry N-1 (16 bytes)               │
└────────────────────────────────────┘
```

Total size: `4 + count * 16` bytes. Arena slot is pre-allocated at a generous fixed size (e.g., 1024 bytes = 63 entries max).

## Entry Format (16 bytes)

```
Offset  Size  Field
──────  ────  ─────
0       4     key_path      TkvKey (u32) — 3-bit segment packed path
4       1     value_type    Type tag for the value
5       8     value         Payload (format depends on value_type)
13      1     key_str_disc  Key name string discriminant
14      2     key_str_idx   Key name string index (u16 LE)
```

Entries are **sorted by `key_path.sort_key()`** (high 24 bits only — metadata bits masked out). This enables binary search by path and contiguous prefix scans.

### TkvKey (key_path)

See `matterstream-vm-addressing/src/tkv_key.rs`. 32-bit packed:

```
[31..8]  8 segment slots × 3 bits (segment 0 = bits 31..29, most significant)
[7]      is_array flag — slots 6+7 merge into 6-bit array index
[6..4]   prefix_len (3 bits, 1-7) — CIDR-style depth
[3..1]   type tag (3 bits) — redundant with value_type, carried for key-only ops
[0]      pad
```

Sorting uses `key_path & 0xFFFF_FF00` — only segment bits. Metadata (array flag, prefix_len, type) doesn't affect order.

### Value Types

| Tag | Name    | Payload (8 bytes)                                  |
|-----|---------|----------------------------------------------------|
| 0   | String  | `[u8 disc] [u32 LE index] [u8×3 pad]`             |
| 1   | Integer | `[u64 LE value]`                                   |
| 2   | Boolean | `[u8 value] [u8×7 pad]`                            |
| 3   | Fqa     | `[u64 LE fqa_lo]` (high bits in extended entry TBD)|
| 4   | Table   | `[u32 LE ova] [u8×4 pad]` — nested TKV object     |
| 7   | Null    | `[u8×8 pad]`                                       |

### String References

String values are never stored inline. The 8-byte payload holds a discriminated reference:

```
[u8 discriminant] [u32 LE index] [u8×3 pad]
```

| Disc | Source             | Mutability  |
|------|--------------------|-------------|
| 0x00 | string_table       | Immutable (compile-time interned) |
| 0x01 | string_values table| Mutable (runtime arena-allocated) |

### Key Name String

Every entry carries a human-readable key name in the trailing 3 bytes (`key_str_disc` + `key_str_idx`). This uses the same discriminant scheme as string values:

```
[u8 disc] [u16 LE index]
```

Used for:
- Serialization (JSON export, debugging)
- `TkvAdd` — resolving new key names when inserting under a parent
- Not used for sorting or lookup (that's `key_path`)

## Operations

### By Ordinal (O(1))

`TkvSet` and `TkvGet` take an **ordinal** (entry index, 0-based). The entry is at byte offset `4 + ordinal * 16`. No search needed — the compiler knows the ordinal from the template.

### By Path (O(log n))

`TkvSetPath` and `TkvGetPath` take a `TkvKey` and binary search the sorted entries by `sort_key()`. Used when the ordinal isn't known at compile time.

### Insert (O(n))

`TkvAdd` takes a parent path + new key name string + value. It:
1. Determines the next available segment under the parent
2. Constructs the full `TkvKey` (parent path + new segment)
3. Inserts in sorted position (shifts entries down)
4. Updates the header count

This is the expensive path — re-sorts the table. Non-deterministic gas cost proportional to entry count.

## Templates (tkv_table)

The compiler pre-builds TKV documents at compile time for element prop sets where all values are known (string literals, numbers, booleans). These are stored in `tkv_table: Vec<Vec<u8>>` alongside the string table in `AsmOutput`.

At runtime:
1. `TkvClone(template_id)` copies the template into the dynamic arena → returns OVA
2. `TkvSet(ova, ordinal, value)` patches individual values (for dynamic props)

This avoids the expensive `TkvNew` + repeated `TkvAdd` path for the common case of all-constant props.

## Arena Residency

| Arena    | Use case                                           |
|----------|----------------------------------------------------|
| Dynamic  | Active prop bags, mutable. Created by TkvClone/TkvNew. |
| Nursery  | Sealed (immutable). Created by TkvSeal. Long-lived. |
| Template | Not in arena — in `tkv_table` (compile-time constant). |

## Example

A `<Param name="temperature" type="number" required={true} description="Target temp">` produces:

```
Entry 0: key_path=[1]       type=String  value=str_ref("temperature")   key_name="name"
Entry 1: key_path=[2]       type=String  value=str_ref("number")        key_name="type"
Entry 2: key_path=[3]       type=Boolean value=true                     key_name="required"
Entry 3: key_path=[4]       type=String  value=str_ref("Target temp")   key_name="description"
```

Header: count=4. Total size: 4 + 4×16 = 68 bytes.

Sorted by key_path sort_key: [1] < [2] < [3] < [4] — already in order since single-segment keys with increasing values.
