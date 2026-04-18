#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../../.."
DEMO_DIR="$ROOT_DIR/examples/demo"
BENCH_OVERLAY="$DEMO_DIR/boltffi.benchmark.toml"
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
mkdir -p "$ARTIFACTS_DIR"

export CARGO_TARGET_DIR="$ROOT_DIR/benchmarks/generated/boltffi/target"

(
    cd "$DEMO_DIR"
    cargo build --release --manifest-path "$DEMO_DIR/Cargo.toml" --lib
    cargo build -p boltffi_cli --release --manifest-path "$ROOT_DIR/Cargo.toml"
    cargo run -p boltffi_cli --manifest-path "$ROOT_DIR/Cargo.toml" -- \
        --overlay "$BENCH_OVERLAY" \
        generate csharp \
        --experimental
)
# The default .NET harness runs the BoltFFI benchmarks only.
# The UniFFI C# lane is opt-in and goes through the compat generator path in
# benchmarks/adapters/uniffi/build-csharp.sh when we want to run it.

dotnet_run_command=(dotnet run -c Release -- --filter "${FILTER:-*}")

BOLTFFI_BENCH_ARTIFACTS="$ARTIFACTS_DIR" "${dotnet_run_command[@]}"

REPORT_PATH="$(find "$ARTIFACTS_DIR/results" -name '*-report-full.json' -print | sort | tail -n1)"
if [[ -z "$REPORT_PATH" ]]; then
    echo "BenchmarkDotNet full JSON report not found under $ARTIFACTS_DIR/results" >&2
    exit 1
fi

cp "$REPORT_PATH" "$RESULTS_DIR/results.json"

python3 "$ROOT_DIR/benchmarks/scripts/benchmarkdotnet_to_run.py" \
    --results "$RESULTS_DIR/results.json" \
    --output "$RESULTS_DIR/benchmark_run.json" \
    --profile release

if [[ "$PUBLISH" == true ]]; then
    "$ROOT_DIR/benchmarks/scripts/publish-benchmark-runs.sh" "$RESULTS_DIR/benchmark_run.json"
fi
