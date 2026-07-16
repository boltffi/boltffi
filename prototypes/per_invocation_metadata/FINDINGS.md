# Findings

Prototype of per-invocation metadata capture, motivated by the silent-visibility bug: bindgen
ignores macro-generated `#[data]`/`#[export]` items while `boltffi generate` exits 0.
Built and run on the repo's pinned toolchain, **stable rustc 1.95.0, edition 2024, macOS/arm64**.
Everything below is backed by running code in this workspace — 20 tests via `cargo test`, plus a
manually run reader pass for the section-survival table in §1.

**Verdict: the approach works, and it works better than expected.** Every capability question came
back yes. The costs are real but bounded, and one of them is a new user-facing requirement that the
team has to decide it is willing to accept.

---

## 1. Does const-built metadata with `module_path!()` work on stable?

**Yes, with no nightly features.** `pim_runtime::record::<N>` builds the whole record — magic, length
prefix, module path, slot strings, JSON payload — inside a `const fn` that writes into a `[u8; N]`.
`const_mut_refs` has been stable since 1.83, so the const writer needs nothing special. The length is
computed by a second `const fn` (`record_len`) and the writer ends in `assert!(at == N)`, so a framing
bug is a **compile error**, not a corrupt section.

The `[u8; N]` is align-1, so the linker concatenates the statics with no padding; the reader tolerates
padding anyway.

Section survival, which was the risk I most expected to bite, is a non-issue:

| build | records in `libpim_toy.dylib` |
|---|---|
| `dev` | 17 |
| `release-lto` (`lto = "fat"`, `strip = "symbols"`, `codegen-units = 1`, `panic = "abort"`) | 17 |

`#[used]` + `link_section` survives fat LTO and symbol stripping on macOS. This mirrors boltffi's real
`release-lto` profile, so the transport holds under the profile that actually ships.

## 2. Are macro-emitted and `include!`'d items captured?

**Yes — all three previously-invisible cases now appear.** These are exactly the cases the source
scan cannot see today:

| case | canonical id recovered from the artifact |
|---|---|
| emitted by a `macro_rules!` macro | `pim_toy::emitted::MacroEmitted` |
| emitted by a proc macro (`define_record!`) | `pim_toy::emitted::ProcEmitted` |
| `include!(concat!(env!("OUT_DIR"), …))` | `pim_toy::outdir::BuildScriptRecord` |

This falls out for free rather than being engineered: `module_path!()` expands wherever the macro
output lands, so an item generated into `OUT_DIR` and `include!`d into `mod outdir` reports
`pim_toy::outdir` without anyone tracking include paths. The root cause of the silent-visibility
bug — that discovery reads source text — is gone, because discovery now reads *compiler output*.

## 3. Does compiler-mediated resolution survive real import graphs?

**Yes, every case.** The reader's output on `libpim_toy.dylib` (verbatim):

```
pim_dep::shapes::DepLine
  from       pim_dep::shapes::DepPoint
  to         pim_dep::shapes::DepPoint

pim_dep::shapes::DepPoint
  x          f64

pim_toy::aliased::Route
  start      pim_toy::geometry::Point
  end        pim_toy::geometry::Point

pim_toy::emitted::MacroEmitted
  anchor     pim_toy::geometry::Point

pim_toy::emitted::ProcEmitted
  id         u64
  anchor     pim_toy::physics::Point

pim_toy::fromdep::Wrapper
  point      pim_dep::shapes::DepPoint

pim_toy::geometry::Point
  x          f64
  y          f64

pim_toy::geometry::Shape
  origin     pim_toy::geometry::Point
  points     Vec<pim_toy::geometry::Point>
  id         u64

pim_toy::globbed::Sim
  body       pim_toy::physics::Body

pim_toy::nested::Index
  table      HashMap<String, Vec<pim_toy::geometry::Point>>

pim_toy::nested::Node
  next       Option<Box<pim_toy::nested::Node>>

pim_toy::outdir::BuildScriptRecord
  id         u64
  anchor     pim_toy::geometry::Point

pim_toy::physics::Body
  position   pim_toy::physics::Point

pim_toy::physics::Point
  magnitude  f64

pim_toy::plain::Flags
  a          bool
  b          u32

pim_toy::reexport::Drawing
  shape      pim_toy::geometry::Shape

pim_toy::remote::Deadline
  after      std::time::Duration
  label      String
```

