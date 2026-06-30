# Kotlin Multiplatform Support Roadmap

## Executive Decision

Restart the KMP implementation around a Python-style architecture:

- Render pipeline: `plan -> lower -> emit`
- Packaging pipeline: `plan -> layout -> build/stage/package`
- Strict support contract: every API emitted into `commonMain` must compile and run on every selected KMP target.

The pre-reset KMP branch contained useful lessons, tests, naming helpers, target mapping, Gradle snippets, and JVM/Android delegation ideas. It should not be kept as the owner of the new design. In particular, Apple support should stay deleted or quarantined until it can be rebuilt around explicit capability admission and real Kotlin/Native actuals.

## Why Reset

The previous KMP path grew through several PRs:

- Apple cinterop scaffolding
- Native data/function actuals
- Native class and handle support
- Sync callback parameters
- Class-member support pruning
- Support-surface refactoring

That work proved important details, but it also left the implementation with the wrong shape. Before the M0/M1 reset, `boltffi_bindgen/src/render/kmp/mod.rs` mixed support admission, common API rendering, JVM/Android actuals, Apple actuals, Gradle files, cinterop files, JNI reuse, source filtering, and tests. The old `boltffi_bindgen/src/render/kmp/apple.rs` carried a large inline Kotlin/Native runtime and emitted `NotImplementedError` paths for unsupported shapes. The old single-file `boltffi_cli/src/pack/kmp.rs` packaged Android and JVM resources but did not stage or link Apple static libraries.

By contrast, the newer Python backend is cleaner:

- `boltffi_bindgen/src/render/python/plan` defines the target model.
- `boltffi_bindgen/src/render/python/lower` maps `FfiContract` and `AbiContract` into that model and validates collisions.
- `boltffi_bindgen/src/render/python/emit` writes source files from the model.
- `boltffi_cli/src/pack/python` splits packaging into `plan.rs`, `layout.rs`, `build.rs`, and `wheel.rs`.

KMP should follow that pattern.

## Non-Negotiable Invariants

1. No admitted API may compile to a runtime `NotImplementedError`.
2. `commonMain` is the intersection of the selected platform capabilities.
3. Platform admission happens before emission, not inside Apple/JVM rendering helpers.
4. Apple KMP uses an explicit fixed-width ABI input, not an implicit flexible ABI.
5. Packaging owns native library staging and linking; source generation should not guess at build outputs.
6. Unsupported APIs fail with a clear diagnostic by default. If preview pruning remains useful, it should be an explicit opt-in that writes a support report.
7. JVM/Android should delegate to the existing Kotlin/JNI backend instead of copying JNI logic.

## Current Status

M0 is complete:

- `preview_prune_unsupported` defaults to `false`.
- Strict KMP generation fails unsupported APIs by default.
- Preview pruning is explicit and writes support metadata.
- `pack kmp` verifies generated support metadata against the effective config.
- Production KMP generation has no `NotImplementedError` or `NativeRuntimeNotImplemented` fallback path for admitted APIs.

M1 is complete:

- M1a introduced the new IR KMP backend skeleton under `boltffi_backend/src/target/kmp`.
- M1b introduced the KMP plan/lower/admission model, support reports, platform capability intersection, and plan-level tests.
- M1c introduced emit/file-list parity for the default JVM/Android KMP project skeleton from the IR plan.
- M1d introduced the KMP packaging plan/layout foundation under `boltffi_cli/src/pack/kmp`.
- M1c keeps behavior intentionally strict: unsupported or currently unrenderable APIs fail in strict mode and are omitted only in explicit preview-prune mode.
- Empty or fully pruned JVM/Android source sets remain package-only skeletons; they do not emit runtime declarations for APIs that were not admitted.
- M1c does not provide real JVM/Android API body parity. That starts in M2.
- M1d keeps behavior unchanged: KMP packaging still reuses the existing JVM/Android packagers, while generated-project paths now live behind `KmpPackageLayout`.

M2 has started:

