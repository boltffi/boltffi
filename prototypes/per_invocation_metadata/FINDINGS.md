# Findings

Prototype of **p**er-**i**nvocation **m**etadata capture — "pim", the prefix on every crate, tag,
and section name in this workspace — motivated by the silent-visibility bug: bindgen ignores
macro-generated `#[data]`/`#[export]` items while `boltffi generate` exits 0. Built and run on the
repo's pinned toolchain, **stable rustc 1.95.0, edition 2024, macOS/arm64**. Everything below is
backed by running code — 30 tests via `cargo test` (§1–§5 are the metadata phase, §6–§9 the
expansion phase) plus a manual reader pass for the section-survival numbers in §1.

**Verdict: the approach works, and it works better than expected.** Every capability question came
back yes — including, in phase 2, the wrapper codegen itself: a `#[export]` invocation can emit its
own `extern "C"` wrapper, and a reader can call it through the symbol name read from the artifact.
The costs are real but bounded, and one of them is a user-facing requirement the team has to decide
to accept (§4a).

---

## 1. Does const-built metadata with `module_path!()` work on stable?

**Yes, with no nightly features.** `pim_runtime::record::<N>` builds the whole record — magic,
length prefix, module path, slot strings, JSON payload — inside a `const fn`; `record_len` computes
the length and the writer ends in `assert!(at == N)`, so a framing bug is a **compile error**, not
a corrupt section. The statics are align-1, so the linker packs them; the reader tolerates padding
anyway.

Section survival, the risk I most expected to bite, is a non-issue: record counts are identical in
`dev` and `release-lto` (`lto = "fat"`, `strip = "symbols"`, `codegen-units = 1`,
`panic = "abort"`) — 17/17 at phase 1, 27/27 with phase 2's function records (§7). `#[used]` +
`link_section` survives the profile that actually ships.

## 2. Are macro-emitted and `include!`'d items captured?

**Yes — all three previously-invisible cases appear:** emitted by `macro_rules!`
(`pim_toy::emitted::MacroEmitted`), emitted by a proc macro (`emitted::ProcEmitted`, via
`define_record!`), and `include!(concat!(env!("OUT_DIR"), …))` (`outdir::BuildScriptRecord`). This
falls out for free: `module_path!()` expands wherever the macro output lands. The root cause of the
silent-visibility bug — discovery reading source text — is gone, because discovery now reads
*compiler output*.

## 3. Does compiler-mediated resolution survive real import graphs?

**Yes, every case.** The macro never resolves a path; it splices the field's tokens into a const
expression and rustc does the rest. Verified against the reader's output (`roundtrip.rs`):

| case | resolves to |
|---|---|
| aliased import (`use geometry::Point as GeoPoint`) | `pim_toy::geometry::Point` |
| glob import (`use physics::*`) | `pim_toy::physics::Body` |
| re-export (`crate::reexport::Shape`) | the **defining** module: `pim_toy::geometry::Shape` |
| same name, different modules | `geometry::Point` and `physics::Point` stay distinct; each site picks the one in scope |
| recursion (`Node { next: Option<Box<Node>> }`) | `Option<Box<pim_toy::nested::Node>>` — no const cycle; the consts depend on identity, not fields |
| generic nesting | `HashMap<String, Vec<pim_toy::geometry::Point>>` |
| cross-crate through a re-export | `pim_dep::shapes::DepPoint` |

There is no name-uniqueness rule and no heuristic anywhere in the reader.

## 4. What breaks

### 4a. The orphan rule forces a `Tag` parameter on the trait — and a crate-root anchor

A tagless `trait TypeInfo` cannot be implemented for a foreign type (E0117, verified in the
`remote_without_local_tag` ui test), which kills `custom_type!`. The fix is UniFFI's: a `Tag` type
parameter. Local types get a blanket `impl<Tag> TypeInfo<Tag>`; a foreign type is impl'd against
one crate's own tag; every referencing site reads `<T as TypeInfo<crate::PimTag>>::MODULE`.
Cross-crate references keep working because local types are blanket over `Tag`.

**The cost: every crate using `#[data]` must have a `crate::PimTag` at its root** — a proc macro
cannot inject an item at the crate root from a module — so `boltffi::scaffolding!()` at the top of
`lib.rs` becomes mandatory, the equivalent of UniFFI's `setup_scaffolding!()`. **This is a
breaking, user-visible API change and the main thing to decide before committing.** Second-order
cost: `custom_type!` registrations are per crate and cannot be inherited (same restriction as
UniFFI, known-tolerable).

### 4b. A fixed container vocabulary breaks on type aliases