Reading that table against the hard cases:

- **Aliased import.** `aliased::Route` is written `use crate::geometry::Point as GeoPoint;` and
  `start: GeoPoint`. It resolves to `pim_toy::geometry::Point`.
- **Glob import.** `globbed::Sim` is written `use crate::physics::*;` and `body: Body`. Resolved.
- **Re-export.** `reexport::Drawing` references `crate::reexport::Shape`, which is a `pub use` of
  `geometry::Shape`. It resolves to `pim_toy::geometry::Shape` — the **defining** module, not the
  re-exporting one. That is the correct answer and it is not one a source scan gets for free.
- **Same name, different modules.** `geometry::Point` and `physics::Point` stay distinct, and each
  referencing site picks up the one that was actually in scope.
- **Recursion.** `nested::Node { next: Option<Box<Node>> }` resolves to
  `Option<Box<pim_toy::nested::Node>>` with no const cycle — the `TypeInfo` consts depend on the
  type's *identity*, not its fields, so a self-reference never re-enters const evaluation.
- **Generic nesting.** `HashMap<String, Vec<pim_toy::geometry::Point>>`.
- **Cross-crate.** `fromdep::Wrapper` references `pim_dep::DepPoint` (itself a re-export) and resolves
  to `pim_dep::shapes::DepPoint`.

There is no name-uniqueness rule and no heuristic anywhere in the reader. The macro never resolves a
path; it splices tokens into a const expression and the compiler does the rest.

## 4. What breaks

### 4a. The orphan rule forces a `Tag` parameter on the trait — and a crate-root anchor

This is the finding with real design consequences, so I want to be precise about it.

The naive trait — `trait TypeInfo { const MODULE; const NAME; }` — **cannot be implemented for a
foreign type**, which kills `custom_type!(remote = …)`. Verified, `tests/ui/remote_without_local_tag.rs`:

```
error[E0117]: only traits defined in the current crate can be implemented for types defined outside of the crate
 |
1 | impl pim_runtime::TypeInfo<()> for std::time::Duration {
 |      ---------------------------      -------------------
 |      |                                `Duration` is not defined in the current crate
 |      this is not defined in the current crate because this is a foreign trait
```

The fix is the one UniFFI uses: give the trait a `Tag` type parameter.

- Local types get a blanket impl: `impl<Tag> TypeInfo<Tag> for Local` — legal, `Local` is local.
- A foreign type gets an impl against **one crate's own tag**:
  `impl TypeInfo<crate::PimTag> for std::time::Duration` — legal, `PimTag` is local, and that is
  enough to satisfy the orphan rule.
- Every referencing site is emitted as `<T as TypeInfo<crate::PimTag>>::MODULE`.

That works (`remote::Deadline.after` → `std::time::Duration`, above), and because local types are
blanket-impl'd over `Tag`, a *dependency's* types still resolve against the *referencing* crate's tag
— so cross-crate references keep working (`fromdep::Wrapper`, above).

**The cost: every crate using `#[data]` must have a `crate::PimTag` type at its root**, because the
emitted code names exactly one path and a proc macro cannot inject an item at the crate root from a
module. That means a new required item — `boltffi::scaffolding!()` at the top of `lib.rs`, the
equivalent of UniFFI's `setup_scaffolding!()`. **This is a breaking, user-visible API change**, and it
is the main thing to decide before committing to this approach. I could not find a way around it: the
tag must be local to the declaring crate (a tag owned by `boltffi` itself would put two foreign types
in the impl and violate the orphan rule again).

