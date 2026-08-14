// Smoke test for the REAL `boltffi_dart_runtime` crate (not the throwaway
// POC), via `reference_dispatcher/` -- a minimal stand-in for what real
// generated codegen will eventually produce per synchronous callback
// method. Re-confirms the same properties the crate's own Rust unit tests
// already cover (see boltffi_dart_runtime/src/lib.rs), but observed
// externally through a real compiled artifact and a real Dart VM, the way
// an actual consumer would experience them.
//
// Run with (from this directory):
//   cd reference_dispatcher && cargo build --release && cd ..
//   dart run main_test.dart

import 'dart:async';
import 'dart:ffi' as ffi;
import 'dart:io';

import 'package:ffi/ffi.dart' as pkg_ffi;

int handler(int value) => value * 2;

const _candidates = [
  'reference_dispatcher/target/release/reference_dispatcher.dll',
  'reference_dispatcher/target/debug/reference_dispatcher.dll',
  'reference_dispatcher/target/release/libreference_dispatcher.so',
  'reference_dispatcher/target/debug/libreference_dispatcher.so',
  'reference_dispatcher/target/release/libreference_dispatcher.dylib',
  'reference_dispatcher/target/debug/libreference_dispatcher.dylib',
];

ffi.DynamicLibrary _open() {
  for (final path in _candidates) {
    if (File(path).existsSync()) return ffi.DynamicLibrary.open(path);
  }
  throw StateError(
    'reference_dispatcher not built -- run `cargo build --release` in '
    'reference_dispatcher/ first',
  );
}

int failures = 0;
void check(String name, bool pass) {
  // ignore: avoid_print
  print('${pass ? 'PASS' : 'FAIL'}: $name');
  if (!pass) failures++;
}

