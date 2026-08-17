import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('constants', () {
    expect(demoEnabled, isTrue);
    expect(demoAnswer, 42);
    expect(demoLarge, 9007199254740993);
    expect(demoHalf, 0.5);
    expect(demoLabel, 'boltffi');
    expect(demoBytes, [102, 102, 105]);
    expect(demoMode, DemoMode.fast);
    expect(demoIdle, DemoState$Idle());
    expect(demoAlias, 'boltffi');
    expect(demoComputed, 42);
    expect(demoPair, (3, 5));
    expect(
      demoBusy,
      DemoState$Busy(jobs: 3),
      reason: "case:constants.values.should_expose_inline_and_accessor_values",
    );

    expect(DemoMode.fallback, DemoMode.safe);
    expect(DemoMode.variantCount, 2);
    expect(Point.zero, Point(x: 0.0, y: 0.0));
    expect(MathUtils.defaultPrecision, 2);
  });
}
