# Diagnostics coverage

This file tracks how good BoltFFI's compile errors are and which ones to fix next. Update it in any PR that changes a case, a message, or a rule.

The findings come from `main` at f0da5b12 on rustc 1.97.1. Each "Today" entry is observed output. Likelihood and relevance are judgement calls.

## How the suite works

Each case in `diagnostics/<rule>/<what_the_user_did>.rs` must fail to compile, and its `.stderr` file records the output. There are 111 of these, and many record bad messages on purpose, so a fix shows up as a `.stderr` diff.

The 8 cases in `tests/accepted/` compile today and should not. When a rule lands, move its case to `diagnostics/` and record the error.

Re-record with `TRYBUILD=overwrite cargo test -p boltffi_macros --test diagnostics`, and re-record after a toolchain bump.

Avoid cases where rustc quotes the standard library. CI has no `rust-src`, so the quoted snippet renders differently there.

The suite runs `cargo check`, so it can't record link errors, `generate` failures, or anything that depends on Cargo features or dependency names.

## Current state

We compiled 217 small crates, one mistake or feature each:

| Outcome | Crates | Example |
|---|---:|---|
| Clear message | 45 | ``export can only be applied to const, enum, fn, impl, struct, or trait`` |
| rustc error | 13 | `` `Rc<u32>` cannot be sent between threads safely `` |
| Compiler internals | 23 | `cannot be represented in binding IR yet` |
| Failed macro lookup | 34 | ``cannot find macro `Point` in this scope`` |
| Internal id | 4 | ``method $slot:0::g cannot use generics`` |
| Accepted | 98 | Most are valid code, the rest are in "Accepted mistakes" |

Of the 119 rejected crates, 61 print a message that users can't act on without knowing how BoltFFI works.

68 of the 69 `BoltFFI:` errors point at the `#[export]` or `#[data]` attribute instead of the parameter, field, or method at fault. In a large `impl` block, users have to guess which method is wrong.

## Most common bad messages

Most of these rows have one cause. When a type has no BoltFFI declaration, the lookup fails and rustc reports a missing macro. Replacing that error is the most valuable single fix.

| Mistake | Today | Should say |
|---|---|---|
| Forgot `scaffolding!()` | ``cannot find type `__BoltffiTag` in the crate root`` | Add `boltffi::scaffolding!();` at the crate root |
| Undeclared struct or trait in a signature | ``cannot find macro `Point` in this scope`` | `Point` is not declared, add `#[data]` or `#[export]` |
| `char` or `i128` | ``cannot find macro `char` in this scope`` | `char` can't cross the boundary, use `u32` or `String` |
| `pub type Id = u64;` in a signature | ``cannot find macro `Id` in this scope`` | BoltFFI can't follow this alias, write `u64` |
| `std::collections::HashSet` or another unsupported type by full path | ``cannot find `HashSet` in `collections` ``, which is false | `HashSet` can't cross the boundary, use `Vec` |
| `std::io::Result<T>` | ``cannot find `Result` in `io` `` | The error type must be `String` or a declared type |
| Undeclared error type | ``cannot find macro `E` in this scope`` | `E` is not declared, add `#[error]` |
| `type Result<T> = ...` | ``unsupported source type `Result<i32>` `` | BoltFFI can't follow this alias |
| `Arc<Point>`, `Box<i32>`, `-> Arc<Self>` | `opaque Rust pointer container cannot be represented in binding IR yet` | Name the type, and suggest `Self` for constructors |
| `[u8; 4]` or `-> !` | ``unsupported source type `unrecognized type` `` | Name the type, and suggest `Vec<u8>` for arrays |
| `#[data]` record that derives `Clone` but not `Copy` | Several E0277 errors naming `Blittable` | This record needs `#[derive(Clone, Copy)]` |
| `#[data]` on an `impl` | Several E0119 errors naming `Passable` and `Blittable` | Use `#[data(impl)]` |
| Generic method | ``method $slot:0::g cannot use generics`` | `g` can't be generic |

`#[error]` from `use boltffi::*;` also conflicts with `#[derive(thiserror::Error)]`, and rustc reports ``E0659: `error` is ambiguous``. Importing names and writing `#[boltffi::error]` works. No case covers this yet because it needs `thiserror` as a dev-dependency.

## Accepted mistakes

These compile, so users find out late or never.

| Input | What happens | Likelihood | Relevance | Plan |
|---|---|---|---|---|
| Private `fn` with `#[export]` | Public on every host | High | High | Reject, scheduled for later |
| Same `#[export] fn` name in two modules | `check` passes, `build` fails with ``symbol ... is already defined`` | Medium | High | Detect it in the macro |
| `#[boltffi::default("x")] a: i32` | Swift gets `a: Int32 = "x"`, Python gets `a: int = "x"`, Kotlin skips the function | Medium | High | Reject |
| `#[export(fast)]`, `#[data(compact)]` | Ignored, although `#[export(single_thread)]` on a struct is rejected | Medium | High | Reject unknown arguments |
| `&mut` class parameter on a thread-safe class | Accepted, although `&mut self` on the same class is rejected | Medium | High | Apply the same rule, scheduled for later |
| Private method in `#[export] impl` | Left out of the bindings with no message | High | Medium | Document it |
| `#[data(impl)] impl Default for Point` | Method missing on every host | Medium | Medium | Reject |
| `#[ffi_stream]` on a free function | Ignored | Low | Medium | Reject |
| `#[data] union` | Nothing exported, then `generate` reports no records | Low | Low | Reject |
| `#[boltffi::default(1)]` on a function | Ignored | Low | Low | Reject if cheap |

## Other messages to improve

A case records each of these.

| Input | Today | Likelihood | Relevance | Fix |
|---|---|---|---|---|
| Trait with `fn f();` or `&mut self` | `callback method receiver cannot be represented...` | High | Medium | List the allowed receivers |
| `Option<&str>` | ``unsupported source type `&str` ``, although `&str` works elsewhere | High | Medium | Name `Option<&str>` and suggest `Option<String>` |
| `&dyn Trait` parameter | `borrowed callback parameter cannot be...` | Medium | Medium | Suggest `Box<dyn T>` or `Arc<dyn T>` |
| `fn(i32) -> i32` parameter | `unsupported expansion: function-pointer closure parameter` | Medium | Medium | Suggest `impl Fn` |
| `-> Vec<Self>` | `unsupported expansion: codec node` | Medium | Medium | Name the return type |
| `self: Arc<Self>` | `owned class receiver...` | Medium | Medium | List the allowed receivers |
| `#[export]` on a method in a plain `impl` | Three errors about `use` and `mod` in impls | Medium | Medium | Suggest `#[export]` on the `impl` |
| Recursive `Box<T>` field | `opaque Rust pointer container...` | Medium | Medium | Say that the record can't contain itself by `Box` |
| `Option<Box<dyn Fn>>` on a trait method | `unsupported expansion: outgoing closure parameter form` | Low | Medium | Say it is not supported |
| `#[export]` inside a function | `cannot find function inner in module super` | Low | Medium | Say it must be at module level |
| Same `#[export] struct` in two modules | E0428 on `__boltffi_declared_C` | Low | Medium | Report it like same-named records |
| Empty `#[data] enum` | E0084 and E0004 on generated code | Low | Low | Print one error |
