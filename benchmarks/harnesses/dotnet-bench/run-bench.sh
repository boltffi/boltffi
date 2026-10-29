#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../../.."
DEMO_DIR="$ROOT_DIR/examples/demo"
BENCH_OVERLAY="$DEMO_DIR/boltffi.benchmark.toml"
BENCH_TARGET_DIR="$ROOT_DIR/benchmarks/generated/boltffi/target"
CSHARP_HOST_TARGET_DIR="$BENCH_TARGET_DIR/csharp-host"
RESULTS_DIR="$SCRIPT_DIR/build/results/dotnet"
ARTIFACTS_DIR="$RESULTS_DIR/artifacts"
PUBLISH=false
FILTER=""

cd "$SCRIPT_DIR"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --filter)
            FILTER="$2"
            shift 2
            ;;
        --publish)
            PUBLISH=true
            shift
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

mkdir -p "$RESULTS_DIR"
rm -rf "$ARTIFACTS_DIR"
rm -f "$RESULTS_DIR/results.json" "$RESULTS_DIR/benchmark_run.json"
mkdir -p "$ARTIFACTS_DIR"

export CARGO_TARGET_DIR="$BENCH_TARGET_DIR"

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

(
    export CARGO_TARGET_DIR="$CSHARP_HOST_TARGET_DIR"
    cargo build --release --manifest-path "$DEMO_DIR/Cargo.toml" --lib
)

mkdir -p "$BENCH_TARGET_DIR/release"
CSHARP_HOST_LIBRARY="$BENCH_TARGET_DIR/release/$HOST_LIBRARY"
cp "$CSHARP_HOST_TARGET_DIR/release/$HOST_LIBRARY" "$CSHARP_HOST_LIBRARY"

if command -v nm >/dev/null 2>&1 && ! nm -g "$CSHARP_HOST_LIBRARY" | grep -E '(^| )_?boltffi_make_point$' >/dev/null; then
    echo "Expected C# benchmark native library to export boltffi_make_point"
    exit 1
fi

(
    cd "$DEMO_DIR"
    cargo build -p boltffi_cli --release --manifest-path "$ROOT_DIR/Cargo.toml"
    cargo run -p boltffi_cli --manifest-path "$ROOT_DIR/Cargo.toml" -- \
        --overlay "$BENCH_OVERLAY" \
        generate csharp \
        --experimental
)

"$ROOT_DIR/benchmarks/adapters/uniffi/build-csharp.sh"

DOTNET_ARGS=("--filter" "${FILTER:-*}")

BOLTFFI_BENCH_ARTIFACTS="$ARTIFACTS_DIR" dotnet run -c Release -- "${DOTNET_ARGS[@]}"

REPORT_PATHS=()
while IFS= read -r report_path; do
    REPORT_PATHS+=("$report_path")
done < <(find "$ARTIFACTS_DIR/results" -name '*-report-full.json' -print | sort)
if [[ ${#REPORT_PATHS[@]} -eq 0 ]]; then
    echo "BenchmarkDotNet full JSON report not found under $ARTIFACTS_DIR/results" >&2
    exit 1
fi

python3 "$ROOT_DIR/benchmarks/scripts/benchmarkdotnet_to_run.py" \
    --results "${REPORT_PATHS[@]}" \
    --output "$RESULTS_DIR/benchmark_run.json" \
    --profile release

if [[ "$PUBLISH" == true ]]; then
    "$ROOT_DIR/benchmarks/scripts/publish-benchmark-runs.sh" "$RESULTS_DIR/benchmark_run.json"
fi
