#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../.." && pwd)"
demo_dir="$repo_root/examples/demo"
build_dir="$script_dir/build"

(
    cd "$demo_dir"
    cargo run --quiet --manifest-path "$repo_root/Cargo.toml" -p boltffi_cli -- \
        --cargo-arg=--features --cargo-arg=c-demo pack c --experimental
    cargo run --quiet --manifest-path "$repo_root/Cargo.toml" -p boltffi_cli -- \
        --cargo-arg=--features --cargo-arg=c-demo pack c --experimental --no-build
)

package_dir="$build_dir/relocated package"
mkdir -p "$build_dir"
rm -f "$build_dir/CMakeCache.txt"
rm -rf "$package_dir"
mv "$script_dir/generated" "$package_dir"
trap 'cp -R "$package_dir" "$script_dir/generated"' EXIT

export PKG_CONFIG_PATH="$package_dir/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"

cmake -S "$script_dir" -B "$build_dir" -DCMAKE_BUILD_TYPE=Debug -DCMAKE_PREFIX_PATH="$package_dir"
cmake --build "$build_dir" --config Debug
ctest --test-dir "$build_dir" --build-config Debug --output-on-failure

if command -v meson >/dev/null 2>&1 && [[ "$(uname -s)" != MINGW* && "$(uname -s)" != MSYS* ]]; then
    meson setup --wipe "$build_dir/meson" "$script_dir"
    meson compile -C "$build_dir/meson"
    meson test -C "$build_dir/meson" --print-errorlogs
fi
