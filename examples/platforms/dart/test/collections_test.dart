import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  test('collections', () {
    final map = makeHashMap();
    expect(map.length, 2, reason: "case:collections.hash_map.should_return_values");
    expect(map['first'], 10);
    expect(map['second'], 20);

    expect(
      echoHashMap({}),
      isEmpty,
      reason: "case:collections.hash_map.should_roundtrip_empty",
    );

    final nested = echoHashMap({
      'a': [1, 2, 3],
      'b': [4, 5],
    });
    expect(
      nested.length,
      2,
      reason: "case:collections.hash_map.should_roundtrip_nested_values",
    );
    expect(nested['a'], [1, 2, 3]);
    expect(nested['b'], [4, 5]);
  });
}