- M2a introduced a backend-owned JVM-family delegate seam for KMP.
- `commonMain` expect declarations remain KMP-owned, while JVM/Android internal Kotlin and JNI glue can be supplied by a delegate owned outside `boltffi_backend`.
- The backend can admit a delegated infallible sync primitive free function in strict mode through internal delegate tests.
- M2b-a introduced the `boltffi_bindgen` adapter scaffold that converts `FfiContract` + `AbiContract` into backend JVM-family delegate output for the admitted primitive sync surface.
- M2b-b wired production IR KMP generation through that delegate for the admitted primitive sync surface.
- M2c let the delegate supply trusted generated internal Kotlin function bodies, so JVM/Android emission now reuses the mature Kotlin/JNI function renderer for the currently admitted surface.
- M2d added a production-generation smoke that validates the strict primitive-sync KMP module shape across common, JVM actual, Android actual, internal Kotlin/JNI, Gradle, and support metadata.
- M2e isolated generated KMP C headers under `boltffi_generated/`, made KMP pack validate JVM/Android glue against that generated-header contract, and removed stale root headers that could shadow regenerated headers.
- Unsupported KMP surfaces remain strict and fail-closed unless explicit preview pruning is enabled.

## Target Architecture

### Backend Layout

Create a real KMP backend boundary. The current M1/M2 path is:

```text
boltffi_backend/src/target/kmp/
  mod.rs
  plan/
    mod.rs
  lower/
    mod.rs
    admission.rs
  emit/
    mod.rs
    output.rs
    common.rs
    jvm.rs
    gradle.rs
  names.rs
  bridge.rs
  host.rs
  syntax.rs
```

`mod.rs` should be a thin public facade. It should expose types similar to the Python backend lowerer/emitter and output-file structs.
Apple-specific lower/emit/runtime modules should be added under this backend boundary in M3, not revived from the old production fallback path.

Suggested core structs:

- `KmpModule`: full generated module plan.
- `KmpCommonModule`: declarations emitted to `commonMain`.
- `KmpPlatformModule`: actuals and runtime needs for one source set.
- `KmpCapability`: platform-supported feature set.
- `KmpSupportReport`: admitted, rejected, and pruned API records with reasons.
- `KmpGradleModule`: Gradle targets, source sets, dependencies, cinterop inputs.

### CLI Packaging Layout

M1d split the old `boltffi_cli/src/pack/kmp.rs` into the current package foundation:

```text
boltffi_cli/src/pack/kmp/
  mod.rs
  plan.rs
  layout.rs
```

Future packaging slices can continue splitting orchestration by responsibility:

```text
boltffi_cli/src/pack/kmp/
  generate.rs
  jvm.rs
  android.rs
  apple.rs
```

Suggested core structs:

- `KmpPackagingPlan`: selected source crate, artifact names, build profile, Cargo args, JVM host matrix, Android targets, Apple targets, and generation source directory.
- `KmpPackageLayout`: `commonMain`, `jvmMain`, `androidMain`, `appleMain`, `nativeInterop`, Android `jniLibs`, JVM native resources, Apple static library staging, support-report path, and safety checks.
- `KmpAppleNativeLibraryPlan`: maps Rust targets to Kotlin/Native targets and staged static libraries.

## Milestones

### M0: Decision And Reset

Goal: remove the ambiguous half-supported Apple state.

Work:

- Decide whether to delete the old production `render/kmp/apple.rs` outright or move it behind a quarantined reference module outside the production path.
- Replace current KMP docs that promise runtime throws for unsupported Apple shapes.
- Write the support contract in `BOLTFFI_TOML_SPEC.md` and user docs.
- Add a failing/diagnostic test proving admitted APIs cannot emit `NotImplementedError`.

Exit criteria:

- Production KMP generation has no Apple fallback actuals.
- Unsupported APIs produce diagnostics or explicit preview-pruning records.
- The old branch work is classified into "reuse", "reference only", and "delete".

Verification:

- `rg "NotImplementedError|NativeRuntimeNotImplemented" boltffi_bindgen/src/render/kmp` returns no production emission path for admitted APIs.
- Unit tests cover the strict admission contract.

### M1: Architecture Foundation

Goal: introduce the Python-like structure without expanding behavior.

Work:

- Add `KmpModule`, platform module, output-file, support-report, and Gradle plan structs.
- Move admission decisions into `lower/admission.rs`.
- Move emission into `emit/*`.
- Add `KmpPackagingPlan` and `KmpPackageLayout`.
- Keep JVM/Android behavior equivalent, or intentionally mark the previous behavior as preview-only until M2.

