#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
demo_dir="$repo_root/examples/demo"
apple_dir="$repo_root/examples/platforms/apple"
kotlin_dir="$repo_root/examples/platforms/kotlin"
java_dir="$repo_root/examples/platforms/java"
csharp_dir="$repo_root/examples/platforms/csharp"
wasm_dir="$repo_root/examples/platforms/wasm"
python_dir="$repo_root/examples/platforms/python"
workspace_manifest="$repo_root/Cargo.toml"

selected_platforms=()
python_interpreter=""

run_step() {
    local title="$1"
    shift
    printf '\n=== %s ===\n' "$title"
    "$@"
}

run_boltffi() {
    (
        cd "$demo_dir"
        cargo run -q --manifest-path "$workspace_manifest" -p boltffi_cli -- "$@"
    )
}

host_default_platforms() {
    case "$(uname -s)" in
        Darwin)
            printf '%s\n' apple kotlin java csharp wasm python
            ;;
        Linux|MINGW*|MSYS*|CYGWIN*)
            printf '%s\n' java csharp wasm python
            ;;
        *)
            printf 'unsupported host for demo verification: %s\n' "$(uname -s)" >&2
            exit 1
            ;;
    esac
}

append_host_default_platforms() {
    while IFS= read -r host_platform; do
        selected_platforms+=("$host_platform")
    done < <(host_default_platforms)
}

selected_platform_needs_check() {
    local expected_platform="$1"

    for selected_platform in "${selected_platforms[@]}"; do
        if [[ "$selected_platform" == "$expected_platform" ]]; then
            return 0
        fi
    done

    return 1
}

prepare_selected_platforms() {
    local check_arguments=(check --fix)

    if selected_platform_needs_check apple; then
        check_arguments+=(--apple)
    fi

    if selected_platform_needs_check wasm; then
        check_arguments+=(--wasm)
    fi

    if [[ ${#check_arguments[@]} -gt 2 ]]; then
        run_step "prepare toolchains" run_boltffi "${check_arguments[@]}"
    fi
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --platform)
            selected_platforms+=("${2:-}")
            shift 2
            ;;
        --python)
            python_interpreter="${2:-}"
            shift 2
            ;;
        --host-defaults)
            shift
            ;;
        *)
            printf 'Unknown argument: %s\n' "$1" >&2
            printf 'Usage: %s [--platform <apple|kotlin|java|csharp|wasm|python>] [--python <interpreter>] [--host-defaults]\n' "$0" >&2
            exit 2
            ;;
    esac
done

if [[ ${#selected_platforms[@]} -eq 0 ]]; then
    append_host_default_platforms
fi

prepare_selected_platforms

for selected_platform in "${selected_platforms[@]}"; do
    case "$selected_platform" in
        apple)
            run_step "pack apple" run_boltffi pack apple
            run_step "swift test" swift test --package-path "$apple_dir"
            ;;
        kotlin)
            run_step "kotlin test" gradle -p "$kotlin_dir" test
            ;;
        java)
            run_step "pack java" run_boltffi pack java
            run_step "java demo" "$java_dir/test-demo.sh" --auto
            ;;
        csharp)
            run_step "csharp demo" "$csharp_dir/test-demo.sh"
            ;;
        wasm)
            run_step "pack wasm" run_boltffi pack wasm
            run_step "wasm demo" "$wasm_dir/test-demo.sh"
            ;;
        python)
            if [[ -n "$python_interpreter" ]]; then
                run_step "pack python" run_boltffi pack python --release --experimental --python "$python_interpreter"
                run_step "python demo" "$python_dir/test-demo.sh" --python "$python_interpreter"
            else
                run_step "pack python" run_boltffi pack python --release --experimental
                run_step "python demo" "$python_dir/test-demo.sh"
            fi
            ;;
        *)
            printf 'Unsupported demo platform: %s\n' "$selected_platform" >&2
            exit 2
            ;;
    esac
done
