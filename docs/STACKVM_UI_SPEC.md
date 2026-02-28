# STACKVM_UI_SPEC v0.1.0

Extension to the MTSM-RPN-Bincode VM adding 2D UI draw opcodes for CPU-side softbuffer rendering.

## UI Opcode Range

Opcodes `0x40–0x48` (9 opcodes). All are zero-payload — arguments come from the stack.

## Opcodes

| Byte | Mnemonic       | Stack (push order → pop) | Effect                        |
|------|----------------|--------------------------|-------------------------------|
| 0x40 | UiSetColor     | (rgba:u32 →)             | Set current draw color        |
| 0x41 | UiBox          | (x y w h →)              | Emit filled rectangle         |
| 0x42 | UiSlab         | (x y w h radius →)       | Emit rounded rectangle        |
| 0x43 | UiCircle       | (x y r →)                | Emit filled circle            |
| 0x44 | UiText         | (x y size slot →)        | Emit text placeholder         |
| 0x45 | UiPushState    | (→)                      | Save draw state               |
| 0x46 | UiPopState     | (→)                      | Restore draw state            |
| 0x47 | UiSetOffset    | (dx dy →)                | Set translation offset        |
| 0x48 | UiLine         | (x1 y1 x2 y2 →)         | Emit line segment             |

## Draw State

- **Current color**: `u32` packed RGBA (`0xRRGGBBAA`). Default: `0xFFFFFFFF` (white, fully opaque).
- **Offset**: `(dx: i32, dy: i32)`. Default: `(0, 0)`. Applied to all draw coordinates.
- **State stack**: `UiPushState` / `UiPopState` save/restore color + offset. Max depth: 16.

## Draw List

Each UI draw opcode appends a `UiDrawCmd` to `Vec<UiDrawCmd>` on the `RpnVm`. Commands are rendered in order (painter's algorithm). Maximum 4096 draw commands per execution.

## Color Format

- VM-side: `u32` packed RGBA — `0xRRGGBBAA`.
- Softbuffer-side: `u32` packed `0x00RRGGBB` (alpha ignored in final pixel, used for blending).
- `rgba(r, g, b, a) -> u32` packs components into RGBA.
- `rgba_unpack(u32) -> (u8, u8, u8, u8)` extracts components.
- Alpha blending via `blend_pixel(dst: u32, src_rgba: u32) -> u32`.

## Rendering

CPU-side rasterization into a softbuffer pixel buffer (`&mut [u32]`, width, height).

Primitives:
- **Box**: Axis-aligned filled rectangle, bounds-clipped.
- **Slab**: Rounded rectangle — box with corner radius, SDF-based corner test.
- **Circle**: Filled circle, midpoint distance check.
- **Line**: Bresenham's line algorithm with per-pixel alpha blend.
- **Text**: Placeholder — emits a colored rectangle at the text position.

## Gas Metering

All UI opcodes cost `cost_ui` gas (default: 5). Configurable via `GasConfig`.

## Error Conditions

| Error                    | Trigger                                  |
|--------------------------|------------------------------------------|
| `UiStateStackOverflow`   | `UiPushState` when stack depth >= 16     |
| `UiStateStackUnderflow`  | `UiPopState` when stack is empty         |
| `UiDrawLimitExceeded`    | Draw command count exceeds 4096          |
