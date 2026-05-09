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
pack_flags=()
if [[ "$configuration" == "Release" ]]; then
  cargo_profile="release"
  pack_flags=(--release)
fi

package_dir="$script_dir/dist/packages"
packages_cache="$script_dir/dist/.nuget/packages"

echo "=== boltffi pack csharp ($cargo_profile) ==="
(cd "$demo_dir" && cargo run --quiet --manifest-path "$manifest_path" -p boltffi_cli -- pack csharp "${pack_flags[@]}")

echo "=== dotnet build DemoTest ==="
rm -rf "$packages_cache"
dotnet build "$test_project" \
  --configuration "$configuration" \
  --source "$package_dir" \
  --property:RestorePackagesPath="$packages_cache" \
  --property:RestoreNoCache=true \
  --nologo

bin_dir="$test_project/bin/$configuration/$target_framework"

echo "=== dotnet run DemoTest ==="
dotnet "$bin_dir/DemoTest.dll"
