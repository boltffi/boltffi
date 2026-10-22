import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('traffic light methods see the variant behind the handle', () {
    final red = TrafficLight();
    final green = red.turnGreen(30);
    try {
      expect(red.secondsLeft(), 0, reason: "case:classes.enum_backed.traffic_light.seconds_left.should_match_on_the_variant_behind_the_handle");
      expect(green.secondsLeft(), 30, reason: "case:classes.enum_backed.traffic_light.seconds_left.should_match_on_the_variant_behind_the_handle");
    } finally {
      green.dispose$();
      red.dispose$();
    }
  });
}
