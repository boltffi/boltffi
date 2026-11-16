# Diagnostics coverage

This file tracks how good BoltFFI's compile errors are and which ones to fix next. Update it in any PR that changes a case, a message, or a rule.

The findings come from `main` at f0da5b12 on rustc 1.97.1. Each "Today" entry is observed output. Likelihood and relevance are judgement calls.

## How the suite works

Each case in `diagnostics/<rule>/<what_the_user_did>.rs` must fail to compile, and its `.stderr` file records the output. There are 125 of these, and many record bad messages on purpose, so a fix shows up as a `.stderr` diff.

The 2 cases in `tests/accepted/` compile today and should not. When a rule lands, move its case to `diagnostics/` and record the error.

Re-record with `TRYBUILD=overwrite cargo test -p boltffi_diagnostics --test diagnostics`, and re-record after a toolchain bump.

Avoid cases where rustc quotes the standard library. CI has no `rust-src`, so the quoted snippet renders differently there.

The suite runs `cargo check`, so it can't record link errors, `generate` failures, or anything that depends on Cargo features or dependency names.

## Current state

We compiled 217 small crates, one mistake or feature each:

| Outcome | Crates | Example |
|---|---:|---|
| Clear message | 68 | ``export can only be applied to const, enum, fn, impl, struct, or trait`` |
| rustc error | 13 | `` `Rc<u32>` cannot be sent between threads safely `` |
| Failed macro lookup | 34 | ``cannot find macro `Point` in this scope`` |
| Internal id | 4 | ``method $slot:0::g cannot use generics`` |
| Accepted | 98 | Most are valid code, the rest are in "Accepted mistakes" |

Of the 119 rejected crates, 38 print a message that users can't act on without knowing how BoltFFI works.

Errors point at the parameter, field, or method at fault. 7 of the suite's 76 `BoltFFI:` errors point at an attribute, and in 6 of those the attribute is the mistake. The exception is a constant whose value doesn't fit its type, which points at `#[export]`.

## Most common bad messages

Several of these rows have one cause. When a type has no BoltFFI declaration, the lookup fails and rustc reports a missing macro. For a struct or enum, a second error now says ``` `Point` is not declared for BoltFFI ``` at the same type. rustc's own error can't be suppressed, and traits and aliases still get only rustc's.

| Mistake | Today | Should say |
|---|---|---|
| Forgot `scaffolding!()` | ``cannot find type `BoltffiScaffolding` in the crate root``. A missing `crate::` path can't carry a custom message, so only the name hints at the fix | Add `boltffi::scaffolding!();` at the crate root |
| Undeclared trait in a signature | ``cannot find macro `Listener` in this scope`` | `Listener` is not declared, add `#[export]` |
| `pub type Id = u64;` in a signature | ``cannot find macro `Id` in this scope`` | BoltFFI can't follow this alias, write `u64` |
| `type Result<T> = ...` | ``unsupported source type `Result<i32>` `` | BoltFFI can't follow this alias |
| `#[data]` record that derives `Clone` but not `Copy` | Several E0277 errors naming `Blittable` | This record needs `#[derive(Clone, Copy)]` |
| `#[data]` on an `impl` | Several E0119 errors naming `Passable` and `Blittable` | Use `#[data(impl)]` |

`#[error]` from `use boltffi::*;` also conflicts with `#[derive(thiserror::Error)]`, and rustc reports ``E0659: `error` is ambiguous``. Importing names and writing `#[boltffi::error]` works. No case covers this yet because it needs `thiserror` as a dev-dependency.

## Accepted mistakes

These compile, so users find out late or never.

| Input | What happens | Likelihood | Relevance | Plan |
|---|---|---|---|---|
| Private `fn` with `#[export]` | Public on every host | High | High | Reject, scheduled for later |
| Same `#[export] fn` name in two modules | `check` passes, `build` fails with ``symbol ... is already defined`` | Medium | High | Detect it in the macro |
| `&mut` class parameter on a thread-safe class | Accepted, although `&mut self` on the same class is rejected | Medium | High | Apply the same rule, scheduled for later |
| Private method in `#[export] impl` | Left out of the bindings with no message | High | Medium | Document it |

## Other messages to improve

A case records each of these.

| Input | Today | Likelihood | Relevance | Fix |
|---|---|---|---|---|
| `Option<&str>` | ``BoltFFI: `&str` cannot cross the boundary; use the owned type `String` ``, although `&str` works elsewhere | High | Medium | Name `Option<&str>` and suggest `Option<String>` |
| `#[export]` on a method in a plain `impl` | Three errors about `use` and `mod` in impls | Medium | Medium | Suggest `#[export]` on the `impl` |
| `#[export]` inside a function | `cannot find function inner in module super` | Low | Medium | Say it must be at module level |
| Same `#[export] struct` in two modules | E0428 on `__boltffi_declared_C` | Low | Medium | Report it like same-named records |
| Empty `#[data] enum` | E0084 and E0004 on generated code | Low | Low | Print one error |
