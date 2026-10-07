# Adding a new target language

A new language uses the IR-based path. Do not copy a folder out of `boltffi_bindgen/src/render/<lang>/`, that is the old path and it is frozen. A target is a host backend that renders from the Binding IR. The IR already carries every decision about how a value crosses the boundary, so the host reads declarations and emits source text, nothing more.

The plumbing comes first so the CLI can reach the target, and after that the demo renders one feature at a time. The steps below follow that order.

Before you write any code, open a tracking issue for the target and lay out the approach in it, the same way the C# and Java targets did. The C# target's issue (https://github.com/boltffi/boltffi/issues/146) is a good template to follow. It is the place to record which bridge stack the target uses, which features land in what order, and where the work currently stands, so a reviewer and anyone picking it up later can see the plan without reading the whole branch. A target is a long stretch of work spread across many PRs, and the issue is what keeps that thread visible.

## Step 1: add the target to the enum

In `boltffi_bindgen/src/target.rs`, add a variant to the `Target` enum and return its name from `name()`. The name is the lowercase string used on the command line.

The demo audit has its own target registry in `examples/demo/src/bin/demo_tests.rs`. Add the target to `TARGETS`, and add its platform scan entry when the target has platform tests or exclusions. The CLI enum makes the command valid. The demo audit lists are what make `just demo-test-audit` enforce markers and exclusions for the new target.

## Step 2: build the host backend

The target lives at `boltffi_backend/src/target/<lang>/`, a sibling of the existing target folders, and its subtree follows the same shape the other targets use:

```text
target/<lang>/
├── mod.rs
├── name_style.rs   (identifier casing and reserved-word escaping)
├── file_group.rs   (only when the language emits multiple source files needing typed routing rules)
├── render/         (Decl + BridgeContract + RenderCx -> Emitted)
├── codec/          (impl CodecRead/CodecWrite, OpRender for the language's primitives)
└── runtime/        (handwritten support code shipped with the generated source)
```

Register the target with `pub mod <lang>;` in `boltffi_backend/src/target/mod.rs`. The backend compiles a target folder only when `mod.rs` declares that target.

The only required flow is `Bindings + BridgeContract -> templates -> Emitted`. `codec/` is where the language's primitive read and write live, the six codec leaves plus its op rendering, while the shared walkers handle composition. `runtime/` is the handwritten support code that ships alongside the generated source.

`codec/` implements `CodecRead`, `CodecWrite`, and `OpRender` for the language's leaves, and that is the only place encoded movement is allowed to happen. Move a value across the boundary by feeding `ReadPlan::render_with`, `WritePlan::render_with`, and `Op::render_with` your leaf implementations. Do not write a second walker over `CodecNode` or branch on `TypeRef` to decide how bytes move. The IR already settled the shape and the host only spells each leaf in the target language. `TypeRef` is fine for names, annotations, and display, but never for transport.

The three render boundaries stay separate. `render/` fills templates and may call the codec boundary, but it does not walk `CodecNode` itself. `codec/` moves encoded values and renders nothing else. `runtime/` is handwritten and ships as-is. Package source, native module source, and codec logic are different files, and this split is exactly where the CPython target had to be untangled, so keep them apart from the start.

If a single language can target more than one runtime, split the host per runtime into its own submodule. The existing CPython target keeps its host under a `cpython/` subfolder for exactly that reason, but a target with one runtime does not need the extra layer.

The host implements the `HostBackend` trait from `boltffi_backend/src/core/host.rs`. It has one method per declaration kind plus `assemble`:

```rust
fn record(&self, decl: &RecordDecl<Self::Surface>, ...) -> Result<Emitted>;
fn enumeration(&self, decl: &EnumDecl<Self::Surface>, ...) -> Result<Emitted>;
fn function(&self, decl: &FunctionDecl<Self::Surface>, ...) -> Result<Emitted>;
fn class(&self, decl: &ClassDecl<Self::Surface>, ...) -> Result<Emitted>;
fn callback(&self, decl: &CallbackDecl<Self::Surface>, ...) -> Result<Emitted>;
fn stream(&self, decl: &StreamDecl<Self::Surface>, ...) -> Result<Emitted>;
fn constant(&self, decl: &ConstantDecl<Self::Surface>, ...) -> Result<Emitted>;
fn custom_type(&self, decl: &CustomTypeDecl, ...) -> Result<Emitted>;
fn assemble(&self, ...) -> Result<GeneratedOutput>;
```

