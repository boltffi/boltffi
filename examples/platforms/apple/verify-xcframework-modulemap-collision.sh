#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/boltffi-xcframework-modulemap.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

case "$(uname -m)" in
    arm64) architecture=arm64 ;;
    x86_64) architecture=x86_64 ;;
    *) printf 'Unsupported Apple host architecture\n' >&2; exit 1 ;;
esac
rust_toolchain="$(cd "$repo_root" && rustup show active-toolchain | awk '{print $1}')"

# Use real, separately built BoltFFI libraries and generated Swift packages.
for library in first second; do
    if [[ "$library" == first ]]; then
        module=First
        value=1
        layout=ffi-only
    else
        module=Second
        value=2
        layout=bundled
    fi
    fixture="$tmp_dir/$library"
    mkdir -p "$fixture/src"
    cat > "$fixture/Cargo.toml" <<TOML
[package]
name = "$library"
version = "0.1.0"
edition = "2024"
[lib]
crate-type = ["staticlib"]
[dependencies]
boltffi = { path = "$repo_root/boltffi" }
TOML
    cat > "$fixture/src/lib.rs" <<RUST
#[boltffi::export]
pub fn ${library}_value() -> i32 { $value }
RUST
    cat > "$fixture/boltffi.toml" <<TOML
[package]
name = "$library"
[targets.apple]
include_macos = true
ios_architectures = []
simulator_architectures = ["$architecture"]
macos_architectures = ["$architecture"]
output = "${module}Package"
[targets.apple.swift]
module_name = "$module"
[targets.apple.spm]
layout = "$layout"
TOML
    (
        cd "$fixture"
        # Fixtures live outside the repo, so pin Cargo to the same toolchain
        # that prepare_selected_platforms used to install Apple targets for.
        RUSTUP_TOOLCHAIN="$rust_toolchain" cargo run --quiet --manifest-path "$repo_root/Cargo.toml" -p boltffi_cli -- -v pack apple
    )
    cp "$fixture/${module}Package/Package.swift" "$fixture/Package.swift.saved"
done

consumer="$tmp_dir/Consumer"
mkdir -p "$consumer/Sources" "$consumer/Tests"
cat > "$consumer/Package.swift" <<'SWIFT'
// swift-tools-version:5.9
import PackageDescription
let package = Package(
    name: "Consumer",
    platforms: [.iOS(.v16), .macOS(.v13)],
    products: [.library(name: "Consumer", targets: ["Consumer"])],
    dependencies: [.package(path: "../first/FirstPackage"), .package(path: "../second/SecondPackage")],
    targets: [
        .target(name: "Consumer", path: "Sources"),
        .testTarget(
            name: "ConsumerTests",
            dependencies: ["Consumer", .product(name: "First", package: "FirstPackage"),
                           .product(name: "Second", package: "SecondPackage")],
            path: "Tests"
        ),
    ]
)
SWIFT
printf 'public enum Consumer {}\n' > "$consumer/Sources/Consumer.swift"
cat > "$consumer/Tests/CoexistenceTests.swift" <<'SWIFT'
import XCTest
import First
import Second
import FirstFFI
import SecondFFI

final class CoexistenceTests: XCTestCase {
    func testBothLibraries() {
        XCTAssertEqual(firstValue(), 1)
        XCTAssertEqual(secondValue(), 2)
    }
}
SWIFT

run_xcodebuild() {
    local derived_data="$1"
    local action="$2"
    local destination="${3:-platform=macOS}"
    (
        cd "$consumer"
        xcodebuild -scheme Consumer -destination "$destination" \
            -derivedDataPath "$derived_data" \
            ARCHS="$architecture" ONLY_ACTIVE_ARCH=YES \
            SWIFT_ENABLE_EXPLICIT_MODULES=YES CLANG_ENABLE_EXPLICIT_MODULES=YES \
            "$action"
    )
}

# Negative control for #374: restore the old nested module maps and binary-only
# discovery. A clean explicit scan must fail, so the positive test cannot pass
# merely because implicit discovery or cached modules masked the regression.
for module in First Second; do
    if [[ "$module" == First ]]; then library=first; else library=second; fi
    package="$tmp_dir/$library/${module}Package"
    headers=("$package/$module.xcframework"/*/Headers)
    for header_dir in "${headers[@]}"; do
        mv "$header_dir/${module}FFI.modulemap" "$header_dir/$library/module.modulemap"
        # The nested map's header path is relative to its new location.
        sed -i '' "s@$library/$library.h@$library.h@" "$header_dir/$library/module.modulemap"
    done
    cat > "$package/Package.swift" <<SWIFT
// swift-tools-version:5.9
import PackageDescription
let package = Package(name: "$module", platforms: [.macOS(.v13)],
    products: [.library(name: "$module", targets: ["$module"])],
    targets: [
        .binaryTarget(name: "${module}FFI", path: "$module.xcframework"),
        .target(name: "$module", dependencies: ["${module}FFI"], path: "Sources"),
    ])
SWIFT
done
if run_xcodebuild "$tmp_dir/Undiscoverable" build-for-testing > "$tmp_dir/negative.log" 2>&1; then
    printf 'Nested module maps unexpectedly passed explicit discovery (#374)\n' >&2
    exit 1
fi
if ! grep -Eiq "(unable to resolve module dependency|no such module).*'(First|Second)FFI'" "$tmp_dir/negative.log"; then
    cat "$tmp_dir/negative.log"
    printf 'Negative control failed for an unexpected reason\n' >&2
    exit 1
fi

for module in First Second; do
    if [[ "$module" == First ]]; then library=first; else library=second; fi
    package="$tmp_dir/$library/${module}Package"
    headers=("$package/$module.xcframework"/*/Headers)
    for header_dir in "${headers[@]}"; do
        mv "$header_dir/$library/module.modulemap" "$header_dir/${module}FFI.modulemap"
        sed -i '' "s@header \"$library.h\"@header \"$library/$library.h\"@" "$header_dir/${module}FFI.modulemap"
    done
    cp "$tmp_dir/$library/Package.swift.saved" "$package/Package.swift"
done
if ! run_xcodebuild "$tmp_dir/Coexistence" test > "$tmp_dir/positive.log" 2>&1; then
    cat "$tmp_dir/positive.log"
    exit 1
fi
for module in First Second; do
    grep -Eq "SwiftExplicitDependencyGeneratePcm.*${module}FFI" "$tmp_dir/positive.log" || {
        cat "$tmp_dir/positive.log"
        printf 'Missing explicit module compilation for %sFFI\n' "$module" >&2
        exit 1
    }
done
grep -F '** TEST SUCCEEDED **' "$tmp_dir/positive.log"
printf 'Two generated BoltFFI packages passed XCTest; nested-map discovery regression reproduced.\n'

# Match the People Work iOS simulator XCTest build without launching a journey.
if ! run_xcodebuild "$tmp_dir/Simulator" build-for-testing 'generic/platform=iOS Simulator' > "$tmp_dir/simulator.log" 2>&1; then
    cat "$tmp_dir/simulator.log"
    exit 1
fi
for module in First Second; do
    grep -Eq "SwiftExplicitDependencyGeneratePcm.*${module}FFI" "$tmp_dir/simulator.log" || {
        cat "$tmp_dir/simulator.log"
        printf 'Missing simulator explicit module compilation for %sFFI\n' "$module" >&2
        exit 1
    }
done
grep -F '** TEST BUILD SUCCEEDED **' "$tmp_dir/simulator.log"
