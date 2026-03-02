# Missing Features

Features not yet implemented in the MatterStream VM and toolchain.

## TSX Compilation

- **Dynamic expressions** — `{count + 1}` in JSX attributes/children
- **State management** — `useState` / `useReducer` hooks mapping to VM bank allocation
- **Event handlers** — `onClick`, `onKeyDown` in TSX markup → bytecode event handlers
- **Conditional rendering** — `{cond && <Elem />}` → JmpIf in bytecode
- **List rendering** — `{items.map(...)}` → loops over ZeroPage arrays
- **Component composition** — nested components with props

## Rendering

- **Actual text rendering** — current implementation draws colored rectangle placeholders for text. Needs glyphon integration for CPU path or glyph atlas for GPU path.
- **Font loading** — no font file loading or glyph rasterization
- **Text layout** — no line breaking, text measurement, or alignment
- **Animations** — no interpolation/easing primitives
- **Gradients** — no linear/radial gradient fill
- **Images/textures** — no image loading or texture sampling
- **Shadows** — no drop shadow or box shadow

## GPU Pipeline

- **Compute shader VM** — WGSL compute shader that interprets render bytecode on the GPU (shaders defined in spec but not yet compiled/loaded)
- **wgpu pipeline setup** — device creation, bind groups, render passes (struct defined, not wired)
- **Indirect draw** — draw_indexed_indirect from compute-generated draw count
- **Text compositing** — glyphon text overlay pass

## Audio

- **Audio playback opcodes** — no sound effect or music support
- **Audio mixing** — no multi-channel audio

## Networking

- **Network I/O opcodes** — no HTTP, WebSocket, or peer-to-peer communication
- **Multiplayer** — no shared state or synchronization

## Runtime

- **Archive-based module loading** — import bytecode from MTSM packages
- **Hot reload** — live bytecode replacement without restart
- **Sandboxing** — filesystem/network isolation (gas metering provides CPU sandboxing)

## Developer Tools

- **Debugger** — step-through mode, breakpoints, register inspection
- **Source maps** — TSX line → bytecode offset mapping
- **Profiler** — per-opcode timing, hot path analysis (ExecTrace provides basic counters)
- **REPL** — interactive bytecode evaluation

## Platform

- **WebGL/WebGPU backend** — browser-based rendering
- **Mobile support** — touch input events, screen scaling
- **Accessibility** — screen reader integration, keyboard navigation
