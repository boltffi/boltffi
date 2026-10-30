# Diagnostics coverage review

The reference for improving BoltFFI's user-facing errors step by step. Update it in the same PR as any change to a case, a message, or a rule.

- **Baseline:** `main` at f0da5b12, rustc 1.97.1, macOS, 2026-10-30.
- **Harness:** 38 trybuild cases in this directory.
- **Evidence:** 217 single-file probes compiled with `cargo check`, 14 ecosystem probes (dependency crates, thiserror, feature flags, renamed crates), and 50 accepted probes followed through `cargo build` and `boltffi generate` for Swift, Kotlin, Python, C#, C, and TypeScript.

Every "today" entry below is observed compiler or generator output, not a reading of the source. Likelihood and relevance are judgement calls.

## Scoreboard

Outcome of the 217 single-file probes:

| Outcome | Probes | Meaning |
|---|---:|---|
| Clear BoltFFI or macro message | 45 | Says what is wrong in user terms |
| Native rustc error | 13 | Mixed; some good, some name generated traits |
| IR jargon | 23 | `cannot be represented in binding IR yet`, `unsupported expansion: …`, `unrecognized type` |
| Lane internals | 34 | ``cannot find macro `Point` ``, ``cannot find `Rc` in `rc` ``, `__BoltffiTag`, `wrong source kind` |
| Internal id leak | 4 | `method $slot:0::g`, `method $self::T::f` |
| Accepted | 98 | 4 controls; many legitimate, some should be rejected (see Silent acceptance) |

Of the 119 rejected probes, 61 (51%) give a message a user cannot act on without knowing BoltFFI's internals.

Span quality: 68 of 69 `BoltFFI:` errors point at the `#[export]` or `#[data]` attribute, not at the offending parameter, field, or method. In a 20-method `impl` block the user has to guess which method is wrong.

## Priority findings

### P0: memory safety, not a diagnostics gap

`&mut` to a thread-safe class as a function parameter compiles, although a `&mut self` method on the same class is rejected:

```rust
#[export]
pub struct Counter { items: Vec<u64> }

#[export]
pub fn push(c: &mut Counter) { c.items.push(1); }
```

The wrapper does an unlocked `&mut *(*handle).value.get()` (`wrapper/class.rs:233`), and the C header exposes the parameter as `const RaceCounter *`. Calling `push` from four threads corrupted the heap and crashed in `free` in 3 of 3 runs; one thread gives the correct length. No `unsafe` in user code.

`examples/demo/src/classes/ownership.rs` exports `mutate_message(&mut OwnedMessage)` on a default class, so closing this hole changes demo behaviour and is a design decision, not a message edit.

### P1: the messages most users will hit

| Mistake | Today | Should say |
|---|---|---|
| Forgot `scaffolding!()` | ``cannot find type `__BoltffiTag` in the crate root``, no hint | add `boltffi::scaffolding!();` at the crate root |
| Plain struct or trait in a signature | ``cannot find macro `Point` in this scope`` | `Point` is not declared; add `#[data]` or `#[export]` |
| `char`, `i128` | ``cannot find macro `char` ``; rustc adds "`char` is in scope, but it is a builtin type" | `char` cannot cross the boundary; use `u32` or `String` |
| `pub type Id = u64;` in a signature | ``cannot find macro `Id` `` | type aliases are not followed; spell the target type |
| `HashSet`, `VecDeque`, `Rc`, `Cow`, `PathBuf` by full path | ``cannot find `HashSet` in `collections` ``, which is false | same as above, naming the type |
| `std::io::Result<T>`, `Box<dyn Error>` | ``cannot find `Result` in `io` `` | error type must be `String`, `#[error]`, or `#[data]` |
| `type Result<T> = …` | ``unsupported source type `Result<i32>` `` | names the alias and spells the fix |
| Undeclared error type | ``cannot find macro `E` `` | add `#[error]` |
| `Arc<Point>`, `Box<i32>`, `-> Arc<Self>` | `opaque Rust pointer container cannot be represented in binding IR yet` | classes cross by handle; return `Self` |
| `[u8; 4]`, `-> !` | ``unsupported source type `unrecognized type` `` | names the type; suggests `Vec<u8>` for arrays |
| `#[data]` record, no `derive(Clone, Copy)` | Four E0277 errors naming `boltffi::__private::wire::Blittable` | blittable records need `Clone, Copy` |
| `#[data]` on an `impl` | Five E0119 conflicts on `Passable`, `Blittable` | use `#[data(impl)]` |

Rule behind most of these rows: a failed lane lookup reaches the user as rustc's missing-macro error. That one mechanism accounts for most of the 34 lane-internal probes, so replacing it is the highest-leverage step here.

### P1: internal ids in messages

`scan` prints ids before the type is known: ``BoltFFI: `method $slot:0::g` cannot use generics``, ``BoltFFI: `method $self::T::f` cannot be unsafe``, ``stream $slot:0::s must return Arc<EventSubscription<T>>``. Render the method name.

