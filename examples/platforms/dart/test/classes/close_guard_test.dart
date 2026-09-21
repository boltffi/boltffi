import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);

  test('closed counter rejects subsequent calls', () {
    final counter = GuardedCounter(1);
    try {
      expect(counter.increment(), 2);
      counter.dispose$();
      expect(
        () => counter.increment(),
        throwsBoltException('Object has been disposed'),
        reason:
            'case:classes.close_guard.guarded_counter.increment.should_reject_calls_after_close',
      );
    } finally {
      counter.dispose$();
    }
  });

  test('gated counter applies the callback result', () {
    final counter = GuardedCounter(10);
    try {
      expect(counter.incrementThroughGate((observed) => observed + 5), 25);
      expect(counter.increment(), 26);
    } finally {
      counter.dispose$();
    }
  });

  test('disposing inside a callback preserves the in-flight call', () {
    final counter = GuardedCounter(10);
    var gateCalled = false;
    try {
      final result = counter.incrementThroughGate((observed) {
        gateCalled = true;
        counter.dispose$();
        counter.dispose$();
        expect(
          () => counter.increment(),
          throwsBoltException('Object has been disposed'),
        );
        return observed + 5;
      });
      expect(gateCalled, isTrue);
      expect(
        result,
        25,
        reason:
            'case:classes.close_guard.guarded_counter.increment_through_gate.should_complete_in_flight_call_when_closed',
      );
      expect(
        () => counter.increment(),
        throwsBoltException('Object has been disposed'),
      );
    } finally {
      counter.dispose$();
    }
  });

  test('nested calls remain alive until the outermost call returns', () {
    final counter = GuardedCounter(10);
    try {
      final result = counter.incrementThroughGate((outerValue) {
        final nested = counter.incrementThroughGate((innerValue) {
          counter.dispose$();
          return innerValue;
        });
        expect(nested, 20);
        expect(
          () => counter.increment(),
          throwsBoltException('Object has been disposed'),
        );
        return outerValue;
      });
      expect(result, 30);
    } finally {
      counter.dispose$();
    }
  });

  test('pending async calls complete after disposal', () async {
    final worker = AsyncWorker('guarded');
    try {
      final first = worker.processAfterPolls('first', 2);
      final second = worker.processAfterPolls('second', 4);
      worker.dispose$();
      expect(
        () => worker.getPrefix(),
        throwsBoltException('Object has been disposed'),
      );
      expect(await first, 'guarded: first');
      expect(await second, 'guarded: second');
    } finally {
      worker.dispose$();
    }
  });

  test('async errors after disposal are propagated', () async {
    final worker = AsyncWorker('guarded');
    try {
      final pending = worker.tryProcess('');
      worker.dispose$();
      await expectLater(
        pending,
        throwsBoltException('input must not be empty'),
      );
    } finally {
      worker.dispose$();
    }
  });

  test('pending async calls can be cancelled after disposal', () async {
    final worker = AsyncWorker('guarded');
    final token = $$BoltCancellationToken();
    try {
      final pending = worker.processAfterPolls(
        'cancelled',
        100,
        cancellationToken: token,
      );
      worker.dispose$();
      token.cancel();
      await expectLater(pending, throwsA(isA<$$BoltCancelledException>()));
    } finally {
      worker.dispose$();
    }
  });
}
