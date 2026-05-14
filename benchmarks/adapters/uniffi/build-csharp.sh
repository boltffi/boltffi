#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../../.."
DEMO_MANIFEST="$ROOT_DIR/examples/demo/Cargo.toml"

DIST_DIR="$SCRIPT_DIR/dist/csharp"
PACKAGE="demo"
BENCH_LIBRARY_BASENAME="bench_uniffi"

# demo pins uniffi = "0.31". Upstream NordSecurity/uniffi-bindgen-cs is still
# on 0.29.4 (see its v0.10.0+v0.29.4 tag) and silently drops record/enum
# declarations when fed a 0.31 cdylib. We pull from the open PR that upgrades
# it to 0.31.0 (NordSecurity/uniffi-bindgen-cs#163, jmbryan4's fork), pinned
# to the PR head commit for reproducibility. Revisit when the PR is merged
# and NordSecurity publishes a v0.31-compatible tag.
UNIFFI_BINDGEN_CS_REPO="https://github.com/jmbryan4/uniffi-bindgen-cs"
UNIFFI_BINDGEN_CS_REV="d788b3ab74f079608515bf24b0bc7fe735f2ea6b"

resolve_bindgen_cs() {
    if [[ -n "${UNIFFI_BINDGEN_CS:-}" && -x "${UNIFFI_BINDGEN_CS}" ]]; then
        printf '%s\n' "${UNIFFI_BINDGEN_CS}"
        return 0
    fi

    local install_root="$SCRIPT_DIR/target/uniffi-bindgen-cs"
    local install_binary="$install_root/bin/uniffi-bindgen-cs"
    if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* || "$(uname -s)" == CYGWIN* ]]; then
        install_binary="${install_binary}.exe"
    fi
    local install_stamp="$install_root/.rev"

    if [[ -x "$install_binary" && -f "$install_stamp" && "$(cat "$install_stamp")" == "$UNIFFI_BINDGEN_CS_REV" ]]; then
        printf '%s\n' "$install_binary"
        return 0
    fi

    cargo install \
        uniffi-bindgen-cs \
        --git "$UNIFFI_BINDGEN_CS_REPO" \
        --rev "$UNIFFI_BINDGEN_CS_REV" \
        --root "$install_root" \
        --locked

    printf '%s' "$UNIFFI_BINDGEN_CS_REV" > "$install_stamp"

    printf '%s\n' "$install_binary"
}

if [[ "$(uname)" == "Darwin" ]]; then
    LIBRARY_FILE="lib${PACKAGE}.dylib"
    BENCH_LIBRARY_FILE="lib${BENCH_LIBRARY_BASENAME}.dylib"
elif [[ "$(expr substr "$(uname -s)" 1 5)" == "Linux" ]]; then
    LIBRARY_FILE="lib${PACKAGE}.so"
    BENCH_LIBRARY_FILE="lib${BENCH_LIBRARY_BASENAME}.so"
elif [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* || "$(uname -s)" == CYGWIN* ]]; then
    LIBRARY_FILE="${PACKAGE}.dll"
    BENCH_LIBRARY_FILE="${BENCH_LIBRARY_BASENAME}.dll"
else
    echo "Unknown platform: $(uname)" >&2
    exit 1
fi

cd "$SCRIPT_DIR"

export CARGO_TARGET_DIR="$SCRIPT_DIR/target"
export BOLTFFI_DISABLE_EXPORTS=1

cargo build --manifest-path "$DEMO_MANIFEST" --lib --release --features uniffi

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

BINDGEN_CS_BIN="$(resolve_bindgen_cs)"

"$BINDGEN_CS_BIN" \
    --library \
    --no-format \
    --out-dir "$DIST_DIR" \
    "$SCRIPT_DIR/target/release/$LIBRARY_FILE"

cp "$SCRIPT_DIR/target/release/$LIBRARY_FILE" "$SCRIPT_DIR/target/release/$BENCH_LIBRARY_FILE"

# The uniffi adapter re-exports the demo cdylib as `libbench_uniffi` so it
# can coexist with the BoltFFI-generated `libdemo` in the same .NET output
# directory. Rewrite both P/Invoke attribute forms (the new
# `LibraryImport` used on .NET 8+ and the `DllImport` fallback) to point at
# the renamed library.
#
# uniffi-bindgen-cs also emits every top-level type as `internal` (either
# explicitly or by omitting the modifier), which breaks compilation when
# a public benchmark class references them (CS0053/CS9338). The benchmark
# assembly is the only consumer of this file, so promote them to `public`
# to keep accessibility uniform with the BoltFFI-generated side.
perl -pi -e '
    s/\[LibraryImport\("demo"/[LibraryImport("bench_uniffi"/g;
    s/\[DllImport\("demo"/[DllImport("bench_uniffi"/g;
    s/^internal /public /;
    s/^((?:static |abstract |sealed |partial )*(?:class|record|enum|interface|struct) )/public $1/;
' "$DIST_DIR/demo.cs"
