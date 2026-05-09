# `boltffi.toml` specification

`boltffi.toml` configures `boltffi` code generation and packaging. The CLI reads it from the current working directory.

This document is normative. It defines schema, defaults, validation rules, and command semantics. For walkthroughs and usage examples, see [BOLTFFI_TOML_GUIDE.md](BOLTFFI_TOML_GUIDE.md).

## Minimal example

```toml
[package]
name = "mylib"
```

Everything else is optional with defaults.

## Path and placeholder rules

Path fields are **project-root-relative** unless they are absolute paths or a section explicitly defines a different base path. This applies to `output`, `artifact_path`, and other path fields.

Placeholder references like `{package.crate}` follow these fallback rules:
- `{package.crate}` resolves to `package.crate` if set, otherwise `package.name`.
- `{package.version}` resolves to `package.version` if set, otherwise read from `Cargo.toml`.
- `{package.license}` and `{package.repository}` resolve to their respective fields, or empty string if unset.

## Top-level

### `experimental` (optional)

List of experimental targets or features that are explicitly enabled.

```toml
experimental = ["typescript.async_streams"]
```

- Type: array of strings
- Default: `[]`
- Format: `"target"` or `"target.feature"`
- CLI `--experimental` flag includes experimental targets for that command

Experimental targets:
- `dart`
- `python`

Experimental features:
- `typescript.async_streams`

### `[package]` (required)

- `name` (string): Logical name used for default module/class naming.
- `crate` (string, optional): Rust crate name to scan/build if different from `name`.
- `version` (string, optional): Package version.
  - Default: read from `Cargo.toml`
- `description` (string, optional): Package description.
- `license` (string, optional): Package license identifier.
- `repository` (string, optional): Repository URL.

## Targets

All platform-specific configuration lives under `[targets.*]`. Each target can be independently enabled or disabled.

## Apple

### `[targets.apple]` (optional)

- `enabled` (bool): Whether this target is active.
  - Default: `true`
- `output` (path): Apple artifact root directory.
  - Default: `dist/apple`
- `deployment_target` (string): iOS deployment target (major.minor).
  - Default: `16.0`
- `include_macos` (bool): Whether `boltffi pack apple` also builds macOS targets.
  - Default: `false`
- `ios_architectures` (`["arm64"]`, optional): iOS device slices to build/package.
  - Default: `["arm64"]`
  - Set to `[]` to exclude device slices, as long as at least one Apple slice remains enabled overall
- `simulator_architectures` (`["arm64", "x86_64"]`, optional): iOS Simulator slices to build/package.
  - Default: `["arm64", "x86_64"]`
  - Set to `[]` to exclude simulator slices, as long as at least one Apple slice remains enabled overall
- `macos_architectures` (`["arm64", "x86_64"]`, optional): macOS slices to build/package when `include_macos = true`.
  - Default: `["arm64", "x86_64"]`
  - Set to `[]` to exclude macOS slices, as long as at least one Apple slice remains enabled overall
  - Ignored unless `include_macos = true`

### `[targets.apple.swift]` (optional)

- `module_name` (string, optional): Swift module name for generated bindings.
  - Default: `PascalCase(package.name)`
- `output` (path, optional): Where Swift bindings are generated.
  - Default: `{targets.apple.output}/Sources`
- `ffi_module_name` (string, optional): Name of the FFI module imported by Swift bindings.
  - Default: `{xcframework_name}FFI`
- `tools_version` (string, optional): SwiftPM tools version emitted in `Package.swift`.
  - Default: `5.9`
- `error_style` (`throwing` | `result`): Error surface style in generated Swift.
  - Default: `throwing`

### `[targets.apple.swift.type_mappings]` (optional)

Maps custom types to native Swift types. When a custom type has a mapping, the generated Swift code uses the native type instead of a typealias, with automatic conversion at the wire boundary.

Each mapping is a table with:
- `type` (string, required): The native Swift type to use (e.g., `UUID`, `URL`).
- `conversion` (string, required): The conversion strategy. One of:
  - `uuid_string`: String ↔ UUID (`UUID(uuidString:)` / `.uuidString`)
  - `url_string`: String ↔ URL (`URL(string:)` / `.absoluteString`)

Example:
```toml
[targets.apple.swift.type_mappings]
Uuid = { type = "UUID", conversion = "uuid_string" }
```

### `[targets.apple.header]` (optional)

- `output` (path, optional): Where the generated C header is written.
  - Default: `{targets.apple.output}/include`

### `[targets.apple.xcframework]` (optional)

- `output` (path, optional): Where `{Name}.xcframework` and `{Name}.xcframework.zip` are written.
  - Default: `{targets.apple.output}`
