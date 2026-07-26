# BoltFFI Benchmarks

Cross-language FFI performance suite. BoltFFI is compared against:

- **Swift / Kotlin (iOS, Android, macOS)**: UniFFI
- **Java (JVM)**: [uniffi-bindgen-java](https://github.com/IronCoreLabs/uniffi-bindgen-java) (FFM) and UniFFI (Kotlin/JNA)
- **WASM (Node.js)**: wasm-bindgen
- **C# (.NET)**: UniFFI (via [uniffi-bindgen-cs](https://github.com/NordSecurity/uniffi-bindgen-cs))

Every backend wraps the **same Rust code** with identical public APIs, so the only variable is FFI overhead.

## How the new system works

There used to be separate `rust-boltffi` / `rust-uniffi` / `rust-wasm-bindgen` crates, each re-implementing the same types and functions. That is gone. The Rust source of truth is now **[`examples/demo`](../examples/demo)** — the same crate used by the platform demos and integration tests. Benchmarks are just another consumer of its public surface.

Three pieces make this work:

1. **`#[benchmark_candidate]` macro** ([`examples/demo/bench_macros`](../examples/demo/bench_macros)). Annotate an item with the kinds of backends it should be exported to:

   ```rust
   #[benchmark_candidate(function, uniffi, wasm_bindgen)]
   pub fn echo_i32(value: i32) -> i32 { value }

   #[benchmark_candidate(record, uniffi, wasm_bindgen)]
   pub struct Location { /* ... */ }

   #[benchmark_candidate(object, uniffi)]
   pub struct Counter { /* ... */ }
   ```

   The macro expands to `#[cfg_attr(feature = "uniffi", uniffi::export)]` (and the wasm-bindgen equivalent) so the item is picked up by each backend only when its feature is enabled. BoltFFI itself discovers items through the regular `#[export]` / derive attributes already on the demo types — the macro is only about the *other* backends.

2. **Benchmark overlay** [`examples/demo/boltffi.benchmark.toml`](../examples/demo/boltffi.benchmark.toml). Same demo crate, different output paths. It redirects every BoltFFI artifact (xcframework, jniLibs, WASM pkg, Java/C# dist) into `benchmarks/generated/boltffi/…`, so benchmark builds never collide with the regular demo outputs. The CLI picks it up with `--overlay`.

3. **UniFFI / wasm-bindgen adapters** under [`benchmarks/adapters/uniffi`](./adapters/uniffi) and [`benchmarks/generated/wasm-bindgen`](./generated/wasm-bindgen). These don't contain Rust source — they are build scripts that compile `examples/demo` with `--features uniffi` or `--features wasm-bench` and run the respective binding generators. The produced libraries and bindings are consumed by the harnesses.

Layout:

```
benchmarks/
├── adapters/uniffi/       # UniFFI build glue (no src; compiles examples/demo --features uniffi)
├── generated/
│   ├── boltffi/           # BoltFFI outputs produced from the benchmark overlay
│   └── wasm-bindgen/      # wasm-bindgen outputs from examples/demo --features wasm-bench
├── harnesses/             # Runnable benchmark suites (one per platform)
│   ├── swift-macos-bench/
│   ├── ios-app/
│   ├── android-app/
│   ├── kotlin-jvm-bench/
│   ├── java-jvm-bench/
│   ├── python-bench/
│   ├── wasm-bench/
│   └── dotnet-bench/
└── scripts/               # Catalog, inventory, audit, normalization, publishing
```

## Where to see the results

Published runs live in the dashboard repo: **https://github.com/boltffi/benchmarks-dashboard**. Every tagged release of BoltFFI runs [`/.github/workflows/benchmark-release.yml`](../.github/workflows/benchmark-release.yml), which executes the full harness suite on macOS CI and commits the normalized JSON under `public/data/` in the dashboard repo. The dashboard UI reads from there.

Locally, each harness writes a raw report plus a normalized `benchmark_run.json` under its `build/results/…` directory:

| Harness        | Raw report                                            | Normalized document                                   |
|----------------|-------------------------------------------------------|-------------------------------------------------------|
| Swift          | `harnesses/swift-macos-bench/build/results/swift-benchmark/` | `.../benchmark_run.json` |
| Kotlin JMH     | `harnesses/kotlin-jvm-bench/build/results/jmh/report.txt`    | `.../benchmark_run.json` |
| Java JMH       | `harnesses/java-jvm-bench/build/results/jmh/results.json`    | `.../benchmark_run.json` |
| Python pyperf  | `harnesses/python-bench/build/results/pyperf/results.json`   | `.../benchmark_run.json` |
| WASM           | `harnesses/wasm-bench/build/results/benchmarkjs/`            | `.../benchmark_run.json` |
| .NET           | `harnesses/dotnet-bench/build/results/dotnet/results.json`   | `.../benchmark_run.json` |

To push local runs into a dashboard clone, point `BENCHMARK_ARCHIVE_REPO` at your checkout and run [`benchmarks/scripts/publish-benchmark-runs.sh`](./scripts/publish-benchmark-runs.sh). CI does the same via [`publish_benchmark_archive.py`](./scripts/publish_benchmark_archive.py).

## Running benchmarks

Prereqs:

```bash
just setup-targets            # rustup targets for the platforms you want
# Android: export ANDROID_NDK_HOME
# Java FFM: JDK 22+ and uniffi-bindgen-java on PATH (or UNIFFI_BINDGEN_JAVA)
# .NET:    dotnet SDK that supports net10.0
```

### All harnesses (what CI does)

The release workflow calls the `run-*.sh` scripts directly; you can do the same locally:

```bash
./benchmarks/harnesses/swift-macos-bench/run-bench.sh
./benchmarks/harnesses/kotlin-jvm-bench/run-jmh.sh
./benchmarks/harnesses/java-jvm-bench/run-jmh.sh
./benchmarks/harnesses/python-bench/run-bench.sh
./benchmarks/harnesses/wasm-bench/run-bench.sh
./benchmarks/harnesses/dotnet-bench/run-bench.sh
```

### Individual harnesses (day-to-day)

| Target              | Command                     | Notes                                                 |
|---------------------|-----------------------------|-------------------------------------------------------|
| Swift (macOS CLI)   | `just bench-swift`          | Builds xcframework, runs Swift Package bench          |
| Kotlin JMH (JVM)    | `just bench-kotlin`         | Builds Android-arch JNI libs, runs JMH                |
| Java FFM JMH (JVM)  | `just bench-java`           | Builds uniffi-bindgen-java bindings, runs JMH         |
| Python (pyperf)     | `just bench-python`         | Builds BoltFFI + UniFFI Python bindings, runs pyperf  |
| WASM (Node.js)      | `just bench-wasm`           | Builds both BoltFFI and wasm-bindgen wasm outputs     |
| C# (.NET)           | `just bench-csharp`         | BenchmarkDotNet; pass filters after `--`              |
| iOS                 | `just bench-build-ios`      | Produces xcframework; open the Xcode project to run   |
| Android             | `just bench-build-android`  | Produces jniLibs; open Android Studio to run          |

Filter examples:

```bash
just bench-csharp -- --filter '*String*'
# JMH (Kotlin / Java) accepts standard JMH arguments via Gradle:
#   cd benchmarks/harnesses/kotlin-jvm-bench && ./gradlew jmh -Pjmh.include='.*echo.*'
```

Clean artifacts: `just clean-benchmarks`.

## Adding a benchmark

Benchmarks are defined in Rust in `examples/demo`. You do **not** touch separate bench crates anymore.

1. **Write (or pick) the Rust item** in `examples/demo/src/…` where it logically belongs (`primitives/`, `records/`, `classes/`, etc.).
2. **Annotate it** with `#[benchmark_candidate]`, declaring which comparison backends should export it:

   ```rust
   use demo_bench_macros::benchmark_candidate;

   #[benchmark_candidate(function, uniffi, wasm_bindgen)]
   pub fn sum_my_thing(xs: Vec<i32>) -> i64 { xs.iter().map(|x| *x as i64).sum() }
   ```

   Kinds: `function`, `record`, `enum`, `object`, `impl` (optionally `constructor = "new"`), `callback_interface`. Targets: `uniffi`, `wasm_bindgen`. BoltFFI export comes from the normal `#[export]` / derive attributes that already sit on the item.
3. **Register it in the catalog** ([`benchmarks/scripts/benchmark_catalog.py`](./scripts/benchmark_catalog.py)). Add a `_case(...)` entry with a canonical name, group, category, and parameters. The catalog is the shared vocabulary that every harness normalizes into — without an entry, the harness output will not map cleanly into `benchmark_run.json`.
4. **Call it from each harness** that should time it. This is still hand-written, but the code is now thin (just call the bound function or construct the bound object). Harness sources:
   - Swift: [`harnesses/swift-macos-bench/Sources/{BoltFFI,Uniffi,AsyncRunner}/main.swift`](./harnesses/swift-macos-bench/Sources)
   - Kotlin (JNI + JNA/UniFFI): [`harnesses/kotlin-jvm-bench/src/main/kotlin/com/example/bench_compare/JmhBenchmarks.kt`](./harnesses/kotlin-jvm-bench/src/main/kotlin/com/example/bench_compare/JmhBenchmarks.kt)
   - Java FFM: [`harnesses/java-jvm-bench/src/jmh/java/com/example/bench_compare/{BoltffiJavaBench,UniffiJavaBench}.java`](./harnesses/java-jvm-bench/src/jmh/java/com/example/bench_compare)
   - Python: [`harnesses/python-bench/bench.py`](./harnesses/python-bench/bench.py)
   - WASM: [`harnesses/wasm-bench/bench.mjs`](./harnesses/wasm-bench/bench.mjs)
   - .NET: [`harnesses/dotnet-bench/{WireReaderBenchmarks,EnumWireBenchmarks}.cs`](./harnesses/dotnet-bench)
   - iOS / Android: the harness apps under [`harnesses/ios-app`](./harnesses/ios-app) and [`harnesses/android-app`](./harnesses/android-app)
5. **Verify discovery and coverage**:

   ```bash
   just bench-audit        # harness names must match the catalog
   just bench-demo-audit   # how much of the demo export surface is benchmarked
   just bench-demo-plan    # machine-readable benchmark family policy
   ```
6. **Run the harness locally**, confirm the new case appears in both the raw report and `benchmark_run.json`, then ship it.

## Removing a benchmark

Reverse of the above, in this order (so audits stay green at every step):

1. Delete the harness calls in every `harnesses/*/…` file that references the case.
2. Remove the entry from [`benchmarks/scripts/benchmark_catalog.py`](./scripts/benchmark_catalog.py).
3. If the Rust item exists **only** for benchmarking, remove it from `examples/demo/…` (and drop the `#[benchmark_candidate]` attribute if the item stays but should no longer be re-exported via UniFFI / wasm-bindgen).
4. Run `just bench-audit && just bench-demo-audit` — both should be clean.
5. If the case shows up in the dashboard, rename-safe handling is built into `publish_benchmark_archive.py`; historical data for the removed case stays in the archive under its old name.

## Tracking a new benchmark on the dashboard

Nothing extra is needed as long as the case is in the catalog and the harness emits it. The release workflow will pick it up on the next tag: it runs the harnesses, normalizes their outputs with the `*_to_run.py` scripts, and commits into `boltffi/benchmarks-dashboard`. For a dry run before a tag:

```bash
# run any subset of harnesses locally, then:
BENCHMARK_ARCHIVE_REPO=/path/to/benchmarks-dashboard \
  ./benchmarks/scripts/publish-benchmark-runs.sh
```

Inspect `public/data/` in the dashboard checkout — that is exactly what the hosted site will render.

## Why this matters

FFI has inherent costs: crossing the language boundary, converting types, copying memory. UniFFI uses a runtime approach with serialization similar to JSON. BoltFFI generates specialized code at compile time that avoids most of this overhead. These benchmarks isolate the FFI layer by using trivial Rust implementations (just constructing data or summing numbers).

## Benchmark surface (summary)

- **Call overhead**: `noop`.
- **Primitives**: `echo_i32`, `echo_f64`, `add`, `multiply`, `inc_u64`.
- **Strings**: `echo_string_small`, `echo_string_1k`.
- **Struct generation (Rust → host)**: `generate_{locations,trades,particles,sensors,user_profiles}_{100,1k,10k}`.
- **Struct consumption (host → Rust)**: `sum_ratings`, `process_locations`, `sum_trade_volumes`, `sum_particle_masses`, `avg_sensor_temp`, `sum_user_scores`, `count_active_users`.
- **Primitive vectors**: `generate_i32_vec_*`, `sum_i32_vec_*`, `generate_f64_vec_*`, `sum_f64_vec_*`, `generate_bytes_64k`.
- **Classes / stateful objects**: `counter_increment`, `datastore_add`, `accumulator`.
- **Enums**: `simple_enum`, `data_enum_input`, `find_even`.
- **Async**: `async_add`.
- **Callbacks (foreign traits)**: `callback_100`, `callback_1k`.

The authoritative list at any commit is what [`demo_export_inventory.py`](./scripts/demo_export_inventory.py) reports intersected with the catalog; `just bench-demo-plan` prints the structured form.

---

## Results

**Live results live on the dashboard: https://github.com/boltffi/benchmarks-dashboard.** The tables below are a static snapshot from v0.24.0 on Apple M3 / M4 Max and are kept here for quick reference only — the dashboard is the source of truth.

### JVM (JMH on Apple M4 Max)

Three-way comparison: BoltFFI (JNI), uniffi-bindgen-java (Java FFM), and UniFFI (Kotlin/JNA). All benchmarks run on JDK 25 using the same Rust benchmark library (`bench_uniffi` for FFM/JNA, `bench_boltffi` for JNI) with identical data structures. Times in nanoseconds (lower is better).

| Benchmark                         | BoltFFI (JNI) | uniffi-bindgen-java (FFM) | UniFFI (Kotlin/JNA) |
|-----------------------------------|---------------|---------------------------|---------------------|
| noop                              | 3 ns          | 5 ns                      | 2,418 ns            |
| echo_i32                          | 3 ns          | 5 ns                      | 2,440 ns            |
| add                               | 3 ns          | 4 ns                      | 2,324 ns            |
| inc_u64                           | 98 ns         | 5 ns                      | 2,356 ns            |
| echo_string_small                 | 227 ns        | 145 ns                    | 9,733 ns            |
| echo_string_1k                    | 482 ns        | 1,075 ns                  | 12,404 ns           |
| simple_enum                       | 17 ns         | 157 ns                    | 16,482 ns           |
| find_even (100x)                  | 11,142 ns     | 5,431 ns                  | 650,098 ns          |
| generate_locations_1k             | 7,039 ns      | 16,640 ns                 | 25,642 ns           |
| generate_locations_10k            | 48,595 ns     | 132,814 ns                | 177,723 ns          |
| generate_trades_1k                | 8,331 ns      | 21,579 ns                 | 22,284 ns           |
| generate_trades_10k               | 65,979 ns     | 183,427 ns                | 144,455 ns          |
| generate_particles_1k             | 8,615 ns      | 25,930 ns                 | 22,920 ns           |
| generate_particles_10k            | 68,912 ns     | 229,298 ns                | 152,793 ns          |
| generate_sensors_1k               | 8,701 ns      | 23,050 ns                 | 33,603 ns           |
| generate_sensors_10k              | 69,623 ns     | 215,935 ns                | 273,687 ns          |
| generate_user_profiles_100        | 28,517 ns     | 35,303 ns                 | 37,892 ns           |
| generate_user_profiles_1k         | 287,604 ns    | 352,007 ns                | 316,651 ns          |
| sum_ratings_1k                    | 5,829 ns      | 12,167 ns                 | 30,111 ns           |
| sum_ratings_10k                   | 74,003 ns     | 110,174 ns                | 214,745 ns          |
| sum_trade_volumes_1k              | 10,672 ns     | 18,146 ns                 | 36,977 ns           |
| sum_trade_volumes_10k             | 50,608 ns     | 166,004 ns                | 324,859 ns          |
| sum_particle_masses_1k            | 9,893 ns      | 21,557 ns                 | 32,419 ns           |
| sum_particle_masses_10k           | 162,794 ns    | 204,995 ns                | 254,240 ns          |
| avg_sensor_temp_1k                | 13,159 ns     | 18,856 ns                 | 37,246 ns           |
| avg_sensor_temp_10k               | 134,216 ns    | 173,644 ns                | 332,399 ns          |
| process_locations_1k              | 8,297 ns      | 11,552 ns                 | 29,517 ns           |
| process_locations_10k             | 70,105 ns     | 104,063 ns                | 215,461 ns          |
| sum_user_scores_100               | 21,331 ns     | 54,682 ns                 | 51,915 ns           |
| sum_user_scores_1k                | 228,259 ns    | 528,509 ns                | 465,332 ns          |
| count_active_users_100            | 21,718 ns     | 52,367 ns                 | 51,927 ns           |
| count_active_users_1k             | 232,519 ns    | 511,834 ns                | 464,459 ns          |
| generate_i32_vec_10k              | 3,142 ns      | 7,390 ns                  | 46,949 ns           |
| generate_i32_vec_100k             | 22,631 ns     | 46,407 ns                 | 234,372 ns          |
| generate_f64_vec_10k              | 6,331 ns      | 9,452 ns                  | 39,585 ns           |
| generate_bytes_64k                | 5,298 ns      | 23,900 ns                 | 30,699 ns           |
| sum_i32_vec_10k                   | 2,706 ns      | 12,586 ns                 | 47,535 ns           |
| sum_i32_vec_100k                  | 36,247 ns     | 99,880 ns                 | 390,410 ns          |
| sum_f64_vec_10k                   | 9,061 ns      | 18,145 ns                 | 52,774 ns           |
| counter_increment (1k calls)      | 6,451 ns      | 18,322 ns                 | 4,610,792 ns        |
| datastore_add (1k items)          | 91,627 ns     | 125,735 ns                | 8,958,340 ns        |
| accumulator (1k calls)            | 6,198 ns      | 18,369 ns                 | 4,495,467 ns        |

BoltFFI's `counter_increment_single_threaded` (no Mutex): 4,349 ns, `accumulator_single_threaded`: 3,120 ns.

### Swift (macOS, Apple M3)

These are actual results from running `just bench-swift` on Apple M3 chip:

| Benchmark | BoltFFI | UniFFI | Speedup |
|-----------|--------:|-------:|--------:|
| noop | <1 ns | 1,416 ns | >1000x |
| echo_i32 | <1 ns | 1,416 ns | >1000x |
| echo_string_small | 125 ns | 4,292 ns | 34x |
| echo_string_1k | 10,209 ns | 14,292 ns | 1.4x |
| generate_locations_1k | 4,167 ns | 1,276,333 ns | 306x |
| generate_locations_10k | 62,542 ns | 12,817,000 ns | 205x |
| generate_trades_1k | 12,208 ns | 1,920,000 ns | 157x |
| generate_user_profiles_100 | 65,125 ns | 505,250 ns | 7.8x |
| generate_user_profiles_1k | 701,604 ns | 5,174,792 ns | 7.4x |
| sum_i32_vec_10k | 833 ns | 69,959 ns | 84x |
| counter_increment (1k calls) | 1,083 ns | 1,388,895 ns | 1,282x |
| datastore_add (1k items) | 54,125 ns | 2,911,833 ns | 54x |
| process_locations_1k | 542 ns | 43,125 ns | 80x |
| callback_100 | 14,834 ns | 203,791 ns | 13.7x |
| callback_1k | 142,959 ns | 1,970,291 ns | 13.8x |

### WASM (Node.js)

Results from `just bench-wasm` on Apple M3:

| Benchmark | BoltFFI | wasm-bindgen | Speedup |
|-----------|--------:|-------------:|--------:|
| noop | 2 ns | 2 ns | 1x |
| echo_i32 | 2 ns | 2 ns | 1x |
| echo_f64 | 2 ns | 2 ns | 1x |
| add | 2 ns | 2 ns | 1x |
| multiply | 2 ns | 2 ns | 1x |
| echo_string_200 | 487 ns | 763 ns | 1.6x |
| echo_string_1k | 806 ns | 2,921 ns | 3.6x |
| generate_string_1k | 231 ns | 241 ns | 1x |
| generate_locations_100 | 2,199 ns | 283,753 ns | 129x |
| generate_locations_1k | 21,931 ns | 4,037,879 ns | 184x |
| generate_trades_100 | 5,595 ns | 616,253 ns | 110x |
| generate_trades_1k | 42,015 ns | 5,781,767 ns | 138x |
| generate_particles_100 | 3,117 ns | 748,287 ns | 240x |
| generate_particles_1k | 29,886 ns | 13,532,530 ns | 453x |
| generate_i32_vec_1k | 623 ns | 559 ns | -1.1x |
| generate_i32_vec_10k | 3,667 ns | 3,493 ns | 1x |
| generate_bytes_64k | 2,973 ns | 2,973 ns | 1x |
| roundtrip_locations_100 | 15,467 ns | 24,587 ns | 1.6x |
| roundtrip_i32_vec_1k | 1,305 ns | 1,228 ns | -1.1x |
| counter_increment_1k | 2,382 ns | 2,594 ns | 1.1x |
| datastore_add_1k | 91,226 ns | 115,574 ns | 1.3x |
| accumulator_1k | 14,096 ns | 13,778 ns | 1x |
| find_even_100 | 172 ns | 173 ns | 1x |
| async_add | 243 ns | 327 ns | 1.3x |

#### So who wins?

1. For pure primitives (integers, floats, scalars), both tie at ~2ns.
2. For strings, BoltFFI is 1.6-3.6x faster.
3. For structured data (records, arrays of structs), BoltFFI is **110-453x faster**.
4. For primitive vectors (`Vec<i32>`, `Vec<u8>`), both tie.

BoltFFI wins for real-world mixed data, and ties or is slightly slower than wasm-bindgen on scalar types.
