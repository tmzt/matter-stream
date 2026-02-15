To truly test the **MTSM** propulsion system, we need TSX examples that exercise the different "stages" of our engine: from simple **Laminar Flow** (translation) to complex **Fragment Nesting** (0x0C jumps).

Here are three templates that **Oxc** will parse into `UIElements` and `UIFragments`.

---

## 1. The "Sensor Atom" (Pure Translation)

This exercises the **Laminar Flow** path. Since it only uses `x` and `y` props, the compiler should emit `SET_TRANS` instead of a full matrix load.

```tsx
// Atom: Minimal footprint, utilizes Tier 1 Registers
const SensorAtom = ({ x, y, value, status }) => (
  <Slab 
    x={x} 
    y={y} 
    width={10} 
    height={10} 
    color={status === 'alert' ? 'red' : 'green'}
    opacity={value / 100}
  />
);

```

**Leah's Audit:** Should see a `SET_TRANS` for `x,y` and a single `REG_VEC4` load for the status color.

---

## 2. The "Telemetry Group" (UIFragment)

This creates a **UIFragment**. It bundles multiple atoms into a single reusable instruction block with an **Ops Handle**.

```tsx
// Fragment: A molecular bundle with its own ZPage Stride
const TelemetryGroup = ({ label, sensors }) => (
  <Group>
    <Text value={label} y={-20} />
    {sensors.map((s, i) => (
      <SensorAtom 
        key={i} 
        x={i * 15} 
        value={s.val} 
        status={s.status} 
      />
    ))}
  </Group>
);

```

**Leah's Audit:** The `map` function should be unrolled into a series of `0x0C` calls. The `Group` should trigger a `PUSH_PROJ` if it has a container transform.

---

## 3. The "Industrial Grid" (Massive Instancing)

This is the ultimate test for **Tier 2 (Zero Page)** and **Tier 3 (BBO)**. It renders thousands of elements by pointing one `OpsHandle` to a massive data buffer.

```tsx
// The "Rocket Engine" Test: 10,000+ units
const PowerGrid = ({ dataSource }) => (
  <BBOProvider source={dataSource} stride={32}>
    <InstancedArray count={10000}>
      <TelemetryGroup label="Phase-A" />
    </InstancedArray>
  </BBOProvider>
);

```

---

## How the Compiler Handles These

| TSX Feature | MTSM Instruction | Memory Tier |
| --- | --- | --- |
| `<Slab x={x} />` | `SET_TRANS` | **Tier 1** (REG_VEC3) |
| `<TelemetryGroup />` | `0x0C [Handle] [Offset]` | **Tier 2** (ZPage Base) |
| `dataSource` | `BBO_REF` | **Tier 3** (Resource) |
| `<Group>` | `PUSH_PROJ` | **Tier 1** (Stack) |

---

## Updated `AGENTS.md`: The TSX-to-Binary Mapping

> ### 44. TSX Pattern Recognition
> 
> 
> * **Laminar Patterns:** Components that only pass `x, y, z` props must be tagged by the `UIElement` generator for `SET_TRANS` optimization.
> * **Fragment Isolation:** Any functional component (`const Comp = ...`) must be baked as a unique `UIFragment` with a dedicated `OpsHandle`.
> * **Prop-to-ZPage Mapping:** Dynamic props that change per-instance (like `value` or `status`) must be mapped to specific offsets in the **Zero Page (Tier 2)**.
> * **Static Prop Folding:** Props that are identical across all instances (like `width={10}`) should be folded into the `UIFragment` instructions as constants to save bandwidth.
> 
> 

---

### The "Ignition" Test Case

When we run `cargo run --bin leah-test`, we will feed it the binary output of the `PowerGrid`.

1. **Leah** will verify that the 10,000 instances don't overflow the VRAM.
2. **The Oracle** will simulate the "Propulsion" to ensure the `SET_TRANS` calls are actually skipping the matrix math.
3. **The Result:** A perfectly stable, 144Hz industrial visualization.

> **Wit's Bit:** These TSX examples are our "Test Pilots." If the `PowerGrid` can fly through Leah's diagnostics without a "Flameout," then the **MTSM** engine is ready for the real world.

**Would you like me to generate the Rust `Visitor` logic that recognizes these specific TSX patterns and maps them to the `SET_TRANS` and `0x0C` instructions?**