Completed:

- M1a: IR KMP backend skeleton.
- M1b: KMP plan/lower/admission, platform modules, support report, capability intersection, and plan-level tests.
- M1c: emit/file-list parity skeleton for common/JVM/Android from the IR plan.
- M1d: KMP packaging plan/layout foundation under `boltffi_cli/src/pack/kmp`.

Remaining:

- None.

Exit criteria:

- KMP has a small facade similar to Python.
- Lowering can be unit-tested without rendering strings.
- Packaging paths are calculated by `layout.rs`, not scattered through orchestration.

Verification:

- Snapshot tests for generated file lists.
- Plan-level tests for capability intersection and diagnostics.
- Existing KMP JVM/Android snapshots pass or are replaced by equivalent plan-level tests.
- M1c exit check passed on `m1c-b-file-list`:
  - `cargo test -p boltffi_backend kmp -- --nocapture`
  - `cargo test -p boltffi_backend`
  - `cargo test -p boltffi_bindgen kmp`
  - `cargo test -p boltffi_cli kmp`
  - `cargo test -p boltffi_cli kotlin_multiplatform`
  - `cargo test -p boltffi_cli ir_generation`
  - `cargo test -p boltffi_cli ir_kmp -- --nocapture`
  - `cargo fmt --check`
  - `git diff --check`
  - `rg "NotImplementedError|NativeRuntimeNotImplemented" boltffi_bindgen/src/render/kmp boltffi_backend/src/target/kmp`
- M1d exit check passed:
  - `cargo test -p boltffi_cli -- --quiet`
  - `cargo test -p boltffi_backend kmp -- --nocapture`
  - `cargo test -p boltffi_bindgen kmp`
  - `cargo fmt --check`
  - `git diff --check`
  - `rg "NotImplementedError|NativeRuntimeNotImplemented" boltffi_bindgen/src/render/kmp boltffi_backend/src/target/kmp`

### M2: JVM And Android Parity

Goal: rebuild the useful part first, cut production JVM/Android KMP over to the new architecture, and remove the old production KMP path.

#### M2a: Backend Delegate Seam

Goal: prove the KMP backend can admit and emit an infallible sync primitive free function only when a JVM-family delegate is supplied, without claiming production Kotlin/JNI integration.

Completed:

- Added a backend-owned JVM-family delegate seam.
- Kept `commonMain` expect declarations KMP-owned.
- Kept strict mode fail-closed when no delegate covers a function.
- Added backend tests for delegated primitive sync functions, native-symbol/type/name matching, package mismatch rejection, and preview-prune duplicate handling.
- Added an IR CLI regression test proving production `generate kmp --ir` remains fail-closed for unsupported KMP surfaces.

M2a non-goals at the time:

- Do not wire `boltffi_bindgen::Generation::kmp_host()` to Kotlin/JNI output.
- Do not model the mature Kotlin/JNI shared `Native` runtime or C translation-unit preamble as production behavior.
- Do not claim generated KMP projects with non-empty APIs compile through the IR path.

Exit criteria:

- Backend-level delegate seam tests pass.
- Production IR KMP remains fail-closed without the real delegate adapter.
- No KMP fallback stubs or `NotImplementedError` paths are emitted for admitted APIs.

Verification:

- `cargo test -p boltffi_backend kmp -- --nocapture`
- `cargo test -p boltffi_cli ir_kmp_strict_generation_still_fails_closed_for_unsupported_surface`
- `cargo fmt --check`
- `git diff --check`
- `rg "NotImplementedError|NativeRuntimeNotImplemented" boltffi_bindgen/src/render/kmp boltffi_backend/src/target/kmp`

#### M2b: Kotlin/JNI Delegate Adapter

Goal: connect production IR KMP generation to the mature Kotlin/JNI lowerers/emitters.

M2b-a completed:

- Added the `boltffi_bindgen` bridge from `FfiContract` + `AbiContract` to `KmpJvmDelegateOutput` without wiring it into production IR generation.
- Reused the existing Kotlin/JNI lowerers and emitters to build delegate Kotlin/JNI output for infallible primitive sync functions.
- Preserved the shared Kotlin `Native` runtime as members of the same object that declares external methods.
- Preserved shared JNI preamble/includes once per generated translation unit and isolated per-function JNI glue.
- Kept non-primitive/common-to-JVM conversion cases uncovered until an explicit conversion plan exists.

