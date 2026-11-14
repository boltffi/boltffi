#!/usr/bin/env bash
set -euo pipefail

# Packs the fixture gem, then installs it twice into empty gem homes.
#
# The fixture crate has only crates.io dependencies, so pack ruby needs no
# --allow-external-path-dependencies. The gem must build on a machine
# without the fixture folder. One install sets no CARGO_TARGET_DIR. The
# other sets CARGO_TARGET_DIR to an absolute path.

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fixture_dir="$script_dir/fixture"
repo_root="$(cd "$script_dir/../../.." && pwd)"
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

rm -rf "$fixture_dir/dist"
# The repository ignores Cargo.lock files. pack ruby needs one, and the
# install builds with --locked, so the script writes the lockfile first.
cargo generate-lockfile --manifest-path "$fixture_dir/Cargo.toml"
(
    cd "$fixture_dir"
    cargo run -q --manifest-path "$repo_root/Cargo.toml" -p boltffi_cli -- pack ruby --experimental
)

work_directory="$(mktemp -d "${TMPDIR:-/tmp}/boltffi-ruby-fixture.XXXXXX")"
trap 'rm -rf "$work_directory"' EXIT

gemspec="$(find "$fixture_dir/dist" -maxdepth 1 -name '*.gemspec' | head -n 1)"
mkdir -p "$work_directory/home"
(
    cd "$fixture_dir/dist"
    HOME="$work_directory/home" GEM_HOME="$work_directory/build-gems" GEM_PATH="$work_directory/build-gems:$("$ruby_command" -e 'print Gem.default_dir')" \
        "$ruby_command" -S gem build "$(basename "$gemspec")" --output "$work_directory/fixture.gem"
)
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"

install_and_check() {
    local name="$1"
    local install_directory="$work_directory/$name"
    mkdir -p "$install_directory/home"
    GEM_HOME="$install_directory/gems" \
    GEM_PATH="$install_directory/gems:$("$ruby_command" -e 'print Gem.default_dir')" \
    HOME="$install_directory/home" \
        "$ruby_command" -S gem install --local --no-document "$work_directory/fixture.gem"
    GEM_HOME="$install_directory/gems" \
    GEM_PATH="$install_directory/gems:$("$ruby_command" -e 'print Gem.default_dir')" \
        "$ruby_command" -e 'require "boltffi_ruby_fixture"; raise "BoltffiRubyFixture is not defined" unless defined?(BoltffiRubyFixture)'
}

install_and_check plain
CARGO_TARGET_DIR="$work_directory/cargo-target" install_and_check with-target-dir
