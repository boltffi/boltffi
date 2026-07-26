# BoltFFI Justfile

set shell := ["bash", "-cu"]

default:
    @just --list

# ─────────────────────────────────────────────────────────────────────────────
# Setup
# ─────────────────────────────────────────────────────────────────────────────

# Install Rust targets for cross-compilation
setup-targets:
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
    rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android

# Install development tools (cargo-insta, cargo-nextest)
setup-tools:
    cargo install cargo-insta cargo-nextest

# Full development setup
setup: setup-targets setup-tools

# Install boltffi CLI to ~/.cargo/bin
install:
    cargo install --path boltffi_cli --force

# Run boltffi pack for the benchmark overlay
pack *args:
    cd examples/demo && cargo run -p boltffi_cli --manifest-path ../../Cargo.toml -- --overlay boltffi.benchmark.toml pack {{args}}

# ─────────────────────────────────────────────────────────────────────────────
# Build
# ─────────────────────────────────────────────────────────────────────────────

# Build boltffi CLI (debug)
build:
    cargo build -p boltffi_cli

# Build boltffi CLI (release)
build-release:
    cargo build -p boltffi_cli --profile release-lto

# Build entire workspace (debug)
build-all:
    cargo build --workspace

# Build entire workspace (release)
build-all-release:
    cargo build --workspace --release

# ─────────────────────────────────────────────────────────────────────────────
# Test
# ─────────────────────────────────────────────────────────────────────────────

# Run all workspace tests
test:
    cargo test --workspace

# Verify demo platforms
demo-verify *args:
    ./examples/demo/verify-platform-demos.sh {{args}}

# Generate demo bindings through the IR path
demo-generate target *args:
    cd examples/demo && cargo run -p boltffi_cli --manifest-path ../../Cargo.toml -- generate {{target}} --experimental --ir {{args}}

# Audit semantic demo test cases against platform test markers
demo-test-audit:
    cargo run --manifest-path examples/demo/Cargo.toml --bin demo-tests -- audit

# Report semantic demo test cases and platform support
demo-test-report:
    cargo run --manifest-path examples/demo/Cargo.toml --bin demo-tests -- report

# Run tests with cargo-nextest (parallel, faster)
test-nextest:
    cargo nextest run --workspace

# Run tests for a single crate
test-crate crate:
    cargo test -p {{crate}}

# Run bindgen snapshot tests only
test-snapshots:
    cargo test -p boltffi_bindgen

# Accept snapshot changes
snapshots-accept:
    cargo insta test --accept

# Run Miri for undefined behavior detection (requires nightly)
test-miri:
    cargo +nightly miri test -p boltffi -p boltffi_tests

# ─────────────────────────────────────────────────────────────────────────────
# Lint & Format
# ─────────────────────────────────────────────────────────────────────────────

# Check code formatting
fmt-check:
    cargo fmt --all -- --check

# Format all code
fmt:
    cargo fmt --all

# Run clippy lints
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Run format check + clippy + tests
check: fmt-check lint test bench-script-test

# ─────────────────────────────────────────────────────────────────────────────
# Benchmarks
# ─────────────────────────────────────────────────────────────────────────────

# Audit benchmark harness names against the shared catalog
bench-audit:
    PYTHONDONTWRITEBYTECODE=1 python3 benchmarks/scripts/audit_benchmark_catalog.py

bench-script-test:
    PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s benchmarks/scripts -p 'test_*.py'

# Audit benchmark coverage against the callable exports in examples/demo
bench-demo-audit:
    PYTHONDONTWRITEBYTECODE=1 python3 benchmarks/scripts/audit_demo_export_coverage.py

# Render the machine-readable demo benchmark family plan
bench-demo-plan:
    PYTHONDONTWRITEBYTECODE=1 python3 benchmarks/scripts/render_demo_benchmark_policy.py

# Swift benchmark (macOS CLI) - builds xcframework and runs benchmark
bench-swift:
    #!/usr/bin/env bash
    set -e
    tmpfile=$(mktemp /tmp/boltffi_bench_swift_XXXXXX.txt)
    trap "rm -f $tmpfile" EXIT
    cd benchmarks/generated/boltffi
    ./build.sh --platform apple --release 2>&1 | tee "$tmpfile"
    echo ""
    echo "=== Summary ==="
    python3 ../harnesses/swift-macos-bench/format_bench.py < "$tmpfile"

# Kotlin benchmark (JVM via JMH) - builds JNI libs, runs JMH, generates report
bench-kotlin:
    #!/usr/bin/env bash
    set -e
    echo "=== Building BoltFFI for Android (Kotlin bindings + JNI glue) ==="
    cd benchmarks/generated/boltffi && ./build.sh --platform android --release --skip-bench
    
    echo "=== Building UniFFI Kotlin baseline ==="
    cd ../adapters/uniffi && ./build-kotlin.sh
    
    echo "=== Building desktop JNI library ==="
    cd ../harnesses/kotlin-jvm-bench && ./build-jni.sh
    
    echo "=== Running JMH benchmarks ==="
    ./gradlew jmh --rerun
    
    echo "=== Generating report ==="
    python3 jmh_report.py --format both
    echo ""
    echo "Report: $(pwd)/build/results/jmh/report.txt"

# Java benchmark (JVM via JMH) - builds uniffi-bindgen-java FFM bindings, runs JMH
bench-java:
    #!/usr/bin/env bash
    set -e
    echo "=== Building UniFFI Java FFM bindings ==="
    cd benchmarks/adapters/uniffi && ./build-java.sh

    echo "=== Running JMH benchmarks ==="
    cd ../harnesses/java-jvm-bench && ./gradlew jmh --rerun

    echo ""
    echo "Report: $(pwd)/build/results/jmh/results.json"