M2b-b completed:

- Wired production IR KMP generation through `Generation::render_kmp()` to build a JVM-family delegate from backend `Bindings<Native>`.
- Preserved backend native symbols when adapting metadata bindings into the legacy Kotlin/JNI lowerer shape.
- Shared KMP default package/module constants from the backend host so delegate package matching cannot drift from host defaults.
- Added a production-host regression proving an infallible primitive sync free function renders common/JVM/Android output through the new delegate path.
- Kept unsupported KMP surfaces strict and fail-closed after delegate wiring.

#### M2c: Delegate-Owned Internal Kotlin Bodies

Goal: keep the KMP backend in charge of common/platform file shape while letting the JVM-family delegate own the internal Kotlin function body implementation.

Completed:

- Added delegate-owned internal Kotlin function source to the backend JVM delegate model.
- Updated JVM/Android emission to prefer delegate-supplied internal Kotlin bodies, retaining direct `Native.*` body rendering only as a manual-delegate fallback.
- Kept backend admission based on typed delegate coverage and non-empty JNI glue; delegate-owned Kotlin source is trusted generated output, not arbitrary Kotlin input validated by the backend.
- Reused the mature Kotlin/JNI function renderer from `boltffi_bindgen` for admitted primitive sync functions, rewriting only the backend-planned Kotlin function name, native symbol, and parameter names.
- Added focused backend and adapter regressions proving delegate-owned internal Kotlin source is propagated into generated KMP JVM output.
- Did not admit new API categories; strings, records, async, classes, callbacks, and broader value conversion remain future explicit plan work.

#### M2d: Current Surface Compile-Readiness Smoke

Goal: prove the currently admitted production IR KMP surface is internally coherent before widening API coverage.

Completed:

- Added a strict production-generation smoke for one infallible primitive sync function.
- Verified the generated file list includes common, JVM actual, Android actual, JVM-family internal Kotlin, JVM/Android JNI glue, Gradle files, and support metadata.
- Verified common `expect`, JVM/Android `actual`, internal delegate function, `Native` external declaration, JNI glue, Gradle JVM/Android targets, and strict support metadata agree for that function.
- Kept this as a deterministic Rust test rather than a default Gradle invocation so normal test runs do not depend on plugin downloads or local Android/Gradle setup.
- Did not admit new API categories or change generation/packaging behavior.

#### M2e: Generated Header Isolation For KMP JNI

Goal: make the generated JVM/Android KMP JNI glue consume cbindgen headers from a generated-header namespace so stale root-level headers cannot shadow regenerated bindings.

Completed:

- Added a delegate-controlled JNI header include so KMP JVM/Android glue includes `boltffi_generated/<module>.h` while standalone non-KMP JNI keeps its existing `<module>.h` default.
- Made KMP pack read and validate both JVM and Android `jni_glue.c` files before packaging.
- Required both source sets to include the same generated header, or both to have no generated include only for empty/comment-only glue that falls back to the selected package header.
- Rejected stale unqualified includes, mismatched generated includes, duplicate generated includes, path-like generated include basenames, and duplicate unqualified preamble-name includes that match the selected generated header basename.
- Taught header generation to create nested parent directories for `boltffi_generated/<module>.h`.
- Removed stale root-level `.h` files from KMP JVM/Android C source roots while preserving nested generated headers and non-header files.
- Kept the cleanup and validation scoped to KMP packaging; standalone Java/JNI generation behavior remains unchanged except for the shared lowerer accepting an explicit header include.

Verification:

- `cargo test -p boltffi_cli kmp`
- `cargo test -p boltffi_bindgen kmp`
- `cargo test -p boltffi_bindgen jni`
- `cargo test -p boltffi_backend kmp`
- `cargo check -p boltffi_cli`
- `cargo fmt --check`
- `git diff --check`
- `rg "NotImplementedError|NativeRuntimeNotImplemented|boltffiNativeRuntimeNotImplemented" boltffi_bindgen/src/render/kmp boltffi_backend/src/target/kmp`

#### M2f: Environment-Gated Gradle Compile Smoke

Goal: prove the current admitted JVM/Android KMP project compiles through Gradle before widening API coverage or deleting more legacy code.