Each `decl` already carries every crossing decision, so the method reads it and emits text. It does not classify anything and it does not ask "is this an error or a value," that was settled before the decl arrived.

`HostBackend` carries three associated types the host has to pin down before any method compiles. There is `Surface`, the binding surface this target consumes, and `Bridge`, the bridge contract it accepts, and `Syntax`, the language's syntax fragments. The surface decides which IR the host reads. A target that calls the C ABI directly consumes `Native`, and a target that goes through Wasm consumes `Wasm32`. Pick it once, because it threads through every `Decl<Self::Surface>` the host receives.

The render code builds generated source from typed fragments, not plain `String`s.
`LanguageSyntax` in `boltffi_backend/src/core/syntax.rs` defines fragments for identifiers, types, expressions, statements, literals, and argument lists.
An identifier field cannot accept an expression.
Each fragment owns its casing and escaping, and the template prints the fragment.
`LanguageSyntax::keyword` checks reserved names.
A language can supply `KEYWORDS` for the default predicate or override `keyword` with a parser.

Ruby uses `ruby-prism` for keyword checks.
Backend builds require a C compiler and `libclang` for `bindgen`.
The crate supplies Prism's C source.
Prism runs in the generator, not in generated Ruby extensions.
Update `ruby-prism` when the supported Ruby grammar changes.

The `ruby-prism` 1.9.0 Rust `parse` API uses Ruby 4.1 grammar by default.
That API has no version parameter.
The current check classifies candidate names, not complete generated Ruby programs.

`HostBackend`, `BridgeBackend`, `BridgeContract`, and `LanguageSyntax` are all sealed inside `boltffi_backend`. An in-crate target implements the matching `sealed` marker alongside the public trait, otherwise the first build fails with a private-bound error that reads like the trait does not exist. One line per impl settles it.

Do not build output strings by hand. Every render method fills an Askama template. The template files live on disk under:

```text
boltffi_backend/templates/target/<lang>/
```

So a function template is the real file `boltffi_backend/templates/target/<lang>/function.<ext>`, where `<ext>` is the extension of the code that template emits. Askama's template root is `boltffi_backend/templates/` (the crate has no `askama.toml`, so that default applies), and the `path` in the derive is that file location with the root stripped off:

```rust
use askama::Template;

#[derive(Template)]
#[template(path = "target/<lang>/function.<ext>", escape = "none")]
struct FunctionTemplate {
    name: Identifier,
    return_type: TypeFragment,
    body: Vec<Statement>,
    // ...more fragment-typed fields the template renders
}
```

The fields are the language's syntax fragments from Step 2, not `String`. The render method builds an `Identifier`, a `TypeFragment`, a list of `Statement`s, and the template prints them through their `Display`. The fragment is what carries the casing and escaping, so the template never has to. The existing target's template structs in `boltffi_backend/src/target/python/cpython/render/` are full of this, an `Identifier` for the C function name, an `Expression` for a default value, never a bare string standing in for code.

Pick `<ext>` from the code the template actually contains, never `.txt` or `.askama`. The extension is what gets the template syntax-highlighted in an editor. A template emitting C uses `.c`, one emitting Java uses `.java`, a Python type stub uses `.pyi`, a manifest uses `.toml`. A target that bridges through C, the way the existing CPython target does, writes C into most of its templates and names them `.c`. Use `escape = "none"` because the output is source code, not HTML.

These do not all have to work at once. Step 8 covers building them up one at a time, so the first method to land is `function`, with the rest returning an error until their turn comes.

## Step 3: build or reuse the bridge

The bridge is the foreign surface the generated code calls into. Bridges live in `boltffi_backend/src/bridge/`, and they stack with `BridgeLayer`. The shared C ABI bridge is `CBridge`, and a host that can call the C ABI directly may need nothing more than that.

A runtime that needs its own surface (a JVM needs JNI, a CPython extension needs the Python C API) gets a second bridge layered on top of `CBridge`. That layer is a `BridgeBackend` from `boltffi_backend/src/core/bridge.rs`, and the trait is more than a constructor. It names an `Input` (the value the layer consumes, either `Bindings` for a base bridge or the contract from the layer below), a `Contract` it produces for the layer or host above, and a `Syntax`. It implements `build_contract`, which turns the input into that contract, and `render_bridge`, which emits the files this layer owns. The contract itself implements `BridgeContract` and reports its `capabilities`, which is what `Target::render` checks the host against. `BridgeBackend` and `BridgeContract` are sealed, so the layer implements the `sealed` marker too.

