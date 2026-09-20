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
}
