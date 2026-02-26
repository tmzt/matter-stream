# MatterStream Examples

Runnable examples demonstrating the MatterStream UI ISA pipeline.

## Quick Start

```sh
# From the repository root:
cargo run -p matterstream --example <name>
```

## Headless Examples

These print results to stdout — no window or GPU required.

### `parse-tsx` — Parse TSX into an AST

Parses a TSX source string containing `<Slab>` elements and prints the
resulting AST: element IDs, kinds, and attribute key/value pairs.

```sh
cargo run -p matterstream --example parse-tsx
```

**Demonstrates:** `Parser::parse()`, `Parsed`, `TsxFragment`, `TsxElement`,
`TsxAttributes`, `TsxKind`, `TsTypeValue`.

---

### `compile-tsx` — Compile TSX to MatterStream Ops

Compiles a TSX fragment into a flat stream of MatterStream ISA instructions
(`Op`). Shows the header (RSI pointers, flags) and every op in sequence.

```sh
cargo run -p matterstream --example compile-tsx
```

**Demonstrates:** `Compiler::compile()`, `CompiledOps`, `OpsHeader`, `Op`
(SetColor, SetTrans, Draw).

---

### `execute-ops` — Build & Execute Ops by Hand

Constructs an op stream programmatically using `StreamBuilder` (no TSX
parsing), then executes it on a `MatterStream` instance and prints the
resulting draw calls with positions, colors, and fast-path info.

```sh
cargo run -p matterstream --example execute-ops
```

**Demonstrates:** `StreamBuilder`, `MatterStream::execute()`, `OpsHeader`,
`RsiPointer`, `BankId`, `Draw`, `PushState`/`PopState`.

---

### `full-pipeline` — End-to-End Pipeline (Headless)

Runs the complete MatterStream pipeline on a TSX login-form layout and
prints draw results with hex colors:

1. **Parse** — `Parser::parse()` produces a `Parsed` AST
2. **Compile** — `Compiler::compile()` emits `CompiledOps`
3. **Process** — `Processor::process()` resolves packages via `PackageRegistry`
4. **Execute** — `MatterStream::execute()` produces draw results

```sh
cargo run -p matterstream --example full-pipeline
```

**Demonstrates:** Full pipeline integration, `PackageRegistry`,
`CoreUiPackage`, `Processor`, `ProcessorOutput`.

---

## Windowed Examples (Winit + Softbuffer)

These open a native window and render MatterStream draw results as colored
rectangles using Softbuffer (CPU-side pixel buffer, no GPU required).

### `window-slabs` — Compile TSX & Render to Window

Compiles inline TSX with 8 colored slabs and renders them as colored
rectangles in a 640x480 window. The simplest path from TSX to pixels.

```sh
cargo run -p matterstream --example window-slabs
```

**Demonstrates:** `Compiler::compile()` + `MatterStream::execute()` +
Winit event loop + Softbuffer pixel rendering. NDC-to-screen coordinate
mapping.

---

### `window-builder` — StreamBuilder Grid in a Window

Builds a 3x3 grid of colored slabs using `StreamBuilder` (no TSX) and
renders them in a window. Shows the ISA working directly without any
parsing or compilation.

```sh
cargo run -p matterstream --example window-builder
```

**Demonstrates:** `StreamBuilder`, `Op::SetColor`, `PushState`/`PopState`,
programmatic scene construction, Winit rendering.

---

### `window-pipeline` — Full Pipeline Rendered to Window

Runs the full parse/compile/process/execute pipeline on a mock login-form
TSX layout with `PackageRegistry`, then renders the result as horizontal
bars in a 400x500 window.

```sh
cargo run -p matterstream --example window-pipeline
```

**Demonstrates:** End-to-end pipeline (`Parser` -> `Compiler` -> `Processor`
-> `MatterStream` -> window), `PackageRegistry`, `CoreUiPackage`.

---

### `run-tsx` — Load & Render a .tsx File

Reads a `.tsx` file from the command line, compiles it, and renders colored
squares in a native window. Supports a `--timeout` flag for CI/testing.

```sh
cargo run -p matterstream --example run-tsx -- crates/matterstream/examples/example.tsx
cargo run -p matterstream --example run-tsx -- --timeout 5 crates/matterstream/examples/login_form.tsx
```

## Example TSX Files

Sample `.tsx` inputs live alongside the examples:

| File | Description |
|------|-------------|
| `crates/matterstream/examples/example.tsx` | 3 colored slabs at different positions |
| `crates/matterstream/examples/login_form.tsx` | Simple 4-component login form layout |

## Architecture Reference

```
TSX Source
    |
    v
 Parser::parse()        -->  Parsed { root_fragment, mtsm_data }
    |
    v
 Compiler::compile()    -->  CompiledOps { header, ops: Vec<Op> }
    |
    v
 Processor::process()   -->  ProcessorOutput { root_fragment, mtsm_data, ops }
    |
    v
 MatterStream::execute()  -->  stream.draws: Vec<Draw>
    |
    v
 Softbuffer / Renderer   -->  pixels on screen
```

The ISA operates on a 4-tier register-mapped memory space:

| Tier | Name | Purpose |
|------|------|---------|
| 0 | Global | Shared uniforms (time, theme) |
| 1 | Registers | Typed banks (MAT4, VEC4, VEC3, SCALAR, INT) |
| 2 | Zero Page | 256-byte direct-addressing storage |
| 3 | Resource | Type-tagged handles (textures, fonts) |
