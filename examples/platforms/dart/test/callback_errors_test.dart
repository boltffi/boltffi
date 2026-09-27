import 'package:demo/demo.dart';
import 'package:test/test.dart';

final class Worker implements FallibleWorker {
  @override
  void run(int mode) {
    if (mode == 1) throw MathError.negativeInput;
    if (mode == 2) throw StateError('unchecked callback failure 東京🦀');
  }

  @override
  int value(int mode) {
    run(mode);
    return 42;
  }
}

final class AsyncWorker implements AsyncFallibleWorker {
  final worker = Worker();

  @override
  Future<void> run(int mode) async => worker.run(mode);

  @override
  Future<int> value(int mode) async => worker.value(mode);
}

void main() {
  tearDownAll(shutdownBoltffi);
  test('callback errors reach Rust', () async {
    final worker = Worker();
    final asyncWorker = AsyncWorker();
    expect(
      () => invokeUnitWorker(worker, 0),
      returnsNormally,
      reason: 'case:callbacks.errors.unit.should_report_success',
    );
    expect(
      () => invokeUnitWorker(worker, 1),
      throwsA(MathError.negativeInput),
      reason: 'case:callbacks.errors.unit.should_report_declared_error',
    );
    expect(
      () => invokeUnitWorker(worker, 2),
      throwsA(MathError.overflow),
      reason: 'case:callbacks.errors.unit.should_report_unexpected_error',
    );
    expect(
      invokeValueWorker(worker, 0),
      42,
      reason: 'case:callbacks.errors.value.should_report_success',
    );
    expect(
      () => invokeValueWorker(worker, 1),
      throwsA(MathError.negativeInput),
      reason: 'case:callbacks.errors.value.should_report_declared_error',
    );
    expect(
      () => invokeValueWorker(worker, 2),
      throwsA(MathError.overflow),
      reason: 'case:callbacks.errors.value.should_report_unexpected_error',
    );
    await expectLater(
      invokeAsyncUnitWorker(asyncWorker, 0),
      completes,
      reason: 'case:callbacks.errors.async_unit.should_report_success',
    );
    await expectLater(
      invokeAsyncUnitWorker(asyncWorker, 1),
      throwsA(MathError.negativeInput),
      reason: 'case:callbacks.errors.async_unit.should_report_declared_error',
    );
    await expectLater(
      invokeAsyncUnitWorker(asyncWorker, 2),
      throwsA(MathError.overflow),
      reason: 'case:callbacks.errors.async_unit.should_report_unexpected_error',
    );
    expect(
      await invokeAsyncValueWorker(asyncWorker, 0),
      42,
      reason: 'case:callbacks.errors.async_value.should_report_success',
    );
    await expectLater(
      invokeAsyncValueWorker(asyncWorker, 1),
      throwsA(MathError.negativeInput),
      reason: 'case:callbacks.errors.async_value.should_report_declared_error',
    );
    await expectLater(
      invokeAsyncValueWorker(asyncWorker, 2),
      throwsA(MathError.overflow),
      reason:
          'case:callbacks.errors.async_value.should_report_unexpected_error',
    );
    expect(
      () => applyResultClosure(
        (value) => throw StateError('unexpected closure error'),
        0,
      ),
      throwsA(MathError.overflow),
    );
  });
}