Planned:

- Generate the strict primitive-sync KMP smoke project through the IR path.
- Add an opt-in/env-gated Gradle check so default Rust tests do not depend on local Gradle, Android SDK, or plugin downloads.
- Validate at least JVM compilation for the generated module and Android assemble/source-set wiring when the Android toolchain is available.
- Keep the smoke focused on the currently admitted surface; do not use this slice to admit strings, records, callbacks, classes, or Apple targets.

Exit criteria:

- CI/local developers can opt into a real generated-project compile check.
- The smoke exercises the IR-generated KMP files that pack will consume.
- Failures point to generation, Gradle configuration, or packaging layout with actionable diagnostics.

#### M2g: Delete Legacy Non-IR KMP JVM/Android Production Path

Goal: make IR generation the only production KMP JVM/Android path before starting Apple support.

Planned:

- Remove the legacy CLI generator path in `boltffi_cli/src/commands/generate/languages/kmp.rs` and any remaining production dispatch that can call `KMPEmitter::emit` for KMP generation.
- Keep `boltffi generate kmp --experimental` routed through `commands::generate::ir` and `Generation::render(Target::KotlinMultiplatform)`.
- Move or delete any remaining tests that depend on `KMPEmitter::emit`; retain only migrated helper tests with clear ownership in the new backend, delegate adapter, or packaging modules.
- Extract any still-useful helper snippets from `boltffi_bindgen/src/render/kmp/mod.rs` into owner-correct modules, then delete the monolithic emitter surface instead of leaving it as a fallback.
- Add route-level regressions proving KMP generation cannot take a non-IR path.
- Keep the support metadata/report types available from their new owner before deleting old modules that currently export shared constants.

Exit criteria:

- `KMPEmitter::emit` is not reachable from production code.
- The legacy `KMPGenerator` path is deleted or reduced to no production behavior.
- `boltffi generate kmp --experimental` and `pack kmp` still work for the currently admitted JVM/Android surface through the IR path.
- No KMP-specific duplicate JNI implementation remains outside the delegate adapter.

Verification:

- Generated KMP project still matches the M2d/M2f smoke expectations.
- `rg "KMPEmitter::emit|KMPGenerator|commands/generate/languages/kmp" boltffi_bindgen/src boltffi_cli/src` finds no production KMP generation path.
- `rg "boltffiNativeRuntimeNotImplemented|NotImplementedError|NativeRuntimeNotImplemented" boltffi_bindgen/src/render/kmp boltffi_backend/src/target/kmp` confirms no legacy fallback runtime path remains.

Remaining M2 work after M2e:

- Extend admitted JVM/Android coverage beyond infallible primitive sync functions through explicit common-to-JVM conversion plans.
- Preserve the shared Kotlin `Native` runtime inside the owner object that declares external methods as more API shapes are admitted.
- Preserve shared JNI C preamble/includes/helpers once per generated translation unit as more API shapes are admitted.
- Keep delegate functions filtered to the admitted KMP support surface.
- Add the environment-gated Gradle compile/assemble smoke from M2f once the generated KMP project is ready for CI toolchain provisioning.
- Keep Android `jniLibs` packaging through the existing Android packager.
- Keep JVM native resources through the existing JVM packager.
- Delete the old monolithic `KMPEmitter` production flow through M2g before starting Apple support.
- Keep JVM/Android packaging on the new `pack/kmp/*` orchestration while production parity moves over.
- Retain only migrated helpers, snippets, and tests with clear ownership in the new modules.

Exit criteria:

- `boltffi generate kmp --experimental` creates a Gradle KMP module for JVM/Android.
- `boltffi pack kmp --experimental` builds/stages Android and JVM native resources.
- No KMP-specific duplicate JNI implementation exists.
- The old KMP renderer/packer is no longer reachable as a fallback production path.

Verification:

- Generated KMP project compiles.
- JVM smoke tests run against `jvmMain`.
- Android assemble/package verifies `src/androidMain/jniLibs`.
- Regression tests compare public KMP API shape to existing Kotlin API where appropriate.
- `rg "KMPEmitter|boltffiNativeRuntimeNotImplemented|NotImplementedError" boltffi_bindgen/src/render/kmp boltffi_cli/src/pack/kmp*` confirms no legacy production path remains for admitted APIs.

