#!/usr/bin/env bash
set -euo pipefail

# Builds the demo gem from the sources `boltffi pack ruby` wrote to dist/,
# installs it into an empty gem home, and runs the Ruby demo tests against
# the installed gem. The install builds the Rust crate with cargo and
# compiles the C extension, so this proves the package, not only the
# generated source.

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dist_directory="$script_dir/dist"
requested_ruby=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --ruby)
            requested_ruby="${2:-}"
            shift 2
            ;;
        *)
            printf 'Unknown argument: %s\n' "$1" >&2
            printf 'Usage: %s [--ruby <interpreter>]\n' "$0" >&2
            exit 2
            ;;
    esac
done

ruby_command="${requested_ruby:-ruby}"
if ! command -v "$ruby_command" >/dev/null 2>&1; then
    printf 'Missing ruby interpreter: %s\n' "$ruby_command" >&2
    exit 1
fi

gemspec="$(find "$dist_directory" -maxdepth 1 -name '*.gemspec' | head -n 1)"
if [[ -z "$gemspec" ]]; then
    printf 'No gemspec in %s\n' "$dist_directory" >&2
    printf 'Run `boltffi pack ruby --experimental` in examples/demo first.\n' >&2
    exit 1
fi

work_directory="$(mktemp -d "${TMPDIR:-/tmp}/boltffi-ruby-demo.XXXXXX")"
trap 'rm -rf "$work_directory"' EXIT

# Only the Ruby install's own gems and the demo gem are visible, so gems in
# the user's gem directory cannot change the result. RubyGems removes
# GEM_PATH when it runs extconf.rb, and the default gem path starts with the
# user gem directory under HOME. So the install also gets an empty HOME.
export GEM_HOME="$work_directory/gems"
GEM_PATH="$GEM_HOME:$("$ruby_command" -e 'print Gem.default_dir')"
export GEM_PATH
mkdir -p "$work_directory/home"

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
# extconf.rb runs cargo with the empty HOME, outside the repository. So the
# install keeps the cargo and rustup homes of the user. The pack copied the
# nearest rust-toolchain.toml into the crate copy, so the install uses the
# toolchain of the pack build. The script sets no RUSTUP_TOOLCHAIN.

(cd "$dist_directory" && "$ruby_command" -S gem build "$(basename "$gemspec")" --output "$work_directory/demo.gem")
# A failed install hides the probe details in mkmf.log, so the script prints
# that file before it exits.
if ! HOME="$work_directory/home" "$ruby_command" -S gem install --local --no-document "$work_directory/demo.gem"; then
    find "$GEM_HOME/extensions" -name mkmf.log -exec cat {} + >&2 || true
    exit 1
fi

# The extension exports only its entry point. Ruby loads extensions with
# RTLD_GLOBAL, so an exported Rust symbol could bind to another extension.
# A linker that ignores the hide flag still installs a working gem, so the
# script reads the exports of the built file and fails on any extra symbol.
extension_file="$(find "$GEM_HOME" -path '*/lib/demo/demo.*' -type f | head -n 1)"
if [[ -z "$extension_file" ]]; then
    printf 'No installed demo extension under %s\n' "$GEM_HOME" >&2
    exit 1
fi
case "$(uname -s)" in
    Darwin)
        exports="$(nm -gU "$extension_file" | awk '{print $NF}' | sed 's/^_//')"
        ;;
    Linux)
        exports="$(readelf --dyn-syms -W "$extension_file" | awk 'NR > 3 && $7 != "UND" {sub(/@.*/, "", $8); print $8}' | sed '/^$/d')"
        ;;
    *)
        printf 'Unsupported platform for the export check: %s\n' "$(uname -s)" >&2
        exit 1
        ;;
esac
unexpected="$(printf '%s\n' "$exports" | grep -vx -e Init_demo -e ruby_abi_version -e '' || true)"
if [[ -n "$unexpected" ]]; then
    printf 'The demo extension exports extra symbols:\n%s\n' "$unexpected" >&2
    exit 1
fi
if ! printf '%s\n' "$exports" | grep -qx Init_demo; then
    printf 'The demo extension at %s does not export Init_demo. The export check read the wrong file or read no symbols.\n' "$extension_file" >&2
    exit 1
fi

"$ruby_command" "$script_dir/run_tests.rb"
