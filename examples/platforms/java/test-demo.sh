#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../.." && pwd)"
demo_dir="$repo_root/examples/demo"
demo_config="$demo_dir/boltffi.toml"
demo_test_source="$script_dir/DemoTest.java"
native_lib_dir="$script_dir/src/main/java"
manifest_path="$repo_root/Cargo.toml"

requested_mode="auto"
requested_python=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --java)
      requested_mode="${2:-}"
      shift 2
      ;;
    --all)
      requested_mode="all"
      shift
      ;;
    --auto)
      requested_mode="auto"
      shift
      ;;
    --python)
      requested_python="${2:-}"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1" >&2
      echo "Usage: $0 [--auto|--all|--java 8|16] [--python <interpreter>]" >&2
      exit 2
      ;;
  esac
done

if [[ ! -f "$demo_config" ]]; then
  echo "Missing demo config: $demo_config" >&2
  exit 1
fi

if [[ ! -f "$demo_test_source" ]]; then
  echo "Missing demo test source: $demo_test_source" >&2
  exit 1
fi

if [[ ! -f "$native_lib_dir/libdemo_jni.dylib" \
   && ! -f "$native_lib_dir/libdemo_jni.so" \
   && ! -f "$native_lib_dir/demo_jni.dll" \
   && ! -f "$native_lib_dir/native/darwin-arm64/libdemo_jni.dylib" \
   && ! -f "$native_lib_dir/native/darwin-x86_64/libdemo_jni.dylib" \
   && ! -f "$native_lib_dir/native/linux-x86_64/libdemo_jni.so" \
   && ! -f "$native_lib_dir/native/linux-aarch64/libdemo_jni.so" \
   && ! -f "$native_lib_dir/native/windows-x86_64/demo_jni.dll" ]]; then
  echo "Missing JNI demo library in $native_lib_dir" >&2
  echo "Build/pack the Java demo JNI artifacts first." >&2
  exit 1
fi

javac_version_text="$(javac -version 2>&1)"
javac_version_number="$(printf '%s\n' "$javac_version_text" | awk '{print $2}' | head -n1)"
if [[ "$javac_version_number" == 1.* ]]; then
  javac_major="$(printf '%s\n' "$javac_version_number" | cut -d. -f2)"
else
  javac_major="$(printf '%s\n' "$javac_version_number" | cut -d. -f1)"
fi

if [[ -z "$javac_major" ]]; then
  echo "Unable to detect javac version" >&2
  exit 1
fi

config_backup="$(mktemp)"
cp "$demo_config" "$config_backup"

cleanup() {
  cp "$config_backup" "$demo_config" >/dev/null 2>&1 || true
  rm -f "$config_backup"
}

trap cleanup EXIT

resolve_python() {
  if [[ -n "$requested_python" ]]; then
    printf '%s\n' "$requested_python"
    return
  fi

  if command -v python3 >/dev/null 2>&1; then
    printf 'python3\n'
    return
  fi

  if command -v python >/dev/null 2>&1; then
    printf 'python\n'
    return
  fi

  echo "Missing python interpreter" >&2
  exit 1
}

python_command="$(resolve_python)"

set_min_version() {
  local version_mode="$1"
  "$python_command" - "$demo_config" "$version_mode" <<'PY'
import re
import sys
from pathlib import Path

config_path = Path(sys.argv[1])
version_mode = sys.argv[2]
text = config_path.read_text()
text = re.sub(r'(?m)^min_version\s*=\s*\d+\s*\n?', '', text)
if version_mode != 'none':
    marker = '[targets.java]\n'
    if marker not in text:
        raise SystemExit('targets.java section missing')
    text = text.replace(marker, marker + f'min_version = {version_mode}\n', 1)
config_path.write_text(text)
PY
}

run_case() {
  local label="$1"
  local config_version="$2"
  local javac_release="$3"
  local expected_point_declaration="$4"
  local output_dir
  local build_dir
  output_dir="$(mktemp -d "/tmp/boltffi-java-${label}-XXXXXX")"
  build_dir="$output_dir/build"

  set_min_version "$config_version"

  (cd "$demo_dir" && cargo run -q --manifest-path "$manifest_path" -p boltffi_cli -- --cargo-arg=--features --cargo-arg=async-initializers generate java --output "$output_dir")

  local point_file="$output_dir/com/boltffi/demo/Point.java"
  if ! grep -q "$expected_point_declaration" "$point_file"; then
    echo "[$label] unexpected Point.java declaration" >&2
    grep -nE 'public (final class|record) Point' "$point_file" || true
    return 1
  fi

  mkdir -p "$build_dir"

  find "$output_dir/com/boltffi/demo" -maxdepth 1 -name '*.java' ! -name 'DemoTest.java' -print0 \
    | xargs -0 javac -encoding UTF-8 --release "$javac_release" -d "$build_dir"

  javac -encoding UTF-8 --release "$javac_release" -cp "$build_dir" -d "$build_dir" "$demo_test_source"

  java -ea -cp "$build_dir" -Djava.library.path="$native_lib_dir" com.boltffi.demo.DemoTest

  echo "[ok] $label (javac --release $javac_release, runtime on JDK $javac_major)"
}

case "$requested_mode" in
  8|java8)
    run_case "java8" "none" "8" "public final class Point"
    ;;
  16|java16)
    if (( javac_major < 16 )); then
      echo "javac $javac_major does not support --release 16" >&2
      exit 1
    fi
    run_case "java16" "16" "16" "public record Point"
    ;;
  all)
    run_case "java8" "none" "8" "public final class Point"
    if (( javac_major < 16 )); then
      echo "javac $javac_major does not support Java 16 record compile; skipping java16 case" >&2
      exit 0
    fi
    run_case "java16" "16" "16" "public record Point"
    ;;
  auto)
    run_case "java8" "none" "8" "public final class Point"
    if (( javac_major >= 16 )); then
      run_case "java16" "16" "16" "public record Point"
    else
      echo "[skip] java16 (javac $javac_major)"
    fi
    ;;
  *)
    echo "Unsupported mode: $requested_mode" >&2
    echo "Usage: $0 [--auto|--all|--java 8|16]" >&2
    exit 2
    ;;
esac
