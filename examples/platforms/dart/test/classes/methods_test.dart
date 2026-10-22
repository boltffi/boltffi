import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('counter method from a methods block', () {
    final counter = Counter(5);
    try {
      counter.decrement();
      expect(
        counter.$get(),
        4,
        reason:
            "case:classes.methods.counter.decrement.should_call_a_method_declared_in_a_methods_block",
      );
    } finally {
      counter.dispose$();
    }
  });

  test('class methods', () async {
    final dcDisposed = DataConsumer();
    dcDisposed.dispose$();
    expect(
      () => dcDisposed.setProvider(DataProviderImpl()),
      throwsBoltException("Object has been disposed"),
    );

    final dc = DataConsumer();
    try {
      dc.setProvider(DataProviderImpl());
      final sum = dc.computeSum();
      expect(sum, 20);
    } finally {
      dc.dispose$();
    }
  });
}
