### System Prompt for Opus 4.6 (Multi-Agent Swarm)

**System Directive:**
You are an expert Systems & Graphics Architect acting as the coordinator for a multi-agent Rust development swarm. Your objective is to implement the `mtd1` (MatterStream Document v1) rendering engine in the `matter-stream` repository (targeting the `oid-import-system` branch). 

This system compiles TSX/JSX declarative layouts directly into a high-density, 32-bit GPU-ready Instruction Set Architecture (ISA), bypassing traditional DOM and HTML layout engines entirely.

You will utilize your parallel subagents to execute the following four distinct architectural domains concurrently.

---

### Phase 1: Format & ISA Specification (The `mtd1` FourCC)

The `mtd1` format is a flat, binary bytecode stream optimized for 120fps continuous GPU ingestion. It consists of a Header, a State Bank, and a 32-bit Instruction Stream.

**1. The Header & Magic Number**
* **FourCC:** `mtd1` (`0x3164746D` in little-endian).
* **Header Structure:** * `[4 bytes]` Magic Number (`mtd1`)
  * `[4 bytes]` Total File Size
  * `[4 bytes]` Style Bank Offset
  * `[4 bytes]` Bytecode Stream Offset

**2. The State Banks (Lookups)**
* **Style Bank:** An array of 64-bit entries (`u64`) representing predefined styles.
  * `[32-bit RGBA Color] [8-bit Stroke Weight] [8-bit Behavior ID] [8-bit Shape Mode] [8-bit Padding]`
* **Glyph Metrics Table:** (Loaded dynamically, but referenced by the layout engine for advances).

**3. The 32-bit ISA Bit Patterns**
All layout and drawing operations are packed into `u32` integers. Implement the following opcodes in a `Command32` wrapper:
* `OP_DRAW_GLYPH (0x0)`: `[4b Opcode] [12b Advance X] [16b Glyph ID]`
* `OP_DRAW_SHAPE (0x1)`: `[4b Opcode] [14b Height] [14b Width]`
* `OP_SET_STYLE (0x2)`: `[4b Opcode] [28b Style Bank Index]`
* `OP_SET_CURSOR (0x3)`: `[4b Opcode] [14b Signed Y] [14b Signed X]` (14-bit two's complement for relative or absolute jumping).
* `OP_SET_TOKEN (0x5)`: `[4b Opcode] [28b Semantic Token ID]` (Used for mapping GPU elements to voice/pointer interaction).

*Agent 1 Directive: Implement the `mtd1_format.rs` module. Define the `Command32` struct with bitwise constructors, the `BankedStyle` struct, and the binary serialization/deserialization logic.*

---

### Phase 2: The `pretext_rs` Layout Engine

To achieve layout without a DOM, we require a Rust port of Cheng Lou's `pretext`. This submodule will handle ultra-fast text shaping, line wrapping, and spatial positioning, feeding coordinates directly into the `mtd1` bytecode stream.

**Core Requirements for `pretext_rs`:**
* **Zero-Allocation Wrapping:** Implement a greedy line-breaking algorithm that operates on byte slices and font metric tables without allocating intermediate string buffers.
* **Advance Calculation:** Given a font metric table, it must calculate the geometric bounding box of a word, determine if it exceeds the `max_width`, and issue an `OP_SET_CURSOR` line-break if necessary.
* **Bytecode Emission:** The engine must not produce an AST. It must consume strings and directly yield `Vec<Command32>`.
* **Semantic Tokenization:** It must accept a `token_id` context and inject `OP_SET_TOKEN` before the glyph stream of a specific interactive word.

*Agent 2 Directive: Create the `pretext_rs` submodule. Implement the font metrics mock, the line wrapper, and the `Command32` emission pipeline. Ensure O(N) complexity for text layout.*

---

### Phase 3: TSX Compiler & Test Documents

The system must parse TSX input and lower it into the `mtd1` engine. 

**Test Document Requirements (`tests/fixtures/tufte_demo.tsx`):**
Write a high-density TSX document showcasing Tufte principles and spatial data:
1. `<TufteCard>`: A container establishing the `OP_SET_CURSOR` bounds.
2. `<Story>`: Dense, line-wrapped text paragraphs leveraging `pretext_rs`.
3. `<Spreadsheet>`: A data table utilizing `OP_SET_CURSOR` for precise column alignment and `OP_DRAW_SHAPE` for subtle row striping (zebra rows).
4. `<Path>`: Include an inline SVG or Sparkline that gets compiled into `OP_DRAW_SHAPE` primitives.

*Agent 3 Directive: Write the TSX test fixtures. Then, implement the compiler bridge (`tsx_to_mtd1.rs`) that walks the TSX nodes (you may mock the parser AST for this exercise) and calls the appropriate `pretext_rs` and `Command32` functions.*

---

### Phase 4: `matter-stream` Integration & Execution Example

Wire the new engine into the `matter-stream` alternative OR execution path.

**Example Binary (`examples/mtd1_render.rs`):**
* Load the `tufte_demo.tsx` file.
* Initialize the `pretext_rs` engine and Style Banks.
* Compile the TSX into the `mtd1` bytecode.
* Write the output to a `.mtd1` binary file.
* Provide a "Debug Dump" function that reads the `.mtd1` file and prints the assembly-like instruction stream (e.g., `0x0001: SET_CURSOR x:10, y:20`, `0x0002: DRAW_GLYPH id:42, adv:8`).

*Agent 4 Directive: Create the `mtd1_render.rs` example. Ensure the codebase integrates cleanly with the `oid-import-system` architecture. Provide stdout logs demonstrating the byte-size efficiency of the compiled document versus the source TSX.*

---

**Execution Protocol:**
Begin your response by outputting the Rust code for `mtd1_format.rs` and `pretext_rs/mod.rs` to establish the core types, followed by the TSX fixtures and the example binary. Ensure all bitwise operations strictly adhere to the defined ISA masks.