### P1: thiserror with a glob import

`use boltffi::*;` with `#[error]` and `#[derive(thiserror::Error)]` fails with three errors, including ``E0659: `error` is ambiguous``. `use boltffi::{export, data};` with `#[boltffi::error]` works. Likely for anyone with an existing error enum. Needs a fix or a documented pattern. A harness case needs `thiserror` as a dev-dependency.

## Silent acceptance

These compile. The danger is that the user finds out late, or never.

| Input | What happens | Likelihood | Relevance | Verdict |
|---|---|---|---|---|
| `#[boltffi::default("x")] a: i32` | Swift emits `a: Int32 = "x"`, which does not compile; Python emits `a: int = "x"`; Kotlin skips the function | Medium | High | Reject in macro |
| Private `fn` with `#[export]` | Exposed as `public` on every host | High | High | Reject (decided, deferred) |
| Private or `pub(crate)` method in `#[export] impl` | Dropped silently | High | Medium | Document with the row above |
| `#[export(fast)]` on a fn, `#[data(compact)]` | Ignored. `#[export(single_thread)]` on a struct is rejected, so behaviour is inconsistent | Medium | High | Reject unknown arguments |
| `#[ffi_stream]` on a free fn | Ignored; exported as a plain function | Low | Medium | Reject |
| `#[boltffi::default(1)]` on a fn | Ignored | Low | Low | Reject if cheap |
| `#[data(impl)] impl Default for Point` | Compiles, method missing from every host | Medium | Medium | Reject, as `#[export] impl Trait` is |
| `#[data] union` | Compiles, nothing exported; `generate` then says the crate has no records | Low | Low | Reject |
| `#[repr(u128)]` enum | Accepted | Very low | Low | Skip |
| `#[repr(transparent)]`, `repr(Rust)` on a record | Accepted | Low | Low | Check layout claims, then skip |
| Same `#[export] fn` name in two modules | `check` passes; `build` fails with ``symbol `boltffi_function_3st21f` is already defined`` | Medium | High | Detect in macro; trybuild cannot see link errors |
| `&mut i32` parameter | Swift `generate` fails; Python and C# skip; others pass | Medium | Medium | Host gap, not a macro error |
| Tuples, nested `Option`, `BTreeMap`, closures | `generate` exits 0 and skips per host (TS, C#, C) | Medium | Medium | Skip tables are easy to miss; consider a strict flag |

## Use-case coverage

Rating: **Good** = main mistakes locked with clear text. **Partial** = some locked, or locked with poor text. **None** = no case. Cases sum to 38. 11 of them lock lane-internal text on purpose and should change as step 3 lands.

| Area | Cases | Rating | Biggest gap |
|---|---:|---|---|
| Scaffolding | 3 | Partial | Two cases lock `__BoltffiTag`; no case for twice, or in a module |
| Undeclared types | 5 | Partial | All lock the macro-lookup message |
| Builtin names | 4 | Good | Low likelihood but cheap |
| Custom type naming | 3 | Partial | Remote-path cases lock a false message and a broken rustc suggestion |
| Custom type declaration | 0 | None | Missing keys print two errors; wrong closure gives bare E0308 |
| Same-name declarations | 2 | Partial | Same-named classes and functions across modules |
| Classes | 6 | Good | Missing `&mut Class`, `Arc<Self>` constructor, `impl &T` |
| Records | 5 | Partial | Missing derive, `#[data]` on impl, lifetime, recursive box |
| Enums | 0 | None | Empty enum, generic, recursive, non-literal discriminant |
| Errors | 0 | None | Undeclared error type, thiserror, `io::Result`, `Box<dyn Error>` |
| Function items | 2 | Partial | `async`, `extern`, `self`, nested in fn or impl |
| Signatures | 4 | Partial | Primitives, std containers, aliases, arrays, fn pointers |
| Callback traits | 1 | Partial | Receivers, generics, assoc items, supertraits, `Self`, `&dyn T` |
| Closures | 0 | None | `fn` pointers, `Option<Box<dyn Fn>>` on traits, record fields |
| Streams | 0 | None | Missing `item`, bad mode, wrong return |
| Constants | 0 | None | Arrays, overflow, anonymous |
| Default values | 0 | None | Wrong type, non-literal |
| Attribute placement | 1 | Partial | `#[data]` on fn or trait prints `wrong source kind` |
| Interned strings | 1 | Good | Low likelihood |
| `cfg` | 1 | Partial | Gated type used by an ungated function |
| Multi-crate | 0 | None | Undeclared type from a dependency |
| Feature flags | 0 | None | `chrono` without the feature gives ``cannot find macro `DateTime` `` |
| Renamed `boltffi` | 0 | None | ``cannot find `boltffi` in the list of imported crates`` on every macro |

## Corner cases

| Case | Today | Likelihood | Relevance | Verdict |
|---|---|---|---|---|
| `-> Option<&str>` | ``unsupported source type `&str` ``, though `&str` works elsewhere | High | Medium | Improve text, add case |
| `self` param on free fn | `free function cannot have a receiver` plus rustc | Medium | Low | Fine, add case |
| `_: i32` parameter | `parameter pattern is not a plain name` | Medium | Medium | Clear enough, add case |
| Trait with `fn f();` or `&mut self` | `callback method receiver cannot be represented…` | High | Medium | Say which receivers are allowed |
| Trait `-> Self` | `Self in callback trait method…` | Low | Low | Reword with the rest |
| `&dyn Trait` parameter | `borrowed callback parameter cannot be…` | Medium | Medium | Suggest `Box<dyn T>` or `Arc<dyn T>` |
| `impl Fn` on a trait method | E0310 | Low | Low | Not object-safe in spirit; skip |
| `Option<Box<dyn Fn>>` on a trait method | `unsupported expansion: outgoing closure parameter form` | Low | Medium | Reword |
| Closure on a callback trait method | Compiles after #1030; Swift and Dart `generate` fail, most hosts skip | Medium | Medium | Host gap; not for this suite |
| `fn(i32) -> i32` parameter | `unsupported expansion: function-pointer closure parameter` | Medium | Medium | Suggest `impl Fn` |
| `-> Vec<Self>` | `unsupported expansion: codec node` | Medium | Medium | Reword; the wording is purely internal |
| `impl &Counter` | Three errors | Very low | Low | Skip |
| `self: Arc<Self>` | `owned class receiver…` | Medium | Medium | Reword, keep case |
| `#[export]` in a fn body | `cannot find function inner in module super` | Low | Medium | Reject with a real message |
| `#[export]` on a method in a plain `impl` | Three errors about `use` and `mod` in impls | Medium | Medium | Suggest `#[export]` on the impl |
| Empty `#[data] enum` | E0084 and E0004 on generated code | Low | Low | Reject in one line |
| Recursive `Box<T>` field | `opaque Rust pointer container…` | Medium | Medium | Suggest `Vec` or `Option` handling |
| Same `#[export] struct` in two modules | E0428 on `__boltffi_declared_C` | Low | Medium | Reuse the record duplicate rule |
| Discriminant out of range | Native lint, clear | Low | Low | Fine, skip |
| `Rc` field in thread-safe class | Native E0277, clear | Medium | High | Fine, add case |
| `HashMap<Point, _>` | Native E0277 on `Eq`, `Hash`, clear | Low | Low | Fine, skip |
| `boltffi` renamed in Cargo.toml | E0433 per macro | Low | Low | Document it |
| Interned pool syntax error | Prefixed `invalid custom type declaration` | Very low | Low | Fix prefix when nearby |

Confirmed working, no action: `use dep::T`, full paths, renamed dependencies, `self::` and `crate::` paths, `use a::P as Q`, `use a::*`, declaration after use, macro-generated `#[export]`, `cfg_attr(…, export)`, gated fields, variants, and methods, `Duration`, `SystemTime`, `Uuid`, and `Url` at default features.

## Roadmap

Each step is one PR. Every message change re-records its cases, so the diff shows the improvement.

1. **Land this harness** with the bad messages recorded as they are.
2. **Decide `&mut Class`.** P0. Deferred by Henrik on 2026-10-30.
3. **Replace the macro-lookup failure** with a BoltFFI message. Resolves most P1 rows.
4. **Add a `scaffolding!()` hint.**
5. **Remove internal ids** from `scan` messages.
6. **Reword `binding IR` and `unsupported expansion` messages** for user reach; lock with cases in the empty areas above (enums, errors, streams, constants, default values, closures).
7. **Point spans at the offending item.** Carry spans through scan, starting with parameters.
8. **Reject silent acceptance:** unknown arguments, default type mismatch, misplaced `#[ffi_stream]`, `#[data(impl)]` on trait impls.
9. **Reject `#[export]` on private functions.** Decided 2026-10-30, scheduled later; breaking change.
10. **Thiserror compatibility.**
11. **Link-stage duplicates.** Detect same-named functions in the macro, or add a build-stage harness.

Target: no recorded `.stderr` contains lane-internal text, internal ids, or `binding IR`, with the empty areas above covered by about 40 to 50 new cases added alongside these steps. The probes were a one-off survey and are not meant to become cases.

## Limits of this review

- `cargo check` on macOS only for this pass; the recorded `.stderr` files have not been rerun on Linux since the port.
- `cfg(wasm32)` paths were probed only for callback closures.
- Generated Swift, Kotlin, and Python were inspected as text and not compiled.
- Probe sources are not in the repository. Add a case here to keep one permanently.
- trybuild runs `check`, so it cannot lock link or `generate` failures.