### M3: Apple Native MVP

Goal: support real Kotlin/Native Apple actuals for sync value APIs, including packaging.

MVP capability set:

- Primitive scalar parameters and returns
- Strings
- Records
- C-style enums
- Data enums
- `Vec<T>` for supported element types
- `Option<T>` for supported value types
- `Result<Ok, Err>` where both sides are supported value types
- Sync free functions and sync value-type constructors/methods only

Work:

- Lower Apple from an explicit fixed 64-bit ABI.
- Generate cinterop `.def` files with headers, compiler options, static libraries, and library paths.
- Stage Rust Apple static libraries under KMP-owned paths per Kotlin/Native target.
- Generate target-specific Gradle cinterop config.
- Generate Apple actuals only for admitted APIs.
- Add macOS and iOS simulator smoke tests before device-only support.

Exit criteria:

- `macosArm64` and `iosSimulatorArm64` compile and link.
- A smoke test calls Rust through Kotlin/Native on macOS and iOS simulator.
- There are no generated fallback actuals for admitted Apple APIs.

Verification:

- `boltffi pack kmp --experimental --release` builds Apple static archives and stages them.
- Gradle cinterop tasks succeed.
- `macosArm64Test` or equivalent native smoke test passes.
- iOS simulator smoke test passes on macOS CI.

Reference:

- Kotlin/Native `.def` files support `headers`, `compilerOpts`, `linkerOpts`, `staticLibraries`, and `libraryPaths`: https://kotlinlang.org/docs/native-definition-file.html

### M4: Classes, Handles, And Lifetimes

Goal: make object APIs safe and deterministic on all selected KMP targets.

Work:

- Add handle return and nullable handle return support.
- Add constructors, named factories, static methods, and instance methods.
- Define close semantics in common API: `AutoCloseable` for JVM/Android and `Closeable` or common-compatible equivalent for KMP.
- Model ownership, borrowed handles, double-close, close-during-call, and cross-thread behavior.
- Decide whether single-threaded classes are excluded from KMP or guarded by target-specific diagnostics.

Exit criteria:

- Class APIs have the same common surface on JVM/Android/Apple.
- Apple handle lifetime rules are encoded in the plan and tests.
- No unsafe lifetime behavior is hidden in emission helpers.

Verification:

- Constructor matrix tests.
- Static and instance method tests.
- Optional handle return tests.
- Double-close and close-after-move tests.
- Thread-safety tests for thread-safe and non-thread-safe class markers.

### M5: Callbacks, Async, And Streams

Goal: complete the runtime-heavy API families once ownership is stable.

Work:

- Add sync callbacks and closure callback ownership.
- Add async callbacks after sync callback `StableRef` ownership is proven.
- Add coroutine bridge for async Rust calls.
- Add cancellation and completion handling.
- Add streams as Kotlin `Flow` or a documented stream abstraction.
- Align callback and async error propagation across JVM/Android/Apple.

Exit criteria:

- Callback, async, cancellation, and stream APIs work on JVM/Android/Apple.
- Runtime resources are released deterministically.
- Backpressure/cancellation behavior is documented and tested.

Verification:

- Callback lifetime tests.
- Callback error propagation tests.
- Async success, failure, and cancellation tests.
- Stream subscribe, next, cancellation, and drop tests.
- Stress tests for callback-after-owner-drop and cancellation races.

### M6: Packaging And Distribution

Goal: make generated KMP modules usable outside the repo.

Work:

- Define Gradle plugin versions and dependency policy.
- Add Maven publication support or a documented local module consumption path.
- Add stable artifact layout for Android, JVM desktop, and Apple.
- Add debug symbol policy for Android/JVM/Apple KMP outputs.
- Decide whether KMP consumes Apple static libraries directly or also offers an xcframework-adjacent distribution mode.
- Add stale-output cleanup for every staged native target.

Exit criteria:

- A clean checkout can generate, pack, publish locally, and consume the KMP module from another Gradle project.
- Native artifact layout is documented and stable.
- Symbols/debug outputs are predictable.

Verification:

- `publishToMavenLocal` or equivalent local publication sample.
- External consumer sample builds without referencing repo-internal paths.
- Stale output tests for target removal.
- Release-like build verifies symbol stripping/debug symbol behavior where supported.

