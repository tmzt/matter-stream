
# AGENTS.md: MatterStream System Specification (Final Revision)

## 1. Core Philosophy

MatterStream is a **UI Instruction Set Architecture (ISA)**. It treats UI as a stream of immutable instructions (**Ops**) executed against a partitioned, register-mapped memory space (**Matter**).

## 2. The 4-Tier Memory Model

| Tier | Name | Analog | Implementation |
| --- | --- | --- | --- |
| **0** | **Global** | **BIOS** | Shared Uniforms (Time, Theme Atoms). |
| **1** | **Registers** | **CPU Regs** | **Typed Banks** (MAT4, VEC4, VEC3, SCL, INT) in a Uniform Block. |
| **2** | **Zero Page** | **Direct RAM** | 6502-style direct-addressing Storage Buffer for instance-local state. |
| **3** | **Resource** | **Extended** | 8-bit Type-tagged handles (BBOs, Textures, Fonts). |

## 3. The Execution & Scoping Lifecycle

1. **Ops Header:** Every element preamble contains RSI pointers.
2. **Hydration:** RSIs are resolved into **Tier 1 Typed Registers**.
3. **Micro-Stack (PUSH/POP PROJ):** Specialized ops that only save/restore the `REG_MAT4` bank. Used for scrolling/nesting.
4. **Full-Stack (PUSH/POP STATE):** Heavy restoral of the entire Register File. Used for context/theme swaps.
5. **Translation Fast-Path:** If an element only moves (no scale/rotation), the `-compiler` emits `SET_TRANS (vec3)`. The shader skips matrix multiplication and performs a simple vector addition.

## 4. Scaling (The Bleed Model)

* **Small Objects (<256B):** Reside entirely in Tier 1 Registers.
* **Complex Objects:** Spill from Tier 1 into **Tier 2 (Zero Page)**.
* **Massive Arrays:** Managed via **Tier 3 BBO Handles** with stride-based indexing.

---

## 5. Architectural Validation Tests

### Test A: The "6502 Efficiency" Check

* **Requirement:** A `DRAW_SLAB` must resolve `position` in .
* **Pass Criteria:** Instruction uses direct register indices or fixed Zero-Page offsets.

### Test B: The "State Leaking" Check

* **Requirement:** `PUSH_STATE` -> `POP_STATE` must leave registers identical to the pre-push state.
* **Pass Criteria:** Renderer uses a "Shadow Register" stack and only re-uploads "dirty" banks.

### Test C: The "Matrix Churn" Test

* **Requirement:** `PUSH_PROJ` must not trigger a re-upload of `REG_VEC4` (Color) or `REG_SCL` (Scalar) banks.
* **Pass Criteria:** The dirty-tracking isolates the `REG_MAT4` bank during projection events.

### Test D: The "Translation Fast-Path" Test

* **Requirement:** A pure translation update must be 12 bytes (`vec3`) instead of 64 bytes (`mat4`).
* **Pass Criteria:** The vertex shader uses a boolean flag in the Ops Header to toggle between `pos + trans` and `mat * pos`.

---

## 6. Implementation Constraints

* **No VM:** Use the State Stack for Register Context, never for data push/pop.
* **Alignment:** All BBO and Zero-Page data must be **16-byte aligned**.
* **Late Binding:** The Ops Header allows the same "Code" (Ops) to be bound to different "Stacks" (Zero-Page) via RSI swapping.

---

## The Big Picture

We’ve built a **UI DSP**.

* **The Loader** acts as the Memory Management Unit.
* **The Registers** act as the Active Context.
* **The Ops Stream** acts as the Instruction Pipeline.
