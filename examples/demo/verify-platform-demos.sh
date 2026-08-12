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
dart_dir="$repo_root/examples/platforms/dart"
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

# pack dart defaults to every Dart native triple. CI/host verification only
# needs the current machine, otherwise cargo tries Android/iOS/etc.
host_dart_native_target() {
    local os
    local arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Darwin)
            case "$arch" in
                arm64) printf '%s\n' "macos:arm64" ;;
                x86_64) printf '%s\n' "macos:x86_64" ;;
                *)
                    printf 'unsupported Darwin architecture for Dart demo: %s\n' "$arch" >&2
                    return 1
                    ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64) printf '%s\n' "linux:x86_64" ;;
                aarch64|arm64) printf '%s\n' "linux:arm64" ;;
                *)
                    printf 'unsupported Linux architecture for Dart demo: %s\n' "$arch" >&2
                    return 1
                    ;;
            esac
            ;;
        MINGW*|MSYS*|CYGWIN*)
            case "$arch" in
                x86_64|AMD64) printf '%s\n' "windows:x86_64" ;;
                aarch64|arm64|ARM64) printf '%s\n' "windows:arm64" ;;
                *)
                    printf 'unsupported Windows architecture for Dart demo: %s\n' "$arch" >&2
                    return 1
                    ;;
            esac
            ;;
        *)
            printf 'unsupported host for Dart demo: %s\n' "$os" >&2
            return 1
            ;;
    esac
}

pack_host_dart() {
    local overlay
    overlay="$(mktemp "${TMPDIR:-/tmp}/boltffi-dart-host.XXXXXX.toml")"
    printf '[targets.dart]\nnative_targets = ["%s"]\n' "$(host_dart_native_target)" >"$overlay"
    run_boltffi --overlay "$overlay" pack dart --release
    rm -f "$overlay"
}

host_default_platforms() {
    case "$(uname -s)" in
        Darwin)
            printf '%s\n' apple kotlin java csharp wasm python dart
            ;;
        Linux|MINGW*|MSYS*|CYGWIN*)
            printf '%s\n' java csharp wasm python dart
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
            printf 'Usage: %s [--platform <apple|kotlin|java|csharp|wasm|python|dart>] [--python <interpreter>] [--host-defaults]\n' "$0" >&2
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
            run_step "pack apple" run_boltffi pack apple --release
            run_step "swift test" swift test --package-path "$apple_dir"
            run_step "xcodebuild xcframework modulemap smoke" bash "$apple_dir/verify-xcframework-modulemap-collision.sh"
            run_step "xcodebuild static library symbolication" bash "$apple_dir/verify-static-library-symbolication.sh"
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
                run_step "pack python" run_boltffi pack python --release --python "$python_interpreter"
                run_step "python demo" "$python_dir/test-demo.sh" --python "$python_interpreter"
            else
                run_step "pack python" run_boltffi pack python --release
                run_step "python demo" "$python_dir/test-demo.sh"
            fi
            ;;
        dart)
            run_step "pack dart" pack_host_dart
            run_step "dart demo" "$dart_dir/test-demo.sh"
            ;;
        *)
            printf 'Unsupported demo platform: %s\n' "$selected_platform" >&2
            exit 2
            ;;
    esac
done
