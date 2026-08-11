#!/usr/bin/env bash
# Re-syncs boltffi_cli/vendor/runtime-typescript from runtime/typescript.
#
# build.rs builds @boltffi/runtime from this vendored copy so a packaged
# boltffi_cli crate (cargo install, crates.io) always has the source on
# hand -- Cargo can never package a sibling ../runtime/typescript
# directory. There's no CI check keeping the copy in sync, so run this
# (and commit the result) whenever runtime/typescript/src changes.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$CLI_DIR/.." && pwd)"
SOURCE_DIR="$ROOT_DIR/runtime/typescript"
VENDOR_DIR="$CLI_DIR/vendor/runtime-typescript"

if [[ ! -d "$SOURCE_DIR/src" ]]; then
    echo "runtime/typescript/src not found at $SOURCE_DIR -- run this from a full boltffi monorepo checkout" >&2
    exit 1
fi

rm -rf "$VENDOR_DIR"
mkdir -p "$VENDOR_DIR/src"
cp "$SOURCE_DIR"/src/*.ts "$VENDOR_DIR/src/"
cp "$SOURCE_DIR/package.json" "$VENDOR_DIR/package.json"
cp "$SOURCE_DIR/package-lock.json" "$VENDOR_DIR/package-lock.json"
cp "$SOURCE_DIR/tsconfig.json" "$VENDOR_DIR/tsconfig.json"

echo "Verifying the vendored copy builds on its own..."
(cd "$VENDOR_DIR" && npm install && npm run build)
rm -rf "$VENDOR_DIR/dist" "$VENDOR_DIR/node_modules"

echo
echo "Synced $VENDOR_DIR from $SOURCE_DIR."
echo "Review the diff and commit boltffi_cli/vendor/runtime-typescript."
