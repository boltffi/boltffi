#!/usr/bin/env bash
set -euo pipefail

# Thin entry point for the Ruby runtime e2e suite. Resolves an interpreter and
# the generated gem sources, then hands off to run_tests.rb, which compiles the
# C extension (statically linking the Rust staticlib), loads it, and exercises
# the demo API at runtime. The real work lives in run_tests.rb to keep this
# free of any heredoc-evaluated Ruby.

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../.." && pwd)"
gem_src="$script_dir/dist"
crate_manifest="$repo_root/examples/demo/Cargo.toml"
requested_ruby=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --ruby)
            requested_ruby="${2:-}"
            shift 2
            ;;
        --gem-src)
            gem_src="${2:-}"
            shift 2
            ;;
        *)
            printf 'Unknown argument: %s\n' "$1" >&2
            printf 'Usage: %s [--ruby <interpreter>] [--gem-src <dir>]\n' "$0" >&2
            exit 2
            ;;
    esac
done

resolve_ruby() {
    if [[ -n "$requested_ruby" ]]; then
        printf '%s\n' "$requested_ruby"
        return
    fi

    if command -v ruby >/dev/null 2>&1; then
        printf 'ruby\n'
        return
    fi

    printf 'Missing ruby interpreter\n' >&2
    exit 1
}

ruby_command="$(resolve_ruby)"

if [[ ! -f "$gem_src/lib/demo.rb" ]]; then
    printf 'Missing generated Ruby gem sources: %s/lib/demo.rb\n' "$gem_src" >&2
    printf 'Generate the Ruby demo package first (boltffi generate ruby).\n' >&2
    exit 1
fi

if [[ ! -f "$crate_manifest" ]]; then
    printf 'Missing crate manifest: %s\n' "$crate_manifest" >&2
    exit 1
fi

# Build + run with the resolved interpreter. Passing --ruby keeps the build and
# load on the same interpreter so the compiled extension's ABI matches.
exec "$ruby_command" "$script_dir/run_tests.rb" \
    --gem-src "$gem_src" \
    --crate-manifest "$crate_manifest" \
    --ruby "$ruby_command"
