# MatterStream

A UI Instruction Set Architecture (ISA) implemented as an async Rust library. MatterStream treats UI as a stream of immutable instructions (Ops) executed against a 4-tier, register-mapped memory space (Matter).

## Architecture

| Tier | Name | Analog | Purpose |
|------|------|--------|---------|
| 0 | Global | BIOS | Shared uniforms (time, theme atoms) |
| 1 | Registers | CPU Regs | Typed banks (MAT4, VEC4, VEC3, SCL, INT) |
| 2 | Zero Page | Direct RAM | 256-byte direct-addressing storage buffer |
| 3 | Resource | Extended | 8-bit type-tagged handles (BBOs, Textures, Fonts) |

## Usage

```rust
use matterstream::{MatterStream, Op, OpsHeader, RsiPointer, Primitive};

smol::block_on(async {
    let mut stream = MatterStream::new();
    let header = OpsHeader::new(
        vec![RsiPointer::new(1, 2, 0)], // Tier 1, Vec3 bank, register 0
        false,
    );
    stream
        .execute(
            &header,
            &[Op::Draw {
                primitive: Primitive::Slab,
                position_rsi: 0,
            }],
        )
        .await;
});
```

## Testing

```sh
cargo test
```

## License

Apache-2.0. See [LICENSE](LICENSE) for details.
