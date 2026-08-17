import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
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
