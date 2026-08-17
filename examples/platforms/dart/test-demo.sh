#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
package_dir="$script_dir/pkgs/demo"

if ! command -v dart >/dev/null 2>&1; then
    printf 'Missing dart executable\n' >&2
    exit 127
fi

if [[ ! -d "$package_dir" ]]; then
    printf 'Missing generated Dart package: %s\n' "$package_dir" >&2
    printf 'Pack the Dart demo first (`boltffi pack dart`).\n' >&2
    exit 1
fi

(
    cd "$script_dir"
    dart pub get
    dart test
)
