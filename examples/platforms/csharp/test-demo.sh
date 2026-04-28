#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../.." && pwd)"
demo_dir="$repo_root/examples/demo"
manifest_path="$repo_root/Cargo.toml"
test_project="$script_dir/DemoTest"

configuration="Debug"
target_framework="net10.0"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release)
      configuration="Release"
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      echo "Usage: $0 [--release]" >&2
      exit 2
      ;;
  esac
done

cargo_profile="debug"
cargo_flags=()
if [[ "$configuration" == "Release" ]]; then
  cargo_profile="release"
  cargo_flags=(--release)
fi

native_lib_dir="$demo_dir/target/$cargo_profile"
case "$(uname -s)" in
  Darwin)   native_lib_file="libdemo.dylib" ;;
  Linux)    native_lib_file="libdemo.so" ;;
  MINGW*|MSYS*|CYGWIN*) native_lib_file="demo.dll" ;;
  *)
    echo "Unsupported host for C# demo: $(uname -s)" >&2
    exit 1
    ;;
esac

echo "=== cargo build demo ($cargo_profile) ==="
(cd "$demo_dir" && cargo build "${cargo_flags[@]}")

if [[ ! -f "$native_lib_dir/$native_lib_file" ]]; then
  echo "Expected native library not found: $native_lib_dir/$native_lib_file" >&2
  exit 1
fi

echo "=== boltffi generate csharp ==="
(cd "$demo_dir" && cargo run --quiet --manifest-path "$manifest_path" -p boltffi_cli -- generate csharp --experimental)

echo "=== dotnet build DemoTest ==="
dotnet build "$test_project" --configuration "$configuration" --nologo

bin_dir="$test_project/bin/$configuration/$target_framework"
cp "$native_lib_dir/$native_lib_file" "$bin_dir/$native_lib_file"

echo "=== dotnet run DemoTest ==="
dotnet "$bin_dir/DemoTest.dll"
