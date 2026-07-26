#!/usr/bin/env bash
set -euo pipefail

apple_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/boltffi-symbolication.XXXXXX")"
package="$scratch/Symbolication"
derived_data="$scratch/DerivedData"
archive="$scratch/Symbolication.xcarchive"
executable="$archive/Products/usr/local/bin/Symbolication"
dsym="$archive/dSYMs/Symbolication.dSYM"
dsym_binary="$dsym/Contents/Resources/DWARF/Symbolication"
symbol="boltffi_function_demo_primitives_scalars_add_i32"
build_log="$scratch/xcodebuild.log"

trap 'rm -rf "$scratch"' EXIT

cp -R "$apple_dir/fixtures/symbolication" "$package"
ln -s "$apple_dir/ffi" "$package/ffi"

if ! (
    cd "$package"
    xcodebuild \
        -scheme Symbolication \
        -configuration Release \
        -destination generic/platform=macOS \
        -archivePath "$archive" \
        -derivedDataPath "$derived_data" \
        DEBUG_INFORMATION_FORMAT=dwarf-with-dsym \
        archive \
        -quiet \
        >"$build_log" 2>&1
); then
    cat "$build_log"
    exit 1
fi

"$executable"

executable_uuid="$(xcrun dwarfdump --uuid "$executable" | awk 'NR == 1 { print $2 }')"
dsym_uuid="$(xcrun dwarfdump --uuid "$dsym" | awk 'NR == 1 { print $2 }')"

[[ -n "$executable_uuid" ]] || { printf 'missing executable UUID\n' >&2; exit 1; }
[[ "$executable_uuid" == "$dsym_uuid" ]] || { printf 'executable and dSYM UUIDs differ\n' >&2; exit 1; }

symbol_address="$(xcrun nm "$dsym_binary" | awk -v expected="_$symbol" '$3 == expected && address == "" { address = "0x" $1 } END { print address }')"
[[ -n "$symbol_address" ]] || { printf 'Rust export missing from dSYM\n' >&2; exit 1; }

symbolicated="$(xcrun atos -arch "$(uname -m)" -o "$dsym_binary" "$symbol_address")"
printf '%s\n' "$symbolicated" | grep -Eq '\.rs:[0-9]+\)' || {
    printf 'Rust export did not resolve to a source line: %s\n' "$symbolicated" >&2
    exit 1
}
