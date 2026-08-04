# BoltFFI

A high-performance multi-language bindings generator for Rust. Up to 1,000x faster than UniFFI. Up to 450x faster than wasm-bindgen.

<p align="center">
  <img src="docs/assets/demo.gif" width="700" />
</p>

<p align="center">
  <a href="https://discord.gg/Q6A7zNNFk3">
    <img src="https://img.shields.io/badge/Discord-Join%20the%20community-5865F2?style=for-the-badge&logo=discord&logoColor=white" alt="Join our Discord" />
  </a>
</p>

Quick links: [User Guide](https://boltffi.dev/docs/overview) | [Tutorial](https://boltffi.dev/docs/tutorial) | [Getting Started](https://boltffi.dev/docs/getting-started)

## Performance

### vs UniFFI (Swift/Kotlin)

| Benchmark | BoltFFI | UniFFI | Speedup |
|-----------|--------:|-------:|--------:|
| noop | <1 ns | 1,416 ns | >1000x |
| echo_i32 | <1 ns | 1,416 ns | >1000x |
| counter_increment (1k calls) | 1,083 ns | 1,388,895 ns | 1,282x |
| generate_locations (1k structs) | 4,167 ns | 1,276,333 ns | 306x |
| generate_locations (10k structs) | 62,542 ns | 12,817,000 ns | 205x |

### vs wasm-bindgen (WASM)

| Benchmark | BoltFFI | wasm-bindgen | Speedup |
|-----------|--------:|-------------:|--------:|
| 1k particles | 29,886 ns | 13,532,530 ns | 453x |
| 100 particles | 3,117 ns | 748,287 ns | 240x |
| 1k locations | 21,931 ns | 4,037,879 ns | 184x |
| 1k trades | 42,015 ns | 5,781,767 ns | 138x |
| 100 locations | 2,199 ns | 283,753 ns | 129x |

Full benchmark code: [benchmarks](./benchmarks)


## Why BoltFFI?

Serialization-based FFI is slow. UniFFI serializes every value to a byte buffer. wasm-bindgen materializes every struct as a JavaScript object. That overhead shows up even when you're making tens or hundreds of FFI calls per second.

BoltFFI uses zero-copy where possible. Primitives pass as raw values. Structs with primitive fields pass as pointers to memory both sides can read directly. WASM uses a wire buffer format that avoids per-field allocation. Only strings and collections go through encoding.

## What it does

Mark your Rust types with `#[data]` and functions with `#[export]`:

```rust
use boltffi::*;

#[data]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[export]
pub fn distance(a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    (dx * dx + dy * dy).sqrt()
}
```

Run BoltFFI for the targets you need:

```bash
boltffi pack all
# Produces: ./dist/apple/YourCrate.xcframework + Package.swift
# Produces: ./dist/android/jniLibs/<abi>/libyour_crate.so + Kotlin bindings
# Produces: ./dist/java/native/<host-target>/libyour_crate_jni.* + Java bindings
# Produces: ./dist/wasm/pkg/*.wasm + TypeScript bindings + npm package
# Produces: ./dist/csharp/packages/*.nupkg with RID native assets
# Produces: ./dist/python/wheelhouse/*.whl with Python package sources
```

Use it from Swift, Kotlin, Java, C#, TypeScript, or Python.

```swift
let d = distance(a: Point(x: 0, y: 0), b: Point(x: 3, y: 4)) // 5.0
```

```kotlin
val d = distance(a = Point(x = 0.0, y = 0.0), b = Point(x = 3.0, y = 4.0)) // 5.0
```

```java
double d = MyLib.distance(new Point(0.0, 0.0), new Point(3.0, 4.0)); // 5.0
```

```csharp
double d = MyLib.Distance(new Point(0.0, 0.0), new Point(3.0, 4.0)); // 5.0
```

```typescript
import { distance } from 'your-crate';
const d = distance({ x: 0, y: 0 }, { x: 3, y: 4 }); // 5.0
```

```python
import your_crate
d = your_crate.distance(your_crate.Point(0, 0), your_crate.Point(3, 4))  # 5.0
```

The generated bindings use each language's idioms. Swift gets async/await. Kotlin gets coroutines. Java gets CompletableFuture and functional interfaces. C# gets Tasks and async enumerables. TypeScript gets Promises. Errors become native exceptions.

## Supported languages

| Language | Status       |
|----------|--------------|
| Swift    | Full support |
| Kotlin   | Full support |
| Java     | Full support |
| C#       | Full support |
| WASM/TypeScript | Full support |
| C        | Partial      |
| Python   | Full support |
| C++      | Planned      |
| Ruby     | Planned      |
| Dart     | In progress  |
| Scala    | Planned      |
| Go       | Planned      |
| Lua      | Potential    |
| R        | Potential    |

Want another language? [Open an issue](https://github.com/boltffi/boltffi/issues).

## Installation

```bash
cargo install boltffi_cli
```

Prebuilt CLI archives are also attached to each [GitHub release](https://github.com/boltffi/boltffi/releases). Each archive has a matching `.sha256` checksum file. For Windows ARM64, download `boltffi-windows-arm64.zip` and `boltffi-windows-arm64.zip.sha256`.

Add BoltFFI to your library crate:

```bash
cargo add boltffi
```

Configure the crate type in `Cargo.toml`:

```toml
[lib]
crate-type = ["cdylib", "staticlib"]
```

## Documentation

- [Overview](https://boltffi.dev/docs/overview)
- [Getting Started](https://boltffi.dev/docs/getting-started)
- [Tutorial](https://boltffi.dev/docs/tutorial)
- [Types](https://boltffi.dev/docs/types)
- [Async](https://boltffi.dev/docs/async)
- [Streaming](https://boltffi.dev/docs/streaming)

## Alternative tools

Other tools that solve similar problems:

- [UniFFI](https://github.com/mozilla/uniffi-rs) - Mozilla's binding generator, uses serialization-based approach
- [Diplomat](https://github.com/rust-diplomat/diplomat) - Focused on C/C++ interop
- [cxx](https://github.com/dtolnay/cxx) - Safe C++/Rust interop

## Contributing
If this tool sounds interesting to you, please help us develop it. You can:

- View the [contributor guide](./docs/contributors/contributing.md).
- File or work on [issues](https://github.com/boltffi/boltffi/issues) here on GitHub.
- Join discussions on [Discord](https://discord.gg/Q6A7zNNFk3).

## License
BOLTFFI is released under the MIT license. See [LICENSE](./LICENSE) for more information.
