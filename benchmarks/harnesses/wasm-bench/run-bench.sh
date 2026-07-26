#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../../.."
DEMO_DIR="$ROOT_DIR/examples/demo"
DEMO_WASM_PKG_DIR="$ROOT_DIR/examples/platforms/wasm/dist"
RUNTIME_DIR="$ROOT_DIR/runtime/typescript"
BENCH_OVERLAY="$DEMO_DIR/boltffi.benchmark.toml"
RESULTS_DIR="$SCRIPT_DIR/build/results/benchmarkjs"
GENERATED_DIR="$SCRIPT_DIR/build/generated"
PUBLISH=false
BENCH_FILTER=""
WASM_RUST_TARGET="wasm32-unknown-unknown"

cd "$SCRIPT_DIR"

resolve_wasm_bindgen_version() {
    cargo metadata --manifest-path "$DEMO_DIR/Cargo.toml" --format-version 1 \
        | python3 -c 'import json, sys; print(next(package["version"] for package in json.load(sys.stdin)["packages"] if package["name"] == "wasm-bindgen"))'
}

ensure_wasm_bindgen_cli() {
    local wasm_bindgen_version
    wasm_bindgen_version="$(resolve_wasm_bindgen_version)"

    if command -v wasm-bindgen >/dev/null 2>&1 && [[ "$(wasm-bindgen --version)" == "wasm-bindgen $wasm_bindgen_version" ]]; then
        return
    fi

    cargo install wasm-bindgen-cli --version "$wasm_bindgen_version" --locked --force
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --publish)
            PUBLISH=true
            shift
            ;;
        --filter)
            BENCH_FILTER="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

mkdir -p "$RESULTS_DIR"
mkdir -p "$GENERATED_DIR"

rustup target add "$WASM_RUST_TARGET"

(
    cd "$RUNTIME_DIR"
    npm ci
    npm run build
)

export PATH="$RUNTIME_DIR/node_modules/.bin:$PATH"

npm ci
ensure_wasm_bindgen_cli

export CARGO_TARGET_DIR="$ROOT_DIR/benchmarks/generated/boltffi/target"
(
    cd "$DEMO_DIR"
    cargo run -p boltffi_cli --manifest-path "$ROOT_DIR/Cargo.toml" -- -v --overlay "$BENCH_OVERLAY" pack wasm --release --regenerate
    cargo run -p boltffi_cli --manifest-path "$ROOT_DIR/Cargo.toml" -- -v pack wasm --release --regenerate
)

rm -rf "$GENERATED_DIR/boltffi"
mkdir -p "$GENERATED_DIR/boltffi"
cp -R "$ROOT_DIR/benchmarks/generated/boltffi/dist/wasm/pkg/." "$GENERATED_DIR/boltffi/"

rm -rf "$GENERATED_DIR/boltffi-demo"
mkdir -p "$GENERATED_DIR/boltffi-demo"
cp -R "$DEMO_WASM_PKG_DIR/." "$GENERATED_DIR/boltffi-demo/"

export CARGO_TARGET_DIR="$ROOT_DIR/benchmarks/generated/wasm-bindgen/target"
cargo build --manifest-path "$DEMO_DIR/Cargo.toml" --release --target "$WASM_RUST_TARGET" --features wasm-bench

rm -rf "$ROOT_DIR/benchmarks/generated/wasm-bindgen/dist"
mkdir -p "$ROOT_DIR/benchmarks/generated/wasm-bindgen/dist"
wasm-bindgen \
    --target nodejs \
    --out-dir "$ROOT_DIR/benchmarks/generated/wasm-bindgen/dist" \
    "$ROOT_DIR/benchmarks/generated/wasm-bindgen/target/$WASM_RUST_TARGET/release/demo.wasm"

rm -rf "$GENERATED_DIR/wasmbindgen"
mkdir -p "$GENERATED_DIR/wasmbindgen"
cp -R "$ROOT_DIR/benchmarks/generated/wasm-bindgen/dist/." "$GENERATED_DIR/wasmbindgen/"
printf '{\n  "type": "commonjs"\n}\n' > "$GENERATED_DIR/wasmbindgen/package.json"

mkdir -p "$SCRIPT_DIR/node_modules/env"
WASM_BINDGEN_WASM="$ROOT_DIR/benchmarks/generated/wasm-bindgen/dist/demo_bg.wasm" \
ENV_STUB_OUT="$SCRIPT_DIR/node_modules/env/index.js" \
node <<'JS'
const fs = require('node:fs');

const wasmPath = process.env.WASM_BINDGEN_WASM;
const outputPath = process.env.ENV_STUB_OUT;
const wasmBytes = fs.readFileSync(wasmPath);
const moduleImports = WebAssembly.Module.imports(new WebAssembly.Module(wasmBytes));
const envImportNames = [...new Set(
  moduleImports
    .filter((item) => item.module === 'env')
    .map((item) => item.name)
)].sort();

const stubLines = [
  "'use strict';",
  "",
  "// Auto-generated stub module for unused demo callback imports in wasm-bindgen benchmarks.",
];

for (const importName of envImportNames) {
  stubLines.push(
    `exports.${importName} = (...args) => {`,
    `  throw new Error('unexpected env import call: ${importName}');`,
    "};",
    "",
  );
}

fs.writeFileSync(outputPath, `${stubLines.join('\n')}\n`);
JS

BENCH_OUTPUT_JSON="$RESULTS_DIR/results.json" BENCH_FILTER="$BENCH_FILTER" node "$SCRIPT_DIR/bench.mjs"

python3 "$ROOT_DIR/benchmarks/scripts/benchmarkjs_to_run.py" \
    --results "$RESULTS_DIR/results.json" \
    --output "$RESULTS_DIR/benchmark_run.json" \
    --profile release

if [[ "$PUBLISH" == true ]]; then
    "$ROOT_DIR/benchmarks/scripts/publish-benchmark-runs.sh" "$RESULTS_DIR/benchmark_run.json"
fi