Build the layer under `boltffi_backend/src/bridge/<surface>/` and stack it:

```rust
BridgeLayer::new(CBridge::default_header()?, MyRuntimeBridge::new(...)?)
```

Keep the stacking inside the target, not out in the generator. The host knows which bridges it needs and in what order, so it is the only thing that should be putting them together. Give it an `into_target` method that builds the stack and hands back a ready `Target`, the same way `PythonCExtHost::into_target` does:

```rust
pub fn into_target(
    self,
    bindings: &Bindings<Native>,
) -> Result<Target<Self, BridgeLayer<CBridge, MyRuntimeBridge>>> {
    Ok(Target::new(self, BridgeLayer::new(CBridge::default_header()?, /* ... */)))
}
```

Bindgen then calls `into_target` and renders. It does not hand-assemble the C bridge plus the runtime bridge itself, because the target decides its own bridge stack.

## Step 4: declare capabilities

In the `HostBackend` impl, `binding_capabilities()` lists which declaration kinds the host renderer should receive. The statuses live in `boltffi_backend/src/core/capabilities.rs`, and they are `stable`, `experimental`, `in_progress`, and `unsupported`. Mark a kind `unsupported` while the host cannot render that declaration family at all.

This is the safety net for incremental work. `Target::render` in `boltffi_backend/src/core/target.rs` checks the crate being generated against this table before rendering, so a crate that uses an unsupported declaration kind fails with a clear message instead of producing broken output.

`bridge_capabilities()` declares what the host needs from the bridge below it, and `Target::render` checks that too.

Use this table as the declaration-kind gate, not as the per-shape work queue. In partial coverage, a non-stable declaration kind is skipped before the host renderer runs. That means `Functions` cannot stay `in_progress` while primitive functions are being brought up, because the renderer would never see any function declarations. Once work starts on a declaration kind, set that kind to `stable` so partial mode calls the renderer, then let the renderer own the remaining gaps with typed errors and coverage diagnostics.

Partial coverage keeps the demo running and prints the remaining per-shape gaps, so treat that report as the work queue. Before a declaration kind is considered done for the target, run complete coverage and delete every matching demo exclusion. If generation or the audit still reports a skipped declaration for that kind, the kind is still not finished even though the renderer is allowed to receive it.

## Step 5: wire it into the generator and the CLI

Three places connect the host to the `boltffi` command.

The IR driver in `boltffi_bindgen/src/generate.rs` needs an arm in `Generation::render` and a `render_<lang>` method that builds the host, calls `into_target` on it to get the composed target, and renders. `render_python` is the pattern to copy, where it builds the host, calls `host.into_target(&bindings)`, and passes the result through `render_backend`.

The generate command in `boltffi_cli/src/commands/generate/bindings.rs` and `boltffi_cli/src/commands/generate/mod.rs` needs the new target routed into `run_generation`, guarded by `config.should_process` so the experimental gate applies.

The argument parsing in `boltffi_cli/src/cli.rs` (the `GenerateTargetArg` and `PackTargetArg` enums) and the config in `boltffi_cli/src/config/` (a `[targets.<lang>]` section in `targets.rs` plus output paths in `config/mod.rs`) expose the target and its settings.

Update `BOLTFFI_TOML_SPEC.md` in the same change. If the new target adds `[targets.<lang>]`, package names, output paths, feature flags, or packaging options, the config spec should show them there.

## Step 6: make the output consumable

Generation is not the whole target. A target also needs a packaging story that feels normal for the language.

The generated files should land in the shape a user can consume directly: a Swift package with an XCFramework, a Python wheel, an npm package for TypeScript, a JVM package for Java or Kotlin, a NuGet package for C#, or a header plus library layout for C. The exact shape depends on the language, but the rule is the same: the user should not have to hunt for generated files, copy a shared library by hand, or guess which manifest to write next.

Keep this separate from the renderer. The backend target renders the files. The pack command stages the compiled Rust artifact and assembles the language's package or binary layout.

Add the target's packaging config under `boltffi_cli/src/config/targets/` and the pack implementation under `boltffi_cli/src/pack/` when the target needs one. If generation alone is the right user story for a target, say so in the config and docs, but do not leave packaging implicit.