# C# benchmark (.NET via BenchmarkDotNet) - builds cdylib, generates bindings, runs benchmarks.
# Pass extra args after --, e.g. `just bench-csharp -- --filter '*String*'`.
bench-csharp *args:
    #!/usr/bin/env bash
    set -e
    cd benchmarks/harnesses/dotnet-bench
    if [ -n "{{ args }}" ]; then
        ./run-bench.sh {{ args }}
    else
        ./run-bench.sh
    fi

# Build xcframework only (for iOS development in Xcode)
bench-build-ios:
    cd benchmarks/generated/boltffi && ./build.sh --platform apple --release --skip-bench
    @echo ""
    @echo "xcframework ready. Open benchmarks/harnesses/ios-app/ in Xcode."

# Build jniLibs only (for Android development in Android Studio)
bench-build-android:
    cd benchmarks/generated/boltffi && ./build.sh --platform android --release --skip-bench
    @echo ""
    @echo "jniLibs ready. Open benchmarks/harnesses/android-app/ in Android Studio."

# WASM benchmark (Node.js) - builds wasm, runs benchmark
bench-wasm:
    #!/usr/bin/env bash
    set -e
    echo "=== Building BoltFFI WASM ==="
    cd examples/demo && CARGO_TARGET_DIR=../../benchmarks/generated/boltffi/target cargo run -p boltffi_cli --manifest-path ../../Cargo.toml -- --overlay boltffi.benchmark.toml pack wasm --release --regenerate
    
    echo "=== Building wasm-bindgen baseline ==="
    cd ../../examples/demo && CARGO_TARGET_DIR=../../benchmarks/generated/wasm-bindgen/target cargo build --target wasm32-unknown-unknown --release --features wasm-bench
    wasm-bindgen --target experimental-nodejs-module --out-dir ../../benchmarks/generated/wasm-bindgen/dist ../../benchmarks/generated/wasm-bindgen/target/wasm32-unknown-unknown/release/demo.wasm
    
    echo "=== Copying to benchmark runner ==="
    mkdir -p ../harnesses/wasm-bench/build/generated/boltffi ../harnesses/wasm-bench/build/generated/wasmbindgen
    cp -r ../generated/boltffi/dist/wasm/pkg/* ../harnesses/wasm-bench/build/generated/boltffi/
    cp -r ../generated/wasm-bindgen/dist/* ../harnesses/wasm-bench/build/generated/wasmbindgen/
    
    echo "=== Running benchmarks ==="
    cd ../harnesses/wasm-bench && npm ci --silent && node bench.mjs

# Python benchmark (pyperf) - builds BoltFFI + UniFFI Python bindings, runs pyperf
bench-python *args:
    #!/usr/bin/env bash
    set -e
    cd benchmarks/harnesses/python-bench
    if [ -n "{{ args }}" ]; then
        ./run-bench.sh {{ args }}
    else
        ./run-bench.sh
    fi

# ─────────────────────────────────────────────────────────────────────────────
# Clean
# ─────────────────────────────────────────────────────────────────────────────

# Clean workspace target/
clean:
    cargo clean

# Clean benchmark build artifacts
clean-benchmarks:
    rm -rf benchmarks/generated/boltffi/target
    rm -rf benchmarks/generated/boltffi/dist
    rm -rf benchmarks/adapters/uniffi/target
    rm -rf benchmarks/adapters/uniffi/dist
    rm -rf benchmarks/generated/wasm-bindgen/target
    rm -rf benchmarks/generated/wasm-bindgen/dist
    rm -rf benchmarks/harnesses/swift-macos-bench/.build
    rm -rf benchmarks/harnesses/swift-macos-bench/.swiftpm
    rm -rf benchmarks/harnesses/swift-macos-bench/build
    rm -rf benchmarks/harnesses/kotlin-jvm-bench/build
    rm -rf benchmarks/harnesses/kotlin-jvm-bench/.gradle
    rm -rf benchmarks/harnesses/kotlin-jvm-bench/.kotlin
    rm -rf benchmarks/harnesses/kotlin-jvm-bench/hs_err_pid*.log
    rm -rf benchmarks/harnesses/java-jvm-bench/build
    rm -rf benchmarks/harnesses/java-jvm-bench/.gradle
    rm -rf benchmarks/harnesses/dotnet-bench/bin
    rm -rf benchmarks/harnesses/dotnet-bench/obj
    rm -rf benchmarks/harnesses/dotnet-bench/BenchmarkDotNet.Artifacts
    rm -rf benchmarks/harnesses/dotnet-bench/build
    rm -rf benchmarks/harnesses/wasm-bench/build
    rm -rf benchmarks/harnesses/wasm-bench/node_modules
    rm -rf benchmarks/harnesses/python-bench/build
    rm -rf benchmarks/harnesses/android-app/.gradle
    rm -rf benchmarks/harnesses/android-app/.kotlin
    rm -rf benchmarks/harnesses/android-app/build
    rm -rf benchmarks/harnesses/android-app/app/build
    rm -rf benchmarks/harnesses/ios-app/.build

# Clean everything
clean-all: clean clean-benchmarks

# ─────────────────────────────────────────────────────────────────────────────
# CI
# ─────────────────────────────────────────────────────────────────────────────

# Run CI checks locally (format + lint + test)
ci: fmt-check lint test bench-script-test

# Run full CI including Miri (slow)
ci-full: ci test-miri
