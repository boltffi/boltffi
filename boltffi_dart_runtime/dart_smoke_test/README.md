# `boltffi_dart_runtime` smoke test

Minimal dual-path dispatcher (`reference_dispatcher/`) plus a Dart script
that exercises owner-thread and foreign-thread calls against a real VM.

```bash
cd reference_dispatcher && cargo build --release && cd ..
dart run main_test.dart
```
