# ADR: Declaration Lanes. Per-Invocation Expansion Without a Source Scan

## Status
Proposed

## Authors
- [Henrik](https://github.com/akesson)

## Date
2026-09-26

---

## 1. Background and Problem

### 1.1 Two halves that must agree

BoltFFI produces two things from one crate. The proc macros write `extern "C"` wrappers
while rustc compiles the crate, and bindgen writes the foreign bindings afterwards. Both
must reach the same answer for every symbol and every value:

```rust
// src/shapes.rs
#[data] pub struct Point { pub x: f64, pub y: f64 }

// src/lib.rs
#[export] pub fn shift(p: Point, dx: f64) -> Point { … }
```

```rust
// Point direct: all fields fixed-width primitives, portable layout
extern "C" fn boltffi_function_…_shift(p: Point, dx: f64) -> Point
// Point encoded, for example with a String field
extern "C" fn boltffi_function_…_shift(p_ptr: *const u8, p_len: usize, dx: f64) -> FfiBuf
```

The `#[export]` on `shift` receives only the tokens of `shift`. The shape of its wrapper
depends on `Point`, which lives in another file.

### 1.2 What `main` does

The first BoltFFI invocation in a crate reads `src/` from disk with `boltffi_scan`, lowers
the whole crate, and emits every wrapper into one `mod __boltffi_expansion`. Later
invocations only strip their attributes. Bindgen runs a second compile gated by
`BOLTFFI_BINDING_METADATA`, in which the macro serializes its scanned contract into the
artifact.

There is one decider, so the halves agree. But the scan approximates what rustc compiles:

| Scan limitation | Effect | Tracked as |
|---|---|---|
| Items from `macro_rules!` or `include!` are never seen | Missing from bindings, and `generate` exits 0 | #885, RFC #665 |
| `cfg` approximated from env and scraped arguments | Wrong or missing members | #630, #618 |
| Name resolution reimplemented | Heuristics can pick the wrong type | — |
| Registry crate types, and dependency types named by full path | "unsupported source type" | — |
| Syntax the scanner cannot parse, anywhere in the crate | Scan fails | #80 |

### 1.3 Records, and the objection to `FfiCross`

RFC #665 moved discovery onto the compiler. Each invocation writes a source record into a
link section of the artifact, and rustc resolves the types it names through trait
constants (#725, #731, #733). Records give bindgen an exact surface.

Wrappers were harder. #734 made each invocation expand itself through a `FfiCross` trait:
primitives crossed as themselves, everything else as one owned buffer. On #770, engali94
objected:

> The macro and the backend must consume the same classified contract. Neither side should
> independently decide how a Rust type crosses the boundary.

Under `FfiCross`, direct records became buffers and C-style enums stopped being integers.
The macro had become a second, simpler decider. The integration branch fell back to
discovery only: records for bindgen and the scan for wrappers, with a guard (#926) for
where the two disagree.

## 2. Goals

- **One decider.** The wrapper and the bindings consume one classified contract, produced
  by the same code.
- **ABI kept.** Today's C signatures on every target: direct records by value, encoded
  values as pointer and length, spans, split `Result` channels.
- **No scan.** Nothing reads source files, so everything rustc compiles is seen, and
  nothing else is.
- **One build.** Plain `cargo build` produces the wrappers, and `generate` needs no
  second compile.

## 3. Considered Alternatives

| | Approach | One decider | ABI kept | No scan | Why not |
|---|---|---|---|---|---|
| A | Whole-crate scan (`main`) | yes | yes | no | The limitations in §1.2 |
| B | Discovery only: records for bindgen, scan for wrappers | guarded | yes | no | Two pipelines kept equal by hand, with no user gain |
| C | `FfiCross` v1 (#734, #770) | no | no | yes | A second, declassified ABI |
| D | Classified `FfiCross`: the data site picks how it crosses | yes | no | yes | Arity. One Rust parameter can be two C parameters, and passing `FfiSpan` by value changes the ABI on Windows x64, wasm32, SysV and AArch64 (measured on 10 targets) |
| E | Build once for records, then again with macros reading them | yes | yes | yes | Two compiles, and `cargo build` alone has no wrappers |
| F | **Declaration lanes** | yes | yes | yes | Accepted |

Also ruled out: workarounds for D's arity (a zero-sized second slot fails on Windows x64;
normalizing the ABI in the binding IR changes every backend), a generated scaffolding file
(its input needs A or E), and rustdoc JSON (nightly only).

## 4. Decision

Every type BoltFFI declares also emits a hidden `macro_rules!` under the type's own name,
called its lane. The lane carries the type's own source fragment. An invocation calls the
lanes of the types its item names, then runs bindgen's aggregation, lowering and expander
over its own fragment and theirs. It lowers and renders only its own item; the named
types only answer what they are and how they are laid out.

```rust
// emitted by #[data] Point
#[macro_export]
macro_rules! __boltffi_lane_geo_Point_0 {
    ([$($callback:tt)*] { $($state:tt)* }) => {
        $($callback)*! { $($state)* r#"{"id":"geo::Point","entries":[…]}"# }
    };
}
pub use __boltffi_lane_geo_Point_0 as Point;
```

Macros and types live in separate namespaces, so any path that names `Point` also names
its lane, through imports, renames, globs, re-exports and other crates. The compiler does
the resolution the scan used to approximate. A `type` alias names only the type, so a
signature naming an alias fails with `cannot find macro`, as it fails on `main`;
`use shapes::Point as Coordinate;` renames both. Records stay the only input to bindgen, and
the envelope path is deleted.

### 4.1 Worked example

```rust
pub mod shapes {
    #[data] pub struct Point { pub x: f64, pub y: f64 }
    #[data] pub struct Line { pub a: Point, pub b: Point }
}

use shapes::Line;

#[export] pub fn length(line: Line) -> f64 { … }
```

Each macro describes its own item as a source fragment. The fragment leaves a hole,
`$slot:0`, wherever the item names another type, because the macro sees only the word
`Point`.

Every declared type defines its lane at once, from its own fragment, so `Line`'s lane
exists before anything resolves `Point`. Then `#[data] Line` expands in three steps:

```rust
// 1. The attribute emits the struct and its lane, and calls Point's lane.
macro_rules! __boltffi_lane_geo_Line_1 { … }   // carries Line's fragment only
pub use __boltffi_lane_geo_Line_1 as Line;
Point! { [lane_resume] { data {} { pub struct Line { … } } [] [] } }

// 2. The lane is a macro_rules!: rustc appends Point's fragment and calls back.
lane_resume! { data {} { pub struct Line { … } } [] [] r#"{"id":"geo::Point",…}"# }

// 3. lane_resume lowers Line with bindgen's functions, reading Point only for its kind.
//    A record field makes Line encoded.
impl WireEncode for Line { … }
```

`#[export] length` takes one hop, through `Line!`, and lowers `length` against `Line`'s
fields:

```rust
pub unsafe extern "C" fn boltffi_function_geo_length(line_ptr: *const u8, line_len: usize) -> f64
```

After the build, bindgen reads the three records from the artifact and runs the same
lowering over the same fragments, so its foreign `length` encodes `Line` into a pointer and
a length. A signature naming several types calls their lanes one after another, carrying
the pending ones in the state.

One level is enough because lowering an item reads only the own fields, repr and variants
of the types it names, and classes and callbacks only by id. Since no lane waits on
another, types may refer to each other: `Folder { files: Vec<File> }` beside
`File { subfolders: Vec<Folder> }` builds, and so do recursive records.

### 4.2 Decisions within the design

| | Decision | Reason |
|---|---|---|
| D1 | Lanes carry fragments, not a classification keyword | The invocation runs bindgen's own lowering, so there is exactly one classifier |
| D2 | Lower per invocation, not pre-lowered summaries | Summaries are a second representation to keep in sync. Revisit if compile time bites a real crate |
| D3 | Ids are crate and name; symbols drop the module (`boltffi_function_demo_add_i32`) | A proc macro cannot know its module path. The foreign namespace is flat by name, so same-named types or exports in one crate fail to build |
| D4 | `cfg` is evaluated through a derive probe | The only exact option, and there is no scan left to fall back to |
| D5 | `custom_type!` types are written by their declared name, which aliases the remote type | A signature naming `DateTime<Utc>` has no lane, and stable Rust cannot test whether a macro exists |
| D6 | Types named `Duration`, `SystemTime`, `Uuid` or `Url` are rejected at the declaration | Signatures spelling those names cross as the builtin |
| D7 | Capture is always on; bindgen reads records only | A fallback is a second pipeline to keep equivalent |
| D8 | A class's `#[export] impl` sits in the struct's module | The class lane is defined beside the impl and must resolve wherever the struct does |
| D9 | Lanes carry one level: the type's own fragment, with its references unresolved | Lowering never reads past a named type's own shape, so this is exact, lets types refer to each other, and keeps each use linear |
| D10 | Bindings cover the root crate and its path dependencies in full; registry and git crates only where they are reached | Matches what `main` binds, and extends it to registry types a signature uses. Two crates binding the same name are refused |
| D11 | `generate` checks every symbol the bindings call against the built library | A dependency's wrappers live in that crate, so a crate the library never uses is not linked. The error lists the symbols and names the fix, `use <crate> as _;` |

## 5. Implementation Status

The design is implemented in full on
[`spike/per-invocation-lanes`](https://github.com/boltffi/boltffi/tree/spike/per-invocation-lanes),
on top of the integration branch, to check that it holds before asking for this decision.
[Per-invocation expansion](https://github.com/boltffi/boltffi/blob/spike/per-invocation-lanes/docs/contributors/per-invocation-expansion.md)
walks one crate through every expansion there.

- Every annotation expands per invocation. The macros read no source files, and bindgen
  reads records only.
- The workspace tests pass (2,645), including fixtures for mutually referring types
  and for linked, unlinked and colliding dependencies.
- The demo suite passes on Python, Swift, Kotlin, Java, C#, wasm and Dart, and CI passes on
  Linux, macOS and Windows, including Android and C# packaging.
- A proof of concept of the lane technique produced `main`'s exact C signatures on 10
  targets, across crates. The full branch was compared with `main` by hand on a sample
  crate: the wrappers match apart from symbol names (D3).

## 6. Consequences

### Positive

- Wrappers and bindings share one classified contract, meeting the condition on #770.
- C signatures are unchanged; the same lowering produces them.
- Items from `macro_rules!` and `include!` are declared and exported (#885), `cfg` is exact
  (#630), and names resolve through rustc, including dependency types by any path.
- Types may refer to each other and to themselves.
- `generate` needs one ordinary build. The scan leaves the macros, and the envelope leaves
  bindgen: about 6,400 lines removed against 5,200 added, relative to discovery only.

### Negative and costs

- **Breaking.** Symbols drop module segments (D3), `custom_type!` spelling changes (D5),
  class impls sit in the struct's module (D8), and builtin names are refused (D6). A
  `macro_rules!` named like a declared type collides with its lane.
- **Compile time.** Against `main`, the demo builds 22% slower clean in debug, 15% faster
  in release, and 19% slower after an edit. Synthetic crates of 400 flat records or a
  200-deep record chain build 50% slower. Pre-lowered summaries (D2) are the next lever.
- **Dependencies.** A path dependency the library never uses is not linked, so `generate`
  fails until the root names it (D11). `custom_type!` in a dependency does not compile
  yet: its conversions are keyed to the declaring crate's tag. It fails on `main` too.
- **Binary size.** Records grow the demo's dylib from 1.74 to 2.23 MB. Nothing reads
  them at runtime, so `boltffi pack` can drop them.
- **Diagnostics.** Misuse surfaces as `cannot find macro` and needs friendlier errors.

### Not yet verified

A large real-world crate, and Android on a device. rust-analyzer expands the lanes; editor
features on top of that are untested.

### Out of scope for this ADR

Removing `boltffi_scan`: the test fixture's build script and capture's item parsing still
use it.
