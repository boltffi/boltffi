import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('nested records', () {
    final line = makeLine(0.0, 0.0, 3.0, 4.0);
    expect(
      line,
      Line(start: Point(x: 0.0, y: 0.0), end: Point(x: 3.0, y: 4.0)),
      reason: "case:records.nested.line.should_make_from_coordinates",
    );

    expect(
      echoLine(line),
      line,
      reason: "case:records.nested.line.should_roundtrip_nested_points",
    );

    expect(
      lineLength(line),
      closeTo(5, 0.0001),
      reason: "case:records.nested.line.should_compute_length",
    );

    final rect = Rect(
      origin: Point(x: 1.0, y: 2.0),
      dimensions: Dimensions(width: 3.0, height: 4.0),
    );

    expect(
      echoRect(rect),
      rect,
      reason: "case:records.nested.rect.should_roundtrip_nested_records",
    );

    expect(
      rectArea(rect),
      closeTo(12.0, 0.0001),
      reason: "case:records.nested.rect.should_compute_area",
    );
  });
}