Future<void> main() async {
  final lib = _open();

  final createInstance = lib.lookupFunction<ffi.Size Function(), int Function()>(
    'boltffi_dart_runtime_create_instance',
  );
  final destroyInstance = lib.lookupFunction<ffi.Void Function(ffi.Size), void Function(int)>(
    'boltffi_dart_runtime_destroy_instance',
  );
  final forgetInstance = lib.lookupFunction<ffi.Void Function(ffi.Size), void Function(int)>(
    'boltffi_dart_runtime_forget_instance',
  );
  final outstandingCount =
      lib.lookupFunction<ffi.Int64 Function(ffi.Size), int Function(int)>(
    'boltffi_dart_runtime_instance_outstanding_count',
  );
  final setFastPath = lib.lookupFunction<
    ffi.Void Function(ffi.Pointer<ffi.NativeFunction<ffi.Int64 Function(ffi.Int64, ffi.Pointer<ffi.Int64>)>>),
    void Function(ffi.Pointer<ffi.NativeFunction<ffi.Int64 Function(ffi.Int64, ffi.Pointer<ffi.Int64>)>>)
  >('reference_set_fast_path');
  final setListener = lib.lookupFunction<
    ffi.Void Function(ffi.Pointer<ffi.NativeFunction<ffi.Void Function(ffi.Int64, ffi.Pointer<ffi.Void>)>>),
    void Function(ffi.Pointer<ffi.NativeFunction<ffi.Void Function(ffi.Int64, ffi.Pointer<ffi.Void>)>>)
  >('reference_set_listener');
  final writeResult =
      lib.lookupFunction<ffi.Void Function(ffi.Pointer<ffi.Void>, ffi.Int64),
          void Function(ffi.Pointer<ffi.Void>, int)>('reference_write_result');
  final dispatchCall = lib.lookupFunction<
    ffi.Int64 Function(ffi.Size, ffi.Int64, ffi.Pointer<ffi.Int64>),
    int Function(int, int, ffi.Pointer<ffi.Int64>)
  >('reference_dispatch_call');

  final handle = createInstance();

  int fastPathBody(int value, ffi.Pointer<ffi.Int64> outStatus) {
    try {
      final result = handler(value);
      outStatus.value = 0; // Ok
      return result;
    } catch (_) {
      outStatus.value = 1; // Error
      return 0;
    }
  }

  final fastPathCallable =
      ffi.NativeCallable<ffi.Int64 Function(ffi.Int64, ffi.Pointer<ffi.Int64>)>.isolateLocal(
    fastPathBody,
    exceptionalReturn: 0,
  );
  setFastPath(fastPathCallable.nativeFunction);

  void listenerBody(int value, ffi.Pointer<ffi.Void> gatePtr) {
    final result = handler(value);
    writeResult(gatePtr, result);
    lib.lookupFunction<ffi.Void Function(ffi.Pointer<ffi.Void>), void Function(ffi.Pointer<ffi.Void>)>(
      'signal_gate_ok',
    )(gatePtr);
  }

  final listenerCallable = ffi.NativeCallable<
    ffi.Void Function(ffi.Int64, ffi.Pointer<ffi.Void>)
  >.listener(listenerBody);
  setListener(listenerCallable.nativeFunction);

  // --- same-thread fast path -----------------------------------------------
  final outStatus = pkg_ffi.calloc<ffi.Int64>();
  final sameThreadResult = dispatchCall(handle, 21, outStatus);
  check(
    'same-thread call returns correct result through the real crate',
    outStatus.value == 0 && sameThreadResult == 42,
  );

  // --- genuinely foreign OS thread, via the real NativeCallable.listener ---
  // The point isn't just the gate/instance logic (already covered
  // exhaustively by boltffi_dart_runtime's own Rust unit tests) -- it's
  // proving the real, exported `NativeCallable.listener` dispatch mechanism
  // actually works from a real foreign thread against this real artifact.
  final startCrossThread = lib.lookupFunction<
    ffi.Void Function(ffi.Size, ffi.Int64, ffi.Int64,
        ffi.Pointer<ffi.NativeFunction<ffi.Void Function(ffi.Int64, ffi.Int64, ffi.Int64)>>),
    void Function(int, int, int,
        ffi.Pointer<ffi.NativeFunction<ffi.Void Function(ffi.Int64, ffi.Int64, ffi.Int64)>>)
  >('reference_start_cross_thread_call');

  final pending = <int, Completer<({int status, int result})>>{};
  void testDoneBody(int id, int status, int result) {
    pending.remove(id)?.complete((status: status, result: result));
  }

  final testDoneCallable =
      ffi.NativeCallable<ffi.Void Function(ffi.Int64, ffi.Int64, ffi.Int64)>.listener(
    testDoneBody,
  );

  Future<({int status, int result})> runCrossThread(int value, int id) {
    final completer = Completer<({int status, int result})>();
    pending[id] = completer;
    startCrossThread(handle, value, id, testDoneCallable.nativeFunction);
    return completer.future.timeout(const Duration(seconds: 5));
  }

  try {
    final results = await Future.wait([
      runCrossThread(1, 100),
      runCrossThread(2, 101),
      runCrossThread(3, 102),
    ]);
    check(
      'cross-thread calls through the real crate return correct results',
      results.every((r) => r.status == 0) &&
          results.map((r) => r.result).join(',') == '2,4,6',
    );
  } on TimeoutException {
    check(
      'cross-thread calls through the real crate return correct results '
      '(timed out -- HUNG)',
      false,
    );
  }
  testDoneCallable.close();

  destroyInstance(handle);
  check(
    'outstanding count still resolves the handle after soft destroy',
    outstandingCount(handle) == 0,
  );
  forgetInstance(handle);
  check('handle is gone after forget', outstandingCount(handle) == 0);

  pkg_ffi.calloc.free(outStatus);
  fastPathCallable.close();
  listenerCallable.close();

  if (failures > 0) {
    // ignore: avoid_print
    print('$failures check(s) failed');
    exit(1);
  }
  // ignore: avoid_print
  print('all checks passed');
  exit(0);
}