- `name` (string, optional): xcframework base name.
  - Default: `{targets.apple.swift.module_name}`

### `[targets.apple.spm]` (optional)

- `output` (path, optional): Directory where `Package.swift` is written.
  - Default: `{targets.apple.output}`
- `distribution` (`local` | `remote`): Whether `Package.swift` points at a local `.xcframework` or a remote release `.zip`.
  - Default: `local`
- `repo_url` (string, conditional): Base URL for remote releases. Required when `distribution = "remote"`.
- `layout` (`bundled` | `split` | `ffi-only`): SwiftPM layout.
  - Default: `ffi-only`
- `package_name` (string, optional): SwiftPM package name override.
  - Default:
    - `layout = "split"`: `{module_name}FFI`
    - otherwise: `{module_name}`
- `wrapper_sources` (path, optional): Swift target sources path used by `layout = "bundled"`.
  - Interpretation: **relative to `targets.apple.spm.output`** when not absolute.
  - Default: `Sources`
- `skip_package_swift` (bool, optional): Skip generating `Package.swift`.
  - Default: `false`

### `[targets.apple.debug_symbols]` (optional)

Companion archive output for Apple slice libraries collected by `boltffi pack apple`.

- `enabled` (bool): Emit a debug-symbol archive alongside Apple packaging output.
  - Default: `false`
  - Validation: release-like packaging profiles must enable Cargo debuginfo or packaging fails
- `output` (path, optional): Directory where the debug-symbol archive is written.
  - Default: `{targets.apple.output}/symbols`
- `format` (`zip`): Archive format.
  - Default: `zip`
- `bundle` (`unstripped`): Bundle kind for the archived payloads.
  - Default: `unstripped`

## Android

### `[targets.android]` (optional)

- `enabled` (bool): Whether this target is active.
  - Default: `true`
- `output` (path): Android artifact root directory.
  - Default: `dist/android`
- `min_sdk` (integer): Android minSdkVersion used for packaging.
  - Default: `24`
- `ndk_version` (string, optional): NDK version hint (used by environment checks).
- `architectures` (array of strings, optional): Android ABIs to build and package.
  - Supported canonical values: `arm64`, `armv7`, `x86_64`, `x86`
  - Default: all four Android architectures above, in that order
  - Behavior: `boltffi build android`, `boltffi check`, `boltffi doctor`, and
    `boltffi pack android` all resolve against this configured list.
  - `boltffi pack android --no-build` requires one prebuilt Rust static library per configured
    architecture and ignores stale artifacts for unconfigured ABIs.

### `[targets.android.kotlin]` (optional)

- `package` (string, optional): Kotlin package for generated sources.
  - Default: `com.example.{package.name}` (with `-` normalized to `_`)
- `output` (path, optional): Output directory for Kotlin sources and JNI glue.
  - Default: `{targets.android.output}/kotlin`
- `module_name` (string, optional): Kotlin module/object name.
  - Default: `PascalCase(package.name)`
- `library_name` (string, optional): Native library name for `System.loadLibrary`.
  - Default: inferred from the configured package/crate name. The Android load name preserves
    hyphens to match `jniLibs`; the desktop JVM loader uses Cargo-normalized artifact names.
- `desktop_loader` (`bundled` | `system` | `none`): How generated Kotlin loads the native library on non-Android JVMs.
  - Default: `bundled`
  - `bundled`: extract bundled desktop natives when present, otherwise fall back to `System.loadLibrary`
  - `system`: call `System.loadLibrary` on desktop JVMs
  - `none`: skip desktop JVM loading and assume the host process has already loaded the native library
- `api_style` (`top_level` | `module_object`): How functions are exposed.
  - Default: `top_level`
- `factory_style` (`constructors` | `companion_methods`): How factory constructors are exposed.
  - Default: `constructors`
- `error_style` (`throwing` | `result`): Error surface style in generated Kotlin.
  - Default: `throwing`

### `[targets.android.kotlin.type_mappings]` (optional)

Maps custom types to native Kotlin/Java types. Same structure as `[targets.apple.swift.type_mappings]`.

Example:
```toml
[targets.android.kotlin.type_mappings]
Uuid = { type = "java.util.UUID", conversion = "uuid_string" }
```

### `[targets.android.header]` (optional)

- `output` (path, optional): Where the generated C header is written (used by Android JNI builds).
  - Default: `{targets.android.output}/include`

### `[targets.android.pack]` (optional)

- `output` (path, optional): Where `boltffi pack android` writes the `jniLibs/` folder.
  - Default: `{targets.android.output}/jniLibs`

### `[targets.android.debug_symbols]` (optional)

Companion archive output for Android JNI libraries collected by `boltffi pack android`.

