#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../../.."
DEMO_DIR="$ROOT_DIR/examples/demo"
BENCH_TARGET_DIR="$SCRIPT_DIR/target"
HOST_TARGET_DIR="$BENCH_TARGET_DIR/kotlin-host"

export CARGO_TARGET_DIR="$BENCH_TARGET_DIR"

(
    export CARGO_TARGET_DIR="$HOST_TARGET_DIR"
    cargo build --manifest-path "$DEMO_DIR/Cargo.toml" --lib --release
)

case "$(uname -s)" in
    Darwin)
        HOST_LIBRARY="libdemo.dylib"
        ;;
    Linux)
        HOST_LIBRARY="libdemo.so"
        ;;
    *)
        echo "Unsupported host platform"
        exit 1
        ;;
esac

mkdir -p "$BENCH_TARGET_DIR/release"
cp "$HOST_TARGET_DIR/release/$HOST_LIBRARY" "$BENCH_TARGET_DIR/release/$HOST_LIBRARY"

rm -rf "$SCRIPT_DIR/dist/android/kotlin" "$SCRIPT_DIR/dist/android/include"

cd "$DEMO_DIR"
cargo run --manifest-path "$ROOT_DIR/Cargo.toml" -p boltffi_cli -- \
    --overlay boltffi.benchmark.toml \
    generate kotlin

mkdir -p "$SCRIPT_DIR/dist/android/include"
cp "$SCRIPT_DIR/dist/android/kotlin/jni/demo.h" "$SCRIPT_DIR/dist/android/include/demo.h"

perl -0pi -e 's/DataPointReader\.read\(buffer, 0\)/DataPoint.decode(WireReader(buffer))/g' \
    "$SCRIPT_DIR/dist/android/kotlin/com/example/bench_boltffi/Demo.kt"
