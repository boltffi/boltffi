# BoltFFI

A high-performance multi-language bindings generator for Rust. Up to 1,000x faster than UniFFI. Up to 450x faster than wasm-bindgen.

<p align="center">
  <img src="docs/assets/demo.gif" width="700" />
</p>

<p align="center">
  <a href="https://discord.gg/udqGGXA4">
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
boltffi pack apple
# Produces: ./dist/apple/YourCrate.xcframework + Package.swift

boltffi pack android
# Produces: ./dist/android/jniLibs/<abi>/libyour_crate.so + Kotlin bindings

boltffi pack java
# Produces: ./dist/java/native/<host-target>/libyour_crate_jni.* + Java bindings

boltffi pack wasm
# Produces: ./dist/wasm/pkg/*.wasm + TypeScript bindings + npm package

boltffi pack csharp
# Produces: ./dist/csharp/packages/*.nupkg with RID native assets
```

Android ABI selection is configurable in `boltffi.toml`:

```toml
[targets.android]
architectures = ["arm64"]
```

Apple slice selection is configurable too:

```toml
[targets.apple]
ios_architectures = ["arm64"]
simulator_architectures = ["arm64"]
include_macos = true
macos_architectures = ["arm64"]
```

Any Apple architecture list can be set to `[]` to exclude that slice family. For example, set
`ios_architectures = []` when you want simulator-only Apple packaging from a config overlay. BoltFFI
still requires at least one Apple slice overall.

When `architectures` is omitted, BoltFFI keeps the existing default Android matrix:
`arm64`, `armv7`, `x86_64`, and `x86`. `boltffi pack android --no-build` now validates that each
configured ABI already has a built Rust static library and ignores stale artifacts for
unconfigured ABIs.

You can also apply per-invocation overlays without mutating the tracked base config.
BoltFFI still requires a base `boltffi.toml`, then merges the overlay on top for that one command:

```bash
boltffi --overlay boltffi.ci.toml pack android
boltffi --overlay boltffi.release.toml pack all --release
```

This is useful for CI, release builds, or machine-local overrides. `boltffi init` does not accept
`--overlay`.

Use it from Swift, Kotlin, Java, C#, or TypeScript:

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
| Python   | In progress  |
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

Add to your `Cargo.toml`:

```toml
[dependencies]
boltffi = "0.1"

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
Contributions are warmly welcomed 🙌

- [File an issue](https://github.com/boltffi/boltffi/issues)
- [Submit a PR](https://github.com/boltffi/boltffi/pulls)

## License
BOLTFFI is released under the MIT license. See [LICENSE](./LICENSE) for more information.
