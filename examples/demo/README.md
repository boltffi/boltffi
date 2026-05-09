# BoltFFI Demo Crate

`examples/demo` is the canonical BoltFFI example crate. It exercises the full binding surface (primitives, records, enums, classes, callbacks, async, results, options, custom types) and doubles as:

- the **integration fixture** for every BoltFFI-supported platform (Apple / Kotlin / Android / Java / WASM / Python / C# / Dart), consumed from `examples/platforms/<lang>`;
- the **Rust source of truth** for the [benchmark suite](../../benchmarks), compared against UniFFI and wasm-bindgen using the same code under different feature flags.

If you are adding or changing BoltFFI functionality, this is almost certainly the crate you want to extend.

## Layout

```
examples/demo/
├── Cargo.toml                   # default features: none; optional: uniffi, wasm-bench
├── bench_macros/                # demo_bench_macros proc-macro crate (see below)
├── boltffi.toml                 # default config → outputs into examples/platforms/<lang>
├── boltffi.benchmark.toml       # overlay: redirects outputs into benchmarks/generated/boltffi
├── boltffi.python.ci.toml       # overlay: local dist/python for CI runs
├── verify-platform-demos.sh     # end-to-end smoke test across all host-supported platforms
└── src/
    ├── lib.rs
    ├── primitives/              # scalars, strings, vecs
    ├── records/                 # blittable, nested, with_strings, with_collections, …
    ├── enums/                   # c_style, data_enum, repr_int, complex_variants
    ├── classes/                 # constructors, methods, static_methods, streams,
    │                              thread_safe, unsafe_single_threaded, async_methods
    ├── callbacks/               # sync_traits, async_traits, closures
    ├── async_fns/               # async free functions
    ├── options/                 # Option<T> — primitives + complex
    ├── results/                 # Result<T, E> — basic, error_enums, nested, async
    ├── bytes/                   # Vec<u8> / &[u8]
    ├── builtins/                # chrono, uuid, url wiring
    ├── custom_types/            # user-defined wire types
    └── wasm_bench.rs            # only compiled with --features wasm-bench
```

Everything under `src/` is organized by **what BoltFFI feature it exercises**, not by what product/feature it solves. Keeping that split means a contributor can land a codegen fix and know exactly where to add coverage.

## Features

| Cargo feature | Purpose                                                                 | Who enables it                             |
|---------------|-------------------------------------------------------------------------|--------------------------------------------|
| *(default)*   | Plain BoltFFI exports via `#[export]` / `#[data]` / derive attributes.  | Every `examples/platforms/*` consumer.     |
| `uniffi`      | Turns on `#[uniffi::export]` shadow attributes through `benchmark_candidate`. Also compiles the UniFFI scaffolding setup in `lib.rs`. | UniFFI comparison benches only.            |
| `wasm-bench`  | Turns on `#[wasm_bindgen]` shadow attributes and compiles `wasm_bench.rs`. | wasm-bindgen comparison bench only.        |

The `uniffi` and `wasm-bench` features exist **for cross-binder benchmarking**. You should not need either for normal demo work; the regular BoltFFI path is the default.

## Config overlays

A single `boltffi.toml` won't cover every use. The crate ships three configs; the CLI picks one via `--overlay`:

| File                          | When it's used                                                      | Output goes to                         |
|-------------------------------|---------------------------------------------------------------------|----------------------------------------|
| `boltffi.toml`                | Default (no overlay).                                               | `examples/platforms/<lang>/…`          |
| `boltffi.benchmark.toml`      | `boltffi --overlay boltffi.benchmark.toml …` (via `just pack`).     | `benchmarks/generated/boltffi/…`       |
| `boltffi.python.ci.toml`      | Python CI wheel build.                                              | `examples/demo/dist/python`            |

Typical invocation from this directory:

```bash
# Regenerate Swift/Xcode artifacts into examples/platforms/apple/
cargo run -p boltffi_cli --manifest-path ../../Cargo.toml -- pack apple

# Same thing, but redirected at the benchmark tree
just pack apple                      # = boltffi --overlay boltffi.benchmark.toml pack apple
```

## The `benchmark_candidate` macro

Some items are exported not only through BoltFFI but also through UniFFI and/or wasm-bindgen so the [benchmark harnesses](../../benchmarks) can race them. This is driven by the [`demo_bench_macros`](./bench_macros) proc-macro crate, which expands to cfg-gated shadow attributes:

```rust
use boltffi::*;
use demo_bench_macros::benchmark_candidate;

#[export]                                                         // BoltFFI binding
#[benchmark_candidate(function, uniffi, wasm_bindgen)]            // + UniFFI + wasm-bindgen
pub fn echo_i32(v: i32) -> i32 { v }

#[data]
#[benchmark_candidate(record, uniffi)]                             // record, UniFFI only
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Point { pub x: f64, pub y: f64 }

#[benchmark_candidate(object, uniffi)]                             // stateful object
pub struct Counter { /* … */ }

#[export]
#[benchmark_candidate(impl, uniffi)]                               // impl block
impl Counter { /* … */ }
```

Expansion only fires under the relevant feature (`#[cfg_attr(feature = "uniffi", uniffi::export)]`, and the wasm-bindgen equivalent), so a default build stays pure BoltFFI.

Valid kinds: `function`, `record`, `enum`, `object`, `impl` (optionally `constructor = "name"`), `callback_interface`.
Valid targets: `uniffi`, `wasm_bindgen`.

If you add an item that **does not** need cross-binder comparison, just use plain `#[export]` and skip the macro entirely.

## Adding a new demo surface

1. Pick the right module under `src/` — for example, a new record goes in `src/records/`, a new class method in `src/classes/methods.rs`.
2. Write the Rust code with the usual BoltFFI attributes (`#[export]`, `#[data]`, `#[data(impl)]`, derive `Object`, etc.).
3. Re-export from the parent `mod.rs` and, if needed, from `src/lib.rs` so host platforms see it.
4. Regenerate bindings for whatever platforms you care about: `cargo run -p boltffi_cli -- pack <platform>` (or `just pack <platform>` for the benchmark overlay).
5. Exercise it from the corresponding `examples/platforms/<lang>` test or demo entry point.
6. *(Optional)* If this surface should also be benchmarked against UniFFI / wasm-bindgen, add `#[benchmark_candidate(...)]`, register a case in [`benchmarks/scripts/benchmark_catalog.py`](../../benchmarks/scripts/benchmark_catalog.py), and wire the harness — see the [benchmarks README](../../benchmarks/README.md) for the full flow.

When removing a surface, reverse this order: drop callers first, then platform bindings, then the Rust item.

## Running the demo on each platform

End-to-end smoke test across everything your host supports:

```bash
just demo-verify
# or
./examples/demo/verify-platform-demos.sh
```

This packs the artifacts, runs each platform's own test entry point, and fails on the first regression. Supported host → platforms:

- **macOS**: apple, kotlin, java, wasm, python
- **Linux / Windows**: java, wasm, python

Scope it down when iterating:

```bash
./examples/demo/verify-platform-demos.sh --platform apple
./examples/demo/verify-platform-demos.sh --platform python --python /path/to/python3
```

Per-platform entry points (all invoked for you by `verify-platform-demos.sh`):

| Platform | How it runs                                                                 |
|----------|-----------------------------------------------------------------------------|
| Apple    | `boltffi pack apple && swift test --package-path examples/platforms/apple`  |
| Kotlin   | `gradle -p examples/platforms/kotlin test`                                  |
| Java     | `boltffi pack java && examples/platforms/java/test-demo.sh --auto`          |
| WASM     | `boltffi pack wasm && examples/platforms/wasm/test-demo.sh`                 |
| Python   | `boltffi pack python --release --experimental && examples/platforms/python/test-demo.sh` |
| C#       | `examples/platforms/csharp/test-demo.sh` (runs `boltffi pack csharp`) |

Rust-side tests (the lib itself) still run via `cargo test -p demo` from this directory or `just test` from the repo root.

## Role in the benchmark suite

The benchmark suite does **not** maintain its own Rust code. Benchmarks compile this crate three times — once as BoltFFI (via the benchmark overlay), once with `--features uniffi`, and once with `--features wasm-bench` — so every comparison exercises the same Rust source against different binding generators. See [`benchmarks/README.md`](../../benchmarks/README.md) for the catalog, harnesses, and dashboard.

Short version: **touch this crate, add `#[benchmark_candidate]` if it should be benched, done.**

## Gotchas

- `lib.rs` calls `uniffi::setup_scaffolding!()` only behind `#[cfg(feature = "uniffi")]`. If you add new UniFFI-exported types, make sure they live under that feature's umbrella; otherwise the default build will fail to compile against UniFFI attributes.
- The `classes` module has two flavors: `thread_safe.rs` (wrapped in a `Mutex`) and `unsafe_single_threaded.rs` (plain `UnsafeCell`-style). They exist specifically to contrast method-call overhead, so keep both shapes when adding new class-style examples.
- `out.txt`, `dist/`, `target/`, `benchmarks/generated/`, and `rust-boltffi/` under this directory are build artifacts; do not commit them.