Classifying fields against a name list (`Vec`, `Option`, `Box`, `HashMap`, `Result`) fails on
`type Points = Vec<Point>` (`alias_to_container` ui test): the compiler sees through the alias, the
list does not. The fix is built and proven in `pim_runtime::compose`: replace the two `&str` consts
with a composable descriptor — `Meta`, a fixed-capacity const buffer that blanket impls concatenate
(UniFFI's `TYPE_ID_META`). Aliases, nested aliases, and `Option<Box<Vec<u64>>>` — a type no macro
ever saw — all compose (`pim_toy::composed`, 3 tests). The price is a fixed capacity (overflow is a
const `assert!`), because length-generic arrays need nightly `generic_const_exprs`.
**Recommendation: skip the vocabulary, go straight to the composable descriptor.**

### 4c. Generics have no canonical id

`#[data]` rejects generic structs outright. Boltffi does not support generic FFI types today
either — a hard ceiling, not a to-do.

### 4d. A clippy ICE, worth knowing before someone hits it

Reconstructing a `&'static str` from a `static [u8; N]` in const context ICEs clippy 1.95
(`mir/consts.rs:176: expected memory, got Static`); rustc is fine, and `const` instead of `static`
avoids it. It never touches the recommended design — the metadata static is only written, never
const-read — but it rules out the "bake the joined id into a `&'static str`" variant. Worth an
upstream report.

## 5. Can a reader reconstruct the graph from the artifact?

**Yes, from both the cdylib and the rlib**, via `object` 0.39 — already a boltffi dependency. Two
structural facts the tests pin down: **an rlib carries only its own crate's records** — aggregation
happens at the link step, so the bindgen aggregator must read the linked artifact or it will
silently miss every dependency's types — and an unreferenced dependency's records still reach the
dylib (`pim_dep::shapes::DepLine`): rustc does not let the linker drop `#[used]` statics, so the
classic inventory-pattern failure does not materialize.

---

## 6. Can the wrapper codegen be per-invocation too? (phase 2)

**Yes.** Each `#[export]` emits its own wrapper adjacent to the function, plus a record carrying
the wrapper's link symbol; the acceptance test dlopens the cdylib and calls wrappers **through
symbol names read from their own records**. The current expander's four pieces of whole-crate
knowledge each have a per-invocation replacement, all backed by running code:

| whole-crate dependency today | per-invocation replacement |
|---|---|
| sequential `SymbolId`s from `SymbolAllocator` | the record carries its own symbol *name*; ids become a bindgen-side concern |
| direct-vs-encoded via the whole-crate `Index` | decided once at the type's own `#[data]` site; use sites are agnostic through `Codec<Tag>::FfiType` |
| `root_visible_paths` / the whole `pub use` graph | **machinery deleted** — the wrapper sits next to the item, so types resolve under exactly the tokens the signature wrote |
| error-payload reverse pass over all callables | `Result<T, E>` codec at the boundary; the reverse pass remains a bindgen aggregation over records |

The third row is the pleasant surprise: `nudge(point: GeoPoint, …)` and
`from_dep(point: pim_dep::DepPoint)` resolve the alias and the re-export by the compiler, making
`RootModuleTypes`/`root_visible_paths` unnecessary rather than merely tolerable to lose.

## 7. Symbol names without a module path, on stable

`module_path!()` answers only at const-eval, too late to name an `extern "C"` item, so symbols are
`pim_{CARGO_CRATE_NAME}_{item}_{fnv1a(Span file:line:column)}` (span accessors stable since 1.88).
Recorded behavior: exports emitted by one `macro_rules!` share a span hash — the span points at the
macro *definition* — and the item name keeps them distinct; the residual collision window
(same name, same macro definition site, different modules) is a duplicate-symbol **link error**,
loud, never silent. An `include!(OUT_DIR)` export hashes the OUT_DIR path, so reproducible-build
implications should be checked at promotion time. All 8 symbols survive `release-lto` via
`#[unsafe(export_name = …)]`, alongside all 27 records.

## 8. The codec moves into the trait system

The whole-crate `Index` exists to answer one question per type: direct or encoded? Phase 2 moves
the answer to the type's own `#[data]` site: `impl<Tag> Codec<Tag> for T` with
`FfiType = Self` (`#[repr(C)]`, all-primitive) or `RawBuffer`, plus `"direct"` in the record — so
the compiled ABI and what bindgen reads **agree by construction**. Wrapper signatures project
through `Codec::FfiType` and compile warning-free (no `improper_ctypes`; the lint sees the
post-projection types). The direct check is deliberately conservative, and the unsafe direction
(direct ABI without direct layout) cannot be expressed.

Design facts with production consequences:

- **Bounds on generated impls must be lazy** (`where` clauses per field type), so `#[data]` on
  `Deadline { after: Duration }` compiles even though `custom_type!` gives `Duration` no encoding;
  the error surfaces at the export that actually moves one, shaped by
  `#[diagnostic::on_unimplemented]` (`export_unannotated_param` ui test).
- **Recursive encoded types hit the trait solver's overflow guard (E0275)** if a wrapper ever
  instantiates `Node: Encode`. Production codec impls must avoid self-referential bounds —
  boltffi's real codec recurses at runtime, so this is likely a prototype artifact, but it has to
  be checked, not assumed.
- **The metadata vocabulary is wider than the codec.** `char`, `HashMap`, and `Result`-as-field all
  classify fine in records, but none has an `Encode` impl here — so with lazy bounds, `#[data]`
  accepts them and the error only surfaces at the first export that moves a value. Loud but late,
  after the record has already said yes. Production should make "representable in metadata" and
  "movable across the boundary" the same set by construction.

## 9. The dlopen round-trip

The reader loads `libpim_toy.dylib`, looks up each function's symbol **from its record**, and calls:

| call | proves |
|---|---|
| `add_vec2(Vec2, Vec2) -> Vec2` | direct records cross by value; ABI matches a hand-declared `#[repr(C)]` mirror |
| `describe_shape(Shape) -> String` | encoded records cross as owned buffers; field order and framing match |
| `checked_div(1.0, 0.0)` | the `Result` error arm carries the typed payload |
| `double_it(21.0)` | a `macro_rules!`-emitted export is discoverable *and callable* |
| `build_script_sum(vec![1.5, 2.5])` | an `include!(OUT_DIR)` export is discoverable and callable |
| `extra_ping` absent | a cfg-gated export leaves no record and no symbol when the feature is off |

The buffer contract is deliberately naive: both sides assume the same process and allocator, and
the test frees returned buffers by reconstructing the `Vec`. A real implementation needs an
explicit free export and boltffi's actual byte codec — the framing here is a stand-in.

## What phase 2 still does not cover

- **Callbacks and streams.** The biggest remaining expansion surface (vtables, foreign-to-Rust
  dispatch, the `trait_path` machinery); needs its own phase.
- **Methods, receivers, async.** `#[export]` rejects them; object/class exports are unexplored.
- **Panic handling.** Wrappers call the user's function and `lift()` bare inside `extern "C"` — a
  panicking function body, or a malformed buffer (the prototype codec panics on truncated input),
  aborts the process from a foreign call. Production wrappers need `catch_unwind` and an error
  return, which may reshape the wrapper signature this phase designed; decide before freezing it.
- **The wasm32 surface.** One `cfg(target_family)`-selected wrapper variant should replace the
  env-var-driven dual build, but no wasm artifact was built or read here.
- **`custom_type!` values at the boundary.** Referenceable (§4a) but no `Encode`; production needs
  user-supplied conversions, as UniFFI does.
- **Bindgen-side aggregation.** The reader lists records and calls symbols; boltffi's global
  lowering passes move behind it per the RFC, but were not prototyped.

## Implications for the real implementation

**The silent-failure mode disappears entirely, and that is the headline.** A referenced type
without an id is a **compile error at the exact field**, and `#[diagnostic::on_unimplemented]`
makes it read like a boltffi diagnostic ("`NotData` has no canonical id, so it cannot cross the FFI
boundary … annotate `NotData` with `#[data]`"). The acceptance bar was "must not exit 0 in
silence"; this does not exit 0 at all. Two further wins: `cfg` is evaluated by rustc instead of
approximated (`boltffi_scan::ActiveCfg` becomes unnecessary), and records carry scan-level items,
so the Native/Wasm32 dual metadata build collapses into one.

What this costs on the boltffi side, in rough order of pain:

1. **`boltffi::scaffolding!()` becomes mandatory.** Breaking API change — decide this first;
   everything else is contingent on accepting it.
2. **The single-shot `EMITTED: AtomicBool` guards go away** in `metadata_build.rs` and
   `expansion_build.rs`, along with the whole-crate `scan_package` re-scan they gate.
3. **Lowering moves from the macro to a bindgen-side aggregator.** The global passes —
   family-indexed ids, `SymbolAllocator`, the error-payload reverse pass — run once over the union
   of records. The direct/encoded choice leaves the list entirely (§8).
4. **`Generation::bindings` must stop assuming one blob per surface.**
   `boltffi_bindgen/src/generate.rs:635-643` takes the first envelope matching the target surface;
   the transport underneath is already plural end-to-end.
5. **The aggregator must read the linked artifact**, not the rlibs (§5).
6. **The expansion build needs the same surgery.** Proven end to end for plain functions (§6–§9),
   deleting `RootModuleTypes`/`root_visible_paths` as a bonus; callbacks, streams, and methods
   remain unprototyped.

Items 1–6 are de-risked with running code for the surface this prototype covers; the open edges
are the ones listed under "What phase 2 still does not cover".