Second-order cost: a remote type is registered *per crate*. If crate A does `custom_type!(Duration)`,
crate B cannot inherit it and must repeat the declaration. UniFFI has the same restriction, so it is
known-tolerable, but it is a semantics change from today's scan-based model.

### 4b. A fixed container vocabulary breaks on type aliases

My `#[data]` classifies each field as primitive / container / slot using a **fixed name list**
(`Vec`, `Option`, `Box`, `HashMap`, `Result`). That is a simplification, and it fails on an alias.
Verified, `tests/ui/alias_to_container.rs`:

```
error[E0277]: `Vec<Point>` has no canonical id, so it cannot cross the FFI boundary
   |
12 |     pub points: Points,          // pub type Points = Vec<Point>;
   |                 ^^^^^^ no canonical id
```

Note what the compiler says: it *did* see through the alias to `Vec<Point>`. It just has no impl.
Today's source scanner resolves such aliases, so this would be a regression — but the error message
names the fix, and I built it:

**`pim_runtime::compose` replaces the two `&'static str` consts with a single composable descriptor**
— a fixed-capacity const byte buffer (`Meta`, 256 bytes) that blanket impls concatenate. This is what
UniFFI's `TYPE_ID_META` is, and it removes the vocabulary entirely:

```rust
impl<Tag, T: TypeMeta<Tag>> TypeMeta<Tag> for Vec<T> {
    const META: Meta = Meta::new().push(b"Vec<").concat(T::META).push(b">");
}
```

Proven on stable in `pim_toy::composed` (3 tests): `type Points = Vec<Point>` yields
`Vec<pim_toy::composed::Point>`; `type Lookup = HashMap<String, Points>` yields
`HashMap<String, Vec<pim_toy::composed::Point>>`; and `Option<Box<Vec<u64>>>` — a type no macro ever
saw — composes correctly. Aliases, nesting and arbitrary compositions all work, because the compiler
resolves the alias to the concrete type before impl selection.

The price is a **fixed capacity**: descriptors longer than the buffer are a compile error (a const
`assert!`), because `[u8; T::LEN]` where `LEN` depends on `T` needs `generic_const_exprs`, which is
still nightly. UniFFI pays exactly this price. If boltffi goes this way, the buffer size is a tuning
knob, not a design flaw.

**Recommendation: skip the vocabulary and go straight to the composable descriptor.** The slot-table
form in the rest of this prototype is simpler to read but is the weaker design.

### 4c. Generics have no canonical id

`#[data]` rejects generic structs outright (there is no single id for `Foo<T>`). Boltffi does not
support generic FFI types today either, so this is a non-issue — but it is a hard ceiling, not a
to-do.

### 4d. A clippy ICE, worth knowing before someone hits it

Reconstructing a `&'static str` from a `static [u8; N]` in const context **ICEs clippy 1.95**
(`rustc_middle/src/mir/consts.rs:176: expected memory, got Static`). Minimal repro:

```rust
const fn str_from(bytes: &'static [u8]) -> &'static str { … }
fn main() {
    static ID: [u8; 3] = make::<3>();
    const ID_STR: &str = str_from(&ID);   // clippy ICEs here; rustc is fine
    assert_eq!(ID_STR, "aaa");
}
```

`rustc` compiles it; only clippy dies, and only when it const-evaluates the value (e.g. inside
`assert_eq!`). Changing `static ID` to `const ID` avoids it. This never touches the recommended
design — the metadata static is only ever *written*, never read back in const — but it does rule out
the "bake the joined id into a `&'static str`" variant, and it is worth an upstream clippy report.

## 5. Can a reader reconstruct the graph from the artifact?