- `enabled` (bool): Emit a debug-symbol archive alongside Android packaging output.
  - Default: `false`
  - Validation: release-like packaging profiles must enable Cargo debuginfo or packaging fails
- `output` (path, optional): Directory where the debug-symbol archive is written.
  - Default: `{targets.android.output}/symbols`
- `format` (`zip`): Archive format.
  - Default: `zip`
- `bundle` (`unstripped`): Bundle kind for the archived payloads.
  - Default: `unstripped`

## Java

### `[targets.java]` (optional)

- `package` (string, optional): Java package for generated sources.
  - Default: `com.example.{package.name}` (with `-` normalized to `_`)
- `module_name` (string, optional): Java class name for the public API.
  - Default: `PascalCase(package.name)`

### `[targets.java.jvm]` (optional)

Desktop JVM target configuration.

- `enabled` (bool): Whether JVM target is active.
  - Default: `false`
- `output` (path): Output directory for Java sources, JNI glue, and host native outputs.
  - Default: `dist/java`
- `host_targets` (array of strings, optional): Desired desktop native outputs.
  - Supported canonical values: `current`, `darwin-arm64`, `darwin-x86_64`, `linux-x86_64`, `linux-aarch64`, `windows-x86_64`
  - Supported aliases: `darwin-aarch64`, `darwin-x86-64`, `linux-x86-64`, `linux-arm64`, `windows-x86-64`
  - Default: `["current"]`
- `strip_symbols` (bool): Strip symbol tables from packaged desktop JNI libraries for custom named Cargo profiles used with `boltffi pack java`.
  - Default: `false`
  - Currently supported for Darwin and Linux desktop JNI packaging only.
  - Built-in `release` profile strips desktop JNI symbols automatically on Darwin and Linux.
  - Named profiles such as `--profile dist` strip only when this is set to `true`.
  - Diagnostic profiles such as `--profile asan` should normally leave this unset.
  - `windows-x86_64` does not support this option yet; enabling it there returns an error instead of silently doing nothing.
  - Phase 3 behavior: all configured values must resolve to the current host target after `current` expansion and deduping
  - Packaging layout: `boltffi pack java` writes the JNI library to `dist/java/native/<host-target>/` and also keeps a flat current-host `_jni` copy in `dist/java/`
  - `boltffi pack java --no-build` is unsupported in Phase 3; rerun without `--no-build`

### `[targets.java.jvm.debug_symbols]` (optional)

Companion archive output for desktop JNI libraries collected by `boltffi pack java`.

- `enabled` (bool): Emit a debug-symbol archive alongside JVM packaging output.
  - Default: `false`
  - Validation: release-like packaging profiles must enable Cargo debuginfo or packaging fails
- `output` (path, optional): Directory where the debug-symbol archive is written.
  - Default: `{targets.java.jvm.output}/symbols`
- `format` (`zip`): Archive format.
  - Default: `zip`
- `bundle` (`unstripped`): Bundle kind for the archived payloads.
  - Default: `unstripped`

### `[targets.java.android]` (optional)

Android target configuration for Java (not Kotlin).

- `enabled` (bool): Whether Android Java target is active.
  - Default: `false`
- `output` (path): Output directory for Java sources.
  - Default: `dist/java/android`
- `min_sdk` (integer): Android minSdkVersion.
  - Default: `24`

## WASM

### `[targets.wasm]` (optional)

- `enabled` (bool): Whether this target is active.
  - Default: `true`
- `triple` (string): Rust target triple.
  - Default: `wasm32-unknown-unknown`
- `profile` (`debug` | `release`): Build profile.
  - Default: `release`
- `output` (path): WASM artifact root directory.
  - Default: `dist/wasm`
- `artifact_path` (path, optional): Project-root-relative path to built `.wasm` file.
  - Default: `target/{triple}/{profile}/{package.crate}.wasm`

### `[targets.wasm.optimize]` (optional)

Controls `wasm-opt` optimization pass after build.

- `enabled` (bool): Whether to run `wasm-opt`.
  - Default: `true` for release, `false` for debug
- `level` (`0` | `1` | `2` | `3` | `4` | `s` | `z`): Optimization level.
  - Default: `s`
- `strip_debug` (bool): Remove debug information.
  - Default: `true`
- `on_missing` (`error` | `warn` | `skip`): Behavior when `wasm-opt` is not installed.
  - Default: `error`

### `[targets.wasm.typescript]` (optional)

- `output` (path, optional): Where TypeScript bindings are generated.
  - Default: `{targets.wasm.output}/pkg`
- `runtime_package` (string, optional): Import path for the BoltFFI runtime.
  - Default: `@boltffi/runtime`
