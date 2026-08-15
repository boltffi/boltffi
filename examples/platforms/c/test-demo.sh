#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
demo_dir="$(cd "$script_dir/../../demo" && pwd)"
generated_dir="$script_dir/generated"
cc_compiler="${CC:-cc}"

# 1. Generate C bindings + build the demo cdylib and stage the package
#    (include/<library>.h + lib/<library>.so under [targets.c].output).
( cd "$demo_dir" && boltffi pack c --experimental )

header_path="$generated_dir/include/demo.h"
lib_path=""
for candidate in \
    "$generated_dir/lib/libdemo.so" \
    "$generated_dir/libdemo.so"; do
    if [[ -f "$candidate" ]]; then
        lib_path="$candidate"
        break
    fi
done

if [[ ! -f "$header_path" ]]; then
    printf 'Missing generated header: %s\n' "$header_path" >&2
    exit 1
fi
if [[ -z "$lib_path" ]]; then
    printf 'Missing generated demo library under %s\n' "$generated_dir/lib" >&2
    exit 1
fi

lib_dir="$(dirname "$lib_path")"

# 2. Build the C smoke/test program and link against the demo library.
"$cc_compiler" -std=c11 -I "$generated_dir/include" "$script_dir/tests/demo.c" \
    -L "$lib_dir" -Wl,-rpath,"$(cd "$lib_dir" && pwd)" -l:libdemo.so -o "$script_dir/demo_test"

# 3. Run the linked smoke test.
"$script_dir/demo_test"

printf 'C platform tests passed.\n'
