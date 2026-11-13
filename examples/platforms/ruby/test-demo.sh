#!/usr/bin/env bash
set -euo pipefail

# Builds the demo gem from the sources `boltffi pack ruby` wrote to dist/,
# installs it into an empty gem home, and runs the Ruby demo tests against
# the installed gem. The install compiles the C extension, so this proves the
# package, not only the generated source.

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

(cd "$dist_directory" && "$ruby_command" -S gem build "$(basename "$gemspec")" --output "$work_directory/demo.gem")
HOME="$work_directory/home" "$ruby_command" -S gem install --local --no-document "$work_directory/demo.gem"

"$ruby_command" "$script_dir/run_tests.rb"