**Yes, from both the cdylib and the rlib**, via `object` 0.39 — the same crate boltffi already uses.
`pim_reader` parses the Mach-O `__DATA,__pimmeta` section (or the archive members of an rlib), walks
the length-framed records, joins the slot pairs, and substitutes them into the JSON payload.

One structural fact the acceptance test pins down: **an rlib carries only its own crate's records.**
`pim_dep`'s records appear in `libpim_toy.dylib` but not in `libpim_toy.rlib` — aggregation happens at
the **link** step. So the bindgen aggregator must read the *linked* artifact (cdylib/staticlib), not
per-crate rlibs, or it will silently miss every dependency's types.

Encouragingly, `pim_dep::shapes::DepLine` reaches the dylib even though **nothing in `pim_toy`
references it**: rustc does not let the linker drop an unreferenced dependency's `#[used]` statics.
That was a live risk (the classic inventory-pattern failure) and it did not materialize.

---

## Implications for the real implementation

**The silent-failure mode disappears entirely, and that is the headline.** Today a type that should be
FFI-visible but isn't produces silence and a zero exit code. Under this design, a referenced type
without an id is a **compile error at the exact field**, and `#[diagnostic::on_unimplemented]` makes
it read like a boltffi diagnostic rather than a trait-solver dump:

```
error[E0277]: `NotData` has no canonical id, so it cannot cross the FFI boundary
 --> tests/ui/unannotated_field.rs:9:16
  |
9 |     pub inner: NotData,
  |                ^^^^^^^ no canonical id
  |
  = note: annotate `NotData` with `#[data]`, or declare it with `custom_type!` if it is foreign
```

The acceptance bar for the silent-visibility bug is that the build must not exit 0 in silence.
This overshoots it: the build does not exit 0 at all.

Two further wins that fall out of moving discovery into the compiler:

- **`cfg` is evaluated for real.** A `#[cfg(feature = "extras")]` item is absent from the section
  without the feature and present with it (`cfg_is_evaluated_by_the_compiler_not_approximated`).
  `boltffi_scan::ActiveCfg`, which today *approximates* cfg evaluation from Cargo env vars, becomes
  unnecessary.
- **The payload is surface-agnostic.** Records carry scan-level items, not a lowered
  `SerializedBindings`, so the Native/Wasm32 dual metadata build collapses into one.

What this costs on the boltffi side, in rough order of pain:

1. **A crate-root anchor (`boltffi::scaffolding!()`) becomes mandatory.** Breaking API change. Decide
   this first — everything else is contingent on accepting it.
2. **The single-shot `EMITTED: AtomicBool` guards go away** in `metadata_build.rs` and
   `expansion_build.rs`, along with the whole-crate `boltffi_scan::scan_package` re-scan they gate.
   Every invocation emits its own record.
3. **Lowering moves from the macro to a bindgen-side aggregator.** The global passes that need the
   full set — family-indexed ids, the sequential `SymbolAllocator`, the error-payload reverse pass,
   and the direct/encoded codec choice that needs referenced definitions — then run once, over the
   union of records, exactly as they do today.
4. **`Generation::bindings` must stop assuming one blob per surface.** `boltffi_bindgen/src/generate.rs:635-643`
   currently takes the *first* envelope matching the target surface via `.find()`. The transport
   underneath it is already plural (`BFFIMD01` framing decodes N records; `BindingMetadataReader` is
   plural end-to-end with contract-hash dedup), so this is the one consumer that has to change.
5. **The aggregator must read the linked artifact**, not the rlibs (see §5).
6. **The expansion build needs the same surgery.** This prototype only covers *metadata*. For a
   macro-emitted `#[export]` to actually *work*, the wrapper codegen in `expansion_build.rs` needs
   per-invocation treatment too — it has the same `EMITTED`-guarded rescan. Roughly doubles the
   macro-side effort, and this prototype says nothing about it.

Item 6 is the honest gap in this prototype. Items 1–5 are now de-risked with running code.