- `module_name` (string, optional): Base name for generated files.
  - Default: normalized `{package.name}`
- `source_map` (bool, optional): Generate source maps.
  - Default: `true`

### `[targets.wasm.typescript.type_mappings]` (optional)

Maps custom types to native TypeScript types. Same structure as `[targets.apple.swift.type_mappings]`.

Example:
```toml
[targets.wasm.typescript.type_mappings]
Uuid = { type = "string", conversion = "uuid_string" }
```

### `[targets.wasm.npm]` (optional)

Controls npm package generation in `boltffi pack wasm`.

- `package_name` (string, required for pack): npm package name with optional scope.
- `output` (path, optional): Where the npm package is assembled.
  - Default: `{targets.wasm.typescript.output}`
- `targets` (array of `bundler` | `web` | `nodejs`): Which loader entrypoints to generate.
  - Default: all three
  - Validation: must be non-empty
- `generate_package_json` (bool): Generate `package.json`.
  - Default: `true`
- `generate_readme` (bool): Generate `README.md` scaffold.
  - Default: `true`
- `version` (string, optional): Package version.
  - Default: `{package.version}` or from `Cargo.toml`
- `license` (string, optional): Package license.
  - Default: `{package.license}`
- `repository` (string, optional): Package repository URL.
  - Default: `{package.repository}`

## C#

### `[targets.csharp]` (optional)

- `enabled` (bool): Whether C# generation and packaging are active.
  - Default: `false`
- `output` (path): C# artifact root directory.
  - Default: `dist/csharp`
  - `boltffi generate csharp` writes `.cs` files directly here.
  - `boltffi pack csharp` writes generated sources under `{output}/src`, native assets under `{output}/runtimes/<rid>/native`, and a generated project file at `{output}/BoltFFI.CSharp.csproj`.
- `package_id` (string, optional): NuGet package ID.
  - Default: `{package.name}`
- `target_framework` (string, optional): Target framework for the generated NuGet package project.
  - Default: `net10.0`
- `package_output` (path, optional): Directory where `boltffi pack csharp` writes `.nupkg` files.
  - Default: `{targets.csharp.output}/packages`
- `runtime_identifiers` (array of strings, optional): Desired .NET native runtime asset outputs.
  - Supported canonical values: `current`, `osx-arm64`, `osx-x64`, `linux-x64`, `linux-arm64`, `win-x64`
  - Supported aliases: `darwin-arm64`, `darwin-x86_64`, `linux-x86_64`, `linux-aarch64`, `windows-x86_64`
  - Default: `["current"]`
  - Behavior: `current` resolves to the active host RID, repeated values are deduped after resolution, and native libraries are packaged under NuGet `runtimes/{rid}/native/`.

`boltffi pack csharp` rejects explicit Cargo `--target` passthrough args because the native asset matrix is controlled by `targets.csharp.runtime_identifiers`. Current-host packaging works on `osx-arm64`, `osx-x64`, `linux-x64`, `linux-arm64`, and `win-x64`. Cross-host support follows the shared desktop toolchain support used by JVM packaging; unsupported host/target pairs fail during preflight.

## Apple SwiftPM layouts

`boltffi pack apple` always produces an xcframework (unless `--spm-only`) and can generate `Package.swift` (unless `--xcframework-only`).

**Swift output precedence:** When running `boltffi generate swift` standalone, bindings are written to `[targets.apple.swift].output`. When running `boltffi pack apple`, output location is layout-specific:
- `ffi-only`: write to `{spm.output}/Sources/BoltFFIGenerated/{module_name}.swift`
- `bundled`: write to `{spm.output}/{spm.wrapper_sources}/BoltFFIGenerated/{module_name}.swift`
- `split`: write to `{swift.output}/BoltFFIGenerated/{module_name}.swift`

### `layout = "ffi-only"`

Generates a standalone SwiftPM package containing:

- a binary target `{XcframeworkName}FFI`
- a Swift target `{module_name}` that depends on that binary target
- generated bindings in `{spm.output}/Sources/BoltFFIGenerated/{module_name}.swift`

### `layout = "bundled"`

Generates `Package.swift` that points the Swift target at your existing wrapper sources directory.

- Set `spm.wrapper_sources` to the wrapper target's source directory.
- Generated bindings go into `{spm.output}/{spm.wrapper_sources}/BoltFFIGenerated/{module_name}.swift`.

### `layout = "split"`

Generates a binary-only SwiftPM package intended to be depended on by a separate wrapper package.

- `Package.swift` exposes only the binary target `{XcframeworkName}FFI`.
- Generated Swift bindings are written to `{swift.output}/BoltFFIGenerated/{module_name}.swift` so you can include them in your wrapper target.
