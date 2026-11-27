# ADR: Per-Target Support Contract. Declared Shapes, Strict by Default

## Status
Proposed

## Authors
- [Henrik](https://github.com/akesson)

## Date
2026-11-27

---

## 1. Background and Problem

### 1.1 One declaration, three outcomes

```rust
#[export]
pub fn f(a: Vec<(i32, String, Option<R>)>) -> Vec<(i32, String, Option<R>)> { a }
```

The macro accepts this. `boltffi generate` then does one of three things,
depending on the target:

| target | exit | result |
|---|---|---|
| Swift | 1 | `swift target cannot render multi-statement codec write` |
| Java | 1 | `java target cannot render tuple wire size` |
| Kotlin | 0 | `f` left out, reason `multi-statement wire writer` |
| TypeScript | 0 | `f` left out, reason `tuple codec size` |
| Python, C# | 0 | `f` generated |

Leaving a declaration out is a skip. It prints a table to stderr and exits 0.
`--deny-skipped` (#799, #802) makes it fail, but the flag is opt-in and the demo
scripts do not pass it. So one Rust API can have a different surface in each
language while CI stays green.

Other shapes split the same way. All of these pass the macro:

| declaration | Swift | Kotlin | Java | Python | TS | C# |
|---|---|---|---|---|---|---|
| `f(a: ((i32, (String, bool)), Vec<u8>))` | ok | ok | fail | ok | skip | ok |
| `f(a: (i32,))` | ok | skip | fail | ok | skip | ok |
| `f() -> impl Fn(i32) -> i32` | ok | fail | skip | skip | skip | skip |
| callback trait method taking `Box<dyn Fn(i32) -> i32>` | fail | skip | skip | skip | skip | skip |
| `f(a: Option<Option<i32>>)` | ok | ok | ok | ok | ok | skip |
| `f() -> Option<K>`, `K` a class | ok | ok | ok | skip | ok | ok |
| class with `async fn new() -> Self` | ok | ok | ok | fail | ok | ok |

Measured on main at `2f93f706` with `boltffi generate` and no flags. "ok" means
generated with nothing skipped. The output was not compiled or run.

The demo is affected too. `generate java` skips `DEMO_PAIR` and `DEMO_SINGLE`,
`generate kotlin` skips `DEMO_SINGLE`, and the overview page claims full support
for both languages.

### 1.2 A skip can leave bindings that do not compile

```rust
#[data] pub enum N { X(Vec<(String, R)>), Y { m: HashMap<String, Vec<E>> } }
#[export] pub fn f(a: N) -> N { a }
```

Kotlin and TypeScript skip `N`, exit 0, and still emit `f`:

```kotlin
fun f(a: N): N {        // kotlinc: unresolved reference 'N'
```

```ts
export function f(a: N): N {   // tsc: TS2304 Cannot find name 'N'
```

A skip caused by a render error removes that one declaration and nothing that
uses it.

What gets removed also varies. For a class with one method that returns a
closure, TypeScript and C# leave out the method, Java leaves out the class and
every function that takes it, and Kotlin and Python fail.

### 1.3 Why the outcome differs

No policy picks between fail and skip. It depends on which pass raises the
error:

| where the error is raised | effect | example |
|---|---|---|
| anywhere, on Swift, Dart or KMP: the CLI never asks for partial output | fail | both Swift failures |
| a host's per-declaration method | skip | Kotlin and TypeScript tuples |
| the bridge build, before any declaration is visited | fail | Kotlin closure return |
| `preflight_coverage` | fail | Java tuples |
| `assemble` | fail | Python `async fn new` |

Java also has an admission pass that removes the shapes it knows before
rendering. It knows closure returns, so those skip. It does not know tuples, so
those reach the preflight and fail.

A failure names the shape and not the declaration:
`java target cannot render tuple wire size`, with no hint which export has the
tuple.

### 1.4 No single record of what a target supports

| record | granularity | written |
|---|---|---|
| `binding_capabilities()` | declaration kind, plus `InternedString` | per host |
| `unsupported("…")` render errors | shape, free text | per call site, several hundred |
| Java's admission pass, the KMP support plan | shape, typed | per target |
| demo `exclude(…, ImplementationGap)` | demo case | per case |
| the docs | prose | by hand |

The capability table marks every declaration kind `stable` on every GA target,
meaning one that does not need `--experimental`. It says nothing about the gaps
above.

The render errors are the only complete record, and each target words them
differently. A closure return is `closure function return` on Java,
`closure return` on Python and TypeScript, and `non-primitive function returns`
on C#.

Nothing keeps the records in agreement. `constants.mdx` says a constant can be
a tuple, and the only place that says Java has no tuples is one demo exclusion.
A new backend adds one more set of answers.

## 2. Decision

1. Each target declares the shapes it supports in its capability table.
2. Generation fails when a target cannot render a declaration, unless the user
   passes `--allow-skipped`.
3. Every gap on a GA target has an issue, or the macro rejects the shape for
   all targets.

### 2.1 One table, per shape

A shape is a property of a declaration that some target cannot render, such as
a closure return. `InternedString` is already handled as one. `boltffi_binding`
tells whether a declaration uses it. `BindingCapabilityAnalysis` adds it to that
declaration's requirements and to those of every declaration that names it. The
host's table says whether the host supports it, and `Target::render_with_coverage`
checks before the renderer runs.

Every known gap becomes a `BindingCapability` variant handled the same way:

```rust
pub enum BindingCapability {
    Records, Enums, Functions, Classes, Callbacks, Streams, Constants, CustomTypes,
    InternedString,
    SingleElementTuple, NestedOptional, ClosureReturn, AsyncInitializer, // …
}
```

```rust
// kotlin
HostCapabilities::new()
    .stable(BindingCapability::Functions)
    .stable(BindingCapability::NestedOptional)
    .gap(BindingCapability::SingleElementTuple, 942, "tuples render as Pair or Triple")
    .gap(BindingCapability::ClosureReturn, /* issue */, "closure returns are not rendered")
```

The names are illustrative. The rules for a variant:

- It is defined by a test on the Binding IR, so it means the same for every
  target.
- A target supports it fully or not at all. Swift renders a tuple parameter but
  not a tuple inside a `Vec` (§1.1), so those are two variants.
- `gap(capability, issue, reason)` is today's `Unsupported` status plus an issue
  number. The other statuses keep their meaning.
- Every host lists every shape variant, and a test enforces it. A variant added
  for one target's gap cannot switch the shape off on another by omission.

The check changes in three ways:

- **It runs before the bridge is built.** Today the bridge comes first, which
  is why the Kotlin closure return fails. Under `--allow-skipped`, the bridge
  and the host see only what remains.
- **Requirements propagate for every shape.** `f(a: N)` requires what `N`
  requires, so nothing is emitted without the types it names (§1.2).
- **It is the only place that decides.** Bridges, `preflight_coverage` and
  `assemble` stop raising shape errors, and their checks move into the variant
  tests. If a host method still returns `UnsupportedTarget` for a declaration
  that passed, the table is missing an entry. Generation fails and names the
  declaration. Under `--allow-skipped` the declaration is left out and listed
  as untracked.

Java's admission pass, the KMP support plan and the C host's filter fold into
the check.

The support matrix in the docs is generated from the tables, and a test fails
when the checked-in page is stale.

### 2.2 Strict by default

`generate` and `pack` fail when a target cannot render a declaration, and list
all of them:

```
kotlin cannot render 2 declarations
kind         name     shape                  tracked
function     f        closure return         #…
method       S::c     single-element tuple   #942
pass --allow-skipped to generate without them
```

`--allow-skipped` generates without them and prints the same table.
`--deny-skipped` is accepted and ignored, with a deprecation note, for one
release.

What is left out depends on where the shape sits:

- In a method or initializer of a class, record or enum: the member. Its type
  stays, as on TypeScript and C# today.
- In the fields or payloads of a type: the type and everything that names it.
- In a callback trait: the whole trait, because the foreign side implements all
  of it.

Swift and Dart are strict already and gain `--allow-skipped`. KMP is strict
unless `preview_prune_unsupported` is set. That setting stays, as the
configuration spelling of `--allow-skipped`, because `pack kmp` checks the
support report against it. Experimental targets follow the same rule, and
`just demo-generate` passes `--allow-skipped`.

**Migration.** A crate that has a skip today fails on upgrade. The error lists
the declarations and names the flag. The default flips in one minor release,
with an entry in `migrating.mdx` and no warning release first (§4.3).

### 2.3 Every gap is tracked or rejected

- **Tracked**, the default. The target's table has a `gap` entry with an issue
  number. A closure return is tracked: Swift renders it and five targets get an
  entry. So is a callback method taking `Box<dyn Fn>`: no GA target renders it,
  but C does.
- **Rejected.** The macro refuses the shape for every target, as it does for a
  13-element tuple today. This is only for a shape that no target renders and
  nobody plans to render. None of the gaps in §1.1 qualifies. Rejecting a shape
  the macro accepts today breaks crates with no flag to get past it, so it is
  decided per shape.

A test fails when a GA target has a gap entry without an issue. Experimental
targets are exempt, because their tracking issue covers the whole target. A gap
closes when its entry becomes `stable` in the PR that makes its corpus cases
pass (§2.4).

A demo `ImplementationGap` exclusion that exists because a declaration is not
generated names its capability, and `just demo-test-audit` checks that the
target's table has that gap. Other exclusions are unchanged, including those
for bindings that are generated and behave differently, such as "C has no
default arguments".

### 2.4 Conformance

A corpus crate holds each shape variant in every position it covers: parameter,
return, inside `Vec`, `Option` and a map, record field, and enum payload. For
each target a test generates the corpus. A `stable` shape must generate, and
compile where CI has the toolchain. A gap must be stopped by the check. A
missing table entry fails the test.

A `stable` claim is only as strong as its corpus cases. A combination the
corpus lacks can still fail in the renderer. That failure names the
declaration, and the fix adds the case.

The corpus checks that bindings exist and compile, not their bytes. ADR 0006,
in draft, covers the bytes: its test vectors run on a target for the shapes
this table calls `stable`.

A new backend starts with a table and the corpus. C++ (#916) and Go should wait
for steps 1 to 3.

## 3. Rollout

1. Core removes the dependents of every skipped declaration (§1.2). This does
   not depend on the rest.
2. Flip the default: `--allow-skipped`, the full table on failure, the
   deprecated `--deny-skipped`, and the docs. The demo then runs strict on GA
   targets, which means resolving the tuple constants Java and Kotlin skip. The
   scripts that pack Ruby or C pass `--allow-skipped`.
3. The shape variants, the check ahead of the bridge, the corpus, every
   target's table for the known gaps, and an issue per gap. Steps 4 and 11 of
   `adding-a-target.md` are rewritten: gaps are table entries, and the GA bar
   is read from the table.
4. Per target: move the shape checks out of the bridge, the preflight and
   `assemble`, and fold in Java's admission pass, the KMP plan and the C
   filter.
5. The generated support page.

Step 2 protects users before the table exists. Until step 4, some shapes still
fail even with `--allow-skipped`, as the Java tuple does today.

## 4. Alternatives considered

### 4.1 Keep skipping, document `--deny-skipped`

This protects only users who know the flag. The demo shows that this repository
did not turn it on either.

### 4.2 Flip the default and stop there

This is step 2 alone, and most of the user-visible value. Fail or skip still
depends on the pass that noticed (§1.3), the docs stay hand-written, and a new
backend has nothing to declare against.

### 4.3 Warn for one release before failing

This delays the protection by a release. The failure already names the fix, and
the fix is one flag.

### 4.4 A per-shape allow list in `boltffi.toml`

```toml
[targets.kotlin]
allow_unsupported = ["closure-return"]
```

A blanket flag accepts every future gap along with the one the user looked at.
A list fails again when a new shape appears. It is the better end state, but it
needs the shape names from §2.1. It can be added after step 3 without another
break.

### 4.5 Reject whatever any target cannot render

Drift disappears. So do closure returns for Swift users, and each new backend
would shrink the language for everyone until it caught up.

## 5. Consequences

- On every target a declaration either renders or is refused by name, before
  any file is written.
- A shared API that differs per language fails CI unless someone chose that.
- **Breaking.** Crates that rely on skips fail on upgrade until they pass
  `--allow-skipped`.
- The enum grows with each gap found. A variant's test can be wider than the
  renderer's real limit, so a table may refuse something that would have
  rendered.
- The table starts from about 55 probed declarations, while the hosts have
  several hundred `unsupported("…")` sites. An unknown gap surfaces as a
  missing entry, by name, instead of a silent skip.
- ADR 0003 (#1034) plans a hook that runs the C filter before its name check.
  Once the check does the filtering, the hook is not needed. Whichever of the
  two lands second makes that change.

### Not verified

- Leaving a member out of the Binding IR ahead of the bridge has not been
  prototyped. Java's admission pass removes whole declarations, and TypeScript
  and C# drop members while rendering.
- §1 measures generation. Only the two outputs in §1.2 were compiled, and
  nothing was run.

### Out of scope

Wire-format agreement between targets (ADR 0006), generated name collisions
(ADR 0003), stream delivery (ADR 0004, #1061), and the wording of diagnostics.
