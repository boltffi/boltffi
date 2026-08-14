# `boltffi_dart_runtime` smoke test

This is a reference integration, not part of the crate's build: a minimal
"generated dispatcher" (`reference_dispatcher/`, a tiny Rust crate depending
on `boltffi_dart_runtime` the same way real generated code eventually will)
plus a Dart script that exercises it against a real Dart VM.

It exists to prove the *real*, final crate -- not the throwaway proof of
concept this design started as -- actually works end-to-end from Dart, and
to serve as a concrete usage example for whoever implements the real
`boltffi_backend`/`boltffi_macros` codegen integration. See the crate's own
`src/lib.rs` doc comment and its unit tests for the detailed design
rationale and edge-case coverage (teardown races, double-destroy, throwing
callbacks, etc.) -- this smoke test only re-confirms the same properties are
externally observable from Dart through a real compiled artifact.

Run with:

```bash
cd reference_dispatcher && cargo build --release && cd ..
dart run main_test.dart
```