The demo should prove the package, not just the generated source. Use `just demo-verify --platform <lang>` once the platform has a demo runner, because that command exercises the path a user cares about: generate, package, install or load, and run the platform tests.

## Step 7: mark the whole target experimental

A new target does not ship as production-ready, so it goes into `Experimental::ALL` in `boltffi_cli/src/config/experimental.rs` as a whole target:

```rust
Experimental::WholeTarget(Target::<Lang>),
```

Once it is in that list, the CLI refuses to generate it unless the user opts in, either with `--experimental` on the command line or with `experimental = ["<lang>"]` in their `boltffi.toml`. Every target ships through this gate before it stabilizes. The gate logic is `should_process` in `boltffi_cli/src/config/mod.rs`, and the policy is described at https://docs.boltffi.dev/docs/experimental. The target stays here until it goes GA in Step 11.

An experimental target is expected to reach GA in six to eight weeks of active work. The window is not bureaucracy, it is what keeps the whole model in your head while you build, and the work gets much harder if you put it down and come back cold. A target that stalls half-finished is a maintenance cost the project did not sign up for, so we reserve the right to remove an incomplete target that goes stale rather than carry it indefinitely. Plan the feature order against that window and keep the tracking issue current so it is clear the work is still moving.

## Step 8: drive it with the demo, one feature at a time

`examples/demo` is the crate to generate against. Its `src/` folder is split by feature, from simple to complex (see `examples/demo/README.md`), roughly in this order:

`primitives/`, `records/`, `options/`, `results/`, `enums/`, `classes/`, `callbacks/`, `async_fns/`, plus `bytes/`, `builtins/`, `custom_types/`, `multicrate/`.

Work through it in that order. Generate the demo through the new target:

```bash
just demo-generate <lang>
```

The two coverage modes serve two different moments. While bringing a kind up, `just demo-generate` runs the CLI in `CoverageMode::Partial` (set in `boltffi_cli/src/commands/generate/bindings.rs`): it skips declarations the host cannot render yet, collects them into the coverage report, and prints what it skipped instead of failing. That is what keeps the demo usable mid-build, so you see every remaining gap in one pass.

`CoverageMode::Complete` is the stabilization check, and it is the default on `Generation`. Under it, anything not built yet stops generation. A declaration kind still marked `unsupported` is rejected up front by the capability gate with `Error::BindingCapability`. A shape a stable renderer cannot handle yet either returns a render error or records a coverage diagnostic, and a complete render with any incomplete coverage fails with `Error::IncompleteCoverage` naming the declaration and reason. Run complete mode to prove a feature is actually done, and each failure names the next task, so build that case, mark the kind `stable` once it fully works, and run again.

As you build out each feature, add a snapshot test for it. The point is to pin down what the target actually emits, so that the next change to the renderer cannot quietly shift the output without someone seeing it in the diff. The existing target shows the shape to copy. The `python_target_renders_*` snapshot tests in `boltffi_backend/src/target/python/cpython/mod.rs` render a small piece of the demo through the host and assert on the source that comes out, the parse helper, the boxed return, the method table entry, the generated Python wrapper. When you add strings, add the test that renders a string function and checks the emitted code. When you add records, do the same for a record. By the time a feature is `stable`, its rendered output is locked in by a test, and you can read those tests to see exactly what the target produces for every feature it supports.

Do one feature per PR, or a small closely related group. Primitives land first and get solid, then records, and so on. Each PR stays small enough to review, and problems show up early instead of piling into one giant change.

## Step 9: keep the audit honest

Every behavior the demo promises is written as a `demo_case` on the Rust item. A platform proves it covers a case by leaving a marker in its own test files. CI runs an audit that enforces one rule per case per target: the case is either covered by a marker or explicitly excluded, never both and never neither.

```bash
just demo-test-audit
```

A target that cannot do something yet does not delete the case or fake a marker. It excludes itself from the case with a reason. From `examples/demo/src/results/basic.rs`:

```rust
#[demo_bench_macros::demo_case(
    "results.basic.safe_divide.should_return_quotient",
    justification = "...",
    directions = "...",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "the lowerer does not currently emit Result-returning functions; include this case when Result returns are implemented"
    )
)]
```

The two reasons are defined in `examples/demo/src/bin/demo_tests.rs`:

- `ImplementationGap`, the target genuinely cannot do this yet. This is the one used while building a target.
- `CoverageGap`, the target can do it but the platform test is not written yet.

The `details` string is required, and it tells the next reader why the case is excluded and what unblocks it.

Landing a feature closes the loop: implement the renderer path, make sure the declaration kind is allowed through the capability gate, write the platform test markers, and delete the matching exclusions. The audit goes green for that feature and stays green.

To see where everything stands:

```bash
just demo-test-report
```

It prints each case, which targets support it, and the exclusion counts per target.

Before a feature PR goes up, run the same checks every backend change runs. The focused backend test for the target, the demo generation command, and then the workspace gates:

```bash
just test-crate boltffi_backend
just fmt-check
just lint
```

If the target emits snapshot or fixture output, regenerate and review the diff so an unintended change to generated source shows up in review instead of in someone's build.

## Step 10: update the user documentation

Before a target goes GA, update the docs users read when they decide whether to adopt it. That means the overview, installation or getting-started notes if they mention supported languages, packaging docs, configuration docs, examples, and any language-specific page the target needs.

The docs should show the real user path: how to enable or configure the target, how to generate or pack it, what package or binary layout appears on disk, and how the target language imports or loads it. If the target was listed as experimental, update the experimental docs too.

Do this before removing the experimental gate. A target is not GA if users can build it but the docs still describe it as missing, experimental, or unsupported.

## Step 11: promote to GA

The target goes GA once it can render the whole demo and the work that proves it is all in place. Every declaration kind is `stable`, complete coverage passes with no skipped declarations, the audit is green with no `ImplementationGap` exclusions left, and the snapshot tests cover the features the target supports. At that point the experimental gate is the only thing still holding it back.

Promoting it is removing it from the gate. Delete the target's `Experimental::WholeTarget(Target::Python)` entry from `Experimental::ALL` in `boltffi_cli/src/config/experimental.rs` (with `Python` replaced by the new target), and the CLI generates the target without `--experimental`. It is now a normal target alongside the others. Do this only after everything above is true, because once the gate is gone the target is something people build against for real.

1. Add the target to the `Target` enum in `boltffi_bindgen/src/target.rs`.
2. Build the host backend under `boltffi_backend/src/target/<lang>/`.
3. Stack it on a bridge, reusing `CBridge` and adding a runtime layer if needed.
4. Declare capabilities, keeping unsupported declaration kinds out and setting active kinds to `stable` so they reach the renderer.
5. Wire it into `boltffi_bindgen/src/generate.rs` and the CLI under `boltffi_cli/src/`.
6. Make the generated output consumable as the target language's normal package or binary layout.
7. Add it to `Experimental::ALL` so it is opt-in.
8. Generate `examples/demo` and render features one at a time, one PR each, adding a snapshot test as each feature comes up.
9. Keep `just demo-test-audit` green by excluding what is not built and removing exclusions as each feature lands.
10. Update overview, packaging, configuration, examples, and any language-specific docs.
11. Promote to GA by removing the target from `Experimental::ALL` once every kind is stable, coverage is complete, the audit is green, and the docs match the new status.

## Reference files

- `boltffi_bindgen/src/target.rs`, the target enum.
- `boltffi_bindgen/src/generate.rs`, the IR driver.
- `boltffi_backend/src/core/host.rs`, the `HostBackend` trait.
- `boltffi_backend/src/core/bridge.rs`, the `BridgeBackend` and `BridgeStack` traits.
- `boltffi_backend/src/core/syntax.rs`, the `LanguageSyntax` fragment family.
- `boltffi_backend/src/core/capabilities.rs`, the capability statuses.
- `boltffi_backend/src/core/target.rs`, the render gate.
- `boltffi_backend/src/bridge/`, the bridges and `BridgeLayer`.
- `boltffi_cli/src/config/experimental.rs`, the opt-in list.
- `boltffi_cli/src/config/targets/`, target packaging configuration.
- `boltffi_cli/src/pack/`, package assembly for targets that need it.
- `docs/src/content/docs/overview.mdx`, supported-language overview.
- `docs/src/content/docs/packaging.mdx`, packaging user guide.
- `docs/src/content/docs/configuration.mdx`, target configuration user guide.
- `docs/src/content/docs/experimental.mdx`, experimental target list.
- `examples/demo/README.md`, the demo tour.
- `examples/demo/src/bin/demo_tests.rs`, the audit and exclusion reasons.
