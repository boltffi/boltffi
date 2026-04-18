#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../../.."
DEMO_MANIFEST="$ROOT_DIR/examples/demo/Cargo.toml"
DEMO_LOCKFILE="$ROOT_DIR/examples/demo/Cargo.lock"

DIST_DIR="$SCRIPT_DIR/dist/csharp"
PACKAGE="demo"
BENCH_LIBRARY_BASENAME="bench_uniffi"
UNIFFI_BINDGEN_CS_TAG="v0.10.0+v0.29.4"
# The published uniffi-bindgen-cs release we can actually run today only matches
# UniFFI 0.29 metadata. The demo stays on the newer UniFFI version for every
# normal build, but the C# benchmark generation path temporarily rewrites the
# demo manifest and lockfile to 0.29.4, builds into an isolated target dir, and
# then restores the original files on exit so the real source of truth never
# stays downgraded in the repo.
UNIFFI_CSHARP_COMPAT_VERSION="0.29.4"
COMPAT_TARGET_DIR="$SCRIPT_DIR/target/csharp-compat"

backup_demo_uniffi_state() {
    RESTORE_DIR="$(mktemp -d /tmp/uniffi-csharp-restore.XXXXXX)"
    cp "$DEMO_MANIFEST" "$RESTORE_DIR/Cargo.toml"
    cp "$DEMO_LOCKFILE" "$RESTORE_DIR/Cargo.lock"
}

restore_demo_uniffi_state() {
    if [[ -n "${RESTORE_DIR:-}" && -d "${RESTORE_DIR:-}" ]]; then
        cp "$RESTORE_DIR/Cargo.toml" "$DEMO_MANIFEST"
        cp "$RESTORE_DIR/Cargo.lock" "$DEMO_LOCKFILE"
        rm -rf "$RESTORE_DIR"
    fi
}

set_demo_uniffi_compat_version() {
    UNIFFI_CSHARP_COMPAT_VERSION="$UNIFFI_CSHARP_COMPAT_VERSION" \
        perl -0pi -e 's/uniffi = \{ version = "(?:=?[^"]+)", optional = true \}/"uniffi = { version = \"=" . $ENV{UNIFFI_CSHARP_COMPAT_VERSION} . "\", optional = true }"/ge' \
        "$DEMO_MANIFEST"

    cargo update \
        --manifest-path "$DEMO_MANIFEST" \
        -p uniffi \
        --precise "$UNIFFI_CSHARP_COMPAT_VERSION"
}

resolve_bindgen_cs() {
    if [[ -n "${UNIFFI_BINDGEN_CS:-}" && -x "${UNIFFI_BINDGEN_CS}" ]]; then
        printf '%s\n' "${UNIFFI_BINDGEN_CS}"
        return 0
    fi

    if command -v uniffi-bindgen-cs >/dev/null 2>&1; then
        command -v uniffi-bindgen-cs
        return 0
    fi

    local install_root="$SCRIPT_DIR/target/uniffi-bindgen-cs"
    local install_binary="$install_root/bin/uniffi-bindgen-cs"

    if [[ -x "$install_binary" ]]; then
        printf '%s\n' "$install_binary"
        return 0
    fi

    cargo install \
        uniffi-bindgen-cs \
        --git https://github.com/NordSecurity/uniffi-bindgen-cs \
        --tag "$UNIFFI_BINDGEN_CS_TAG" \
        --root "$install_root"

    printf '%s\n' "$install_binary"
}

if [[ "$(uname)" == "Darwin" ]]; then
    LIBRARY_FILE="lib${PACKAGE}.dylib"
    BENCH_LIBRARY_FILE="lib${BENCH_LIBRARY_BASENAME}.dylib"
elif [[ "$(expr substr "$(uname -s)" 1 5)" == "Linux" ]]; then
    LIBRARY_FILE="lib${PACKAGE}.so"
    BENCH_LIBRARY_FILE="lib${BENCH_LIBRARY_BASENAME}.so"
else
    echo "Unknown platform: $(uname)" >&2
    exit 1
fi

cd "$SCRIPT_DIR"

backup_demo_uniffi_state
trap restore_demo_uniffi_state EXIT

set_demo_uniffi_compat_version

export CARGO_TARGET_DIR="$COMPAT_TARGET_DIR"
export BOLTFFI_DISABLE_EXPORTS=1

cargo build --manifest-path "$DEMO_MANIFEST" --lib --release --features uniffi

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

BINDGEN_CS_BIN="$(resolve_bindgen_cs)"

"$BINDGEN_CS_BIN" \
    --library \
    --no-format \
    --out-dir "$DIST_DIR" \
    "$COMPAT_TARGET_DIR/release/$LIBRARY_FILE"

cp "$COMPAT_TARGET_DIR/release/$LIBRARY_FILE" "$COMPAT_TARGET_DIR/release/$BENCH_LIBRARY_FILE"

perl -0pi -e 's/\[DllImport\("demo"/[DllImport("bench_uniffi"/g' \
    "$DIST_DIR/demo.cs"