### M7: Demo, CI, And Docs

Goal: make KMP first-class and keep it from regressing.

Work:

- Add `examples/platforms/kmp`.
- Add a KMP overlay config for the demo crate.
- Port representative tests from Swift, Kotlin/JVM, and Python demos.
- Add CI stages gradually:
  - Generate-only
  - JVM test
  - Android assemble
  - macOS native test
  - iOS simulator smoke
- Update public docs only after each capability is real.

Exit criteria:

- KMP is covered in CI across JVM, Android, macOS, and iOS simulator.
- Public docs describe real support, not planned behavior.
- The support report is part of developer diagnostics.

Verification:

- Full demo matrix passes on CI.
- Docs examples are exercised by scripts or smoke tests.
- `boltffi pack all --experimental` has clear KMP behavior.

## Proposed PR Slices

1. Done: reset docs and support contract.
2. Done: introduce KMP plan/lower/emit skeleton and support report.
3. Done: move packaging to `pack/kmp/*` with no feature expansion.
4. In progress: rebuild JVM/Android KMP generation and packaging.
5. Done: connect `boltffi_bindgen` Kotlin/JNI lowerers to the KMP JVM delegate seam.
6. Done: prove current JVM/Android primitive-sync KMP shape and generated-header isolation.
7. Next: add an environment-gated Gradle compile/assemble smoke for the current admitted JVM/Android surface.
8. Delete the legacy non-IR KMP JVM/Android production path.
9. Add Apple target/layout planning and static library staging.
10. Add Apple sync value actuals.
11. Add Apple classes/handles.
12. Add callbacks.
13. Add async and streams.
14. Add publication/demo/CI/docs hardening.

## Salvage Plan

Reuse:

- Target enums and Apple target selection logic from `target.rs` and the legacy KMP generator.
- JVM/Android delegation concept.
- Existing Android and JVM packagers.
- Naming helpers where they match generated Kotlin conventions.
- Tests that describe desired support decisions.

Reference only:

- Apple runtime snippets formerly in `render/kmp/apple.rs`.
- cinterop Gradle snippets.
- Support-surface pruning tests.

Delete or replace:

- Production Apple fallback actuals.
- Inline support pruning inside string emitters.
- Monolithic `KMPEmitter::emit` orchestration.
- Any behavior where unsupported APIs compile and throw at runtime.

## Key Product Decisions

1. Should unsupported APIs fail generation by default?

   Recommendation: yes. Add explicit preview pruning only if needed for demo iteration.

2. Should KMP support be enabled from `[targets.apple]`, `[targets.android]`, and `[targets.java.jvm]`, or should KMP have its own platform matrix?

   Recommendation: KMP should have its own platform matrix that can default from existing target configs. Reusing existing configs silently makes it too easy to accidentally widen the KMP common surface.

3. Should Apple KMP use static libraries through cinterop or consume an xcframework?

   Recommendation: start with staged static libraries through cinterop for the MVP. Revisit xcframework-adjacent distribution in M6 if consumer packaging requires it.

4. Should `pack kmp --no-build` be supported?

   Recommendation: not until layouts and stale-output cleanup are stable. A later implementation can validate all expected staged native artifacts before reusing them.

## Risk Register

- Apple cinterop/linking risk: static library paths and target-specific `.def` files must be deterministic and relocatable.
- ABI mismatch risk: Apple must use fixed 64-bit ABI consistently from lowerer to header to actuals.
- Common-surface risk: platform-specific gaps can leak into `commonMain` unless admission is centralized.
- Runtime ownership risk: callbacks, async, streams, and handles need explicit lifetime models before emission.
- Test matrix cost: iOS simulator and Android tests will be slower; phase CI in after each capability stabilizes.
- Documentation risk: public docs should lag implementation until the smoke tests exist.

## Success Definition

KMP is fully supported when a user can:

1. Enable KMP for JVM, Android, macOS, and iOS simulator/device.
2. Run `boltffi pack kmp --experimental --release`.
3. Consume the generated Gradle module from a separate project.
4. Call the same supported API surface from JVM, Android, and Apple Kotlin/Native.
5. Get generation-time diagnostics for unsupported APIs instead of runtime surprises.
6. Rely on CI-covered demos for primitives, records, enums, results, classes, callbacks, async, and streams.
