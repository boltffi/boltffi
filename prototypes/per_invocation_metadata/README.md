# Prototype: per-invocation metadata capture

A throwaway workspace, excluded from the boltffi build, that tests whether BoltFFI could discover
its FFI surface from **what the compiler actually compiled** instead of from a syn re-scan of the
source files. Motivation: `#[data]`/`#[export]` items emitted by a macro or pulled in with
`include!` are silently absent from the generated bindings while `boltffi generate` exits 0.

Read [FINDINGS.md](FINDINGS.md) for the results; the discussion lives in [RFC #665](https://github.com/boltffi/boltffi/issues/665). This README is just the map.

## The idea being tested

Each `#[data]` expansion emits two things:

1. `impl<Tag> TypeInfo<Tag> for Self`, whose `MODULE` const is `module_path!()` — the type states
   its own canonical id, at the one place that knows it.
2. A `#[used] static [u8; N]` in a link section, built entirely in const context, holding the item's
   JSON payload plus a *slot table*: for every type the item references, the pair
   `<Referenced as TypeInfo<crate::PimTag>>::MODULE` / `::NAME`.

The slot table is the trick. The macro never resolves a type path — it splices the referenced type's
tokens into a const expression and lets **rustc** resolve them. Aliases, globs, re-exports and
same-named types in different modules all come out right, because the compiler is the one doing the
name resolution.

`pim_reader` then reads the section back out of the compiled artifact and rebuilds the type graph.

Phase 2 extends the same idea to the wrapper codegen. Each `#[export]` expansion emits, adjacent
to the function, an `extern "C"` wrapper whose signature goes through
`<T as Codec<crate::PimTag>>::FfiType` — the compiler picks each type's ABI — plus a record
carrying the wrapper's link symbol (crate name + item name + a `Span`-derived hash). The
acceptance test dlopens the artifact and calls wrappers through symbol names read from their own
records.

## Crates

| crate | role |
|---|---|
| `pim_runtime` | the `TypeInfo`, `Encode` and `Codec` traits, const record framing, and the section parser |
| `pim_macros` | `#[data]`, `#[export]`, `define_record!` (a proc macro that emits a `#[data]` item), `custom_type!`, `scaffolding!` |
| `pim_dep` | a dependency crate with its own `#[data]` types, to test cross-crate references |
| `pim_toy` | cdylib + rlib exercising every scenario; `tests/ui` holds the compile-fail cases |
| `pim_reader` | section extractor, slot resolver, table printer, and the dlopen acceptance tests |

## Try it

```sh
cargo test                                          # 30 tests, incl. 4 compile-fail cases
cargo build -p pim_toy
cargo run -p pim_reader -- target/debug/libpim_toy.dylib
```
