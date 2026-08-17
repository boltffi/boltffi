import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('classes constructors', () {
    final inventory = Inventory.tryNew(1);
    expect(
      inventory.capacity(),
      1,
      reason:
          "case:classes.constructors.inventory.try_new.should_return_inventory_for_positive_capacity",
    );
    expect(inventory.count(), 0);
    expect(inventory.add('only'), isTrue);
    expect(inventory.add('overflow'), isFalse);

    expect(
      () => Inventory.tryNew(0),
      throwsBoltException("capacity must be greater than zero"),
      reason:
          "case:classes.constructors.inventory.try_new.should_reject_zero_capacity",
    );
  });
}
