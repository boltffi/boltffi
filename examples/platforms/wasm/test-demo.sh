#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../.." && pwd)"
runtime_directory="$repo_root/runtime/typescript"

npm_bin() {
    if command -v npm >/dev/null 2>&1; then
        printf 'npm\n'
        return
    fi

    if command -v npm.cmd >/dev/null 2>&1; then
        printf 'npm.cmd\n'
        return
    fi

    printf 'Missing npm executable\n' >&2
    exit 127
}

NPM_BIN="$(npm_bin)"

install_node_dependencies() {
    local package_directory="$1"

    (
        cd "$package_directory"

        if [[ -f package-lock.json || -f npm-shrinkwrap.json ]]; then
            "$NPM_BIN" ci
        else
            "$NPM_BIN" install
        fi
    )
}

run_package_script() {
    local package_directory="$1"
    local script_name="$2"

    (
        cd "$package_directory"
        "$NPM_BIN" run "$script_name"
    )
}

if [[ ! -f "$runtime_directory/package.json" ]]; then
    printf 'Missing TypeScript runtime package: %s\n' "$runtime_directory/package.json" >&2
    exit 1
fi

if [[ ! -f "$script_dir/package.json" ]]; then
    printf 'Missing WASM demo package: %s\n' "$script_dir/package.json" >&2
    exit 1
fi

install_node_dependencies "$runtime_directory"
run_package_script "$runtime_directory" build
install_node_dependencies "$script_dir"
if [[ "${1:-}" == "--prepare" ]]; then
    exit 0
fi
run_package_script "$script_dir" test
run_package_script "$script_dir" test:package
