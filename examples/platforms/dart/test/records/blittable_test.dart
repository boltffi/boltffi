import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  test('blittable records', () {
    expect(
      Point.$new(7.0, 8.0),
      Point(x: 7.0, y: 8.0),
      reason: "case:records.blittable.point.should_construct_with_static_new",
    );
    final point = makePoint(1.0, 2.0);
    expect(
      point,
      Point(x: 1.0, y: 2.0),
      reason: "case:records.blittable.point.should_make_from_coordinates",
    );

    final origin = Point.origin();
    expect(
      origin,
      Point(x: 0.0, y: 0.0),
      reason: "case:records.blittable.point.should_return_origin",
    );

    final fromPolar = Point.fromPolar(2.0, 3.141592653589793 / 2.0);
    expect(fromPolar.x, closeTo(0.0, 0.0001));
    expect(
      fromPolar.y,
      closeTo(2.0, 0.0001),
      reason:
          "case:records.blittable.point.should_construct_from_polar_coordinates",
    );

    final unit = Point.tryUnit(3.0, 4.0);
    expect(unit.x, closeTo(0.6, 0.0001));
    expect(
      unit.y,
      closeTo(0.8, 0.0001),
      reason: "case:records.blittable.point.should_normalize_unit_vector",
    );

    expect(
      () => Point.tryUnit(0.0, 0.0),
      throwsBoltException("cannot normalize zero vector"),
      reason: "case:records.blittable.point.should_reject_zero_unit_vector",
    );
    expect(
      Point.checkedUnit(3.0, 4.0),
      isNotNull,
      reason:
          "case:records.blittable.point.should_return_some_for_checked_unit",
    );

    expect(
      Point.checkedUnit(0.0, 0.0),
      isNull,
      reason:
          "case:records.blittable.point.should_return_none_for_zero_checked_unit",
    );

    expect(
      point.distance(),
      closeTo(2.2360679775, 0.0001),
      reason: "case:records.blittable.point.should_compute_distance",
    );

    final toScale = Point(x: 1.0, y: 2.0);
    toScale.scale(2.5);
    expect(toScale.x, 2.5);
    expect(
      toScale.y,
      5.0,
      reason: "case:records.blittable.point.should_scale_coordinates",
    );

    expect(
      point.add(Point(x: 10.0, y: 20.0)),
      Point(x: 11.0, y: 22.0),
      reason: "case:records.blittable.point.should_add_coordinates",
    );

    expect(
      Point.pathLength([
        Point(x: 0.0, y: 0.0),
        Point(x: 3.0, y: 4.0),
        Point(x: 6.0, y: 8.0),
      ]),
      closeTo(10.0, 0.0001),
      reason: "case:records.blittable.point.should_compute_path_length",
    );

    expect(
      Point.dimensions(),
      2,
      reason: "case:records.blittable.point.should_report_dimension_count",
    );

    expect(
      echoPoint(point),
      Point(x: 1.0, y: 2.0),
      reason: "case:records.blittable.point.should_roundtrip_value",
    );

    expect(
      tryMakePoint(1.0, 2.0),
      Point(x: 1.0, y: 2.0),
      reason:
          "case:records.blittable.point.should_return_some_for_nonzero_coordinates",
    );
    expect(
      tryMakePoint(0.0, 0.0),
      isNull,
      reason:
          "case:records.blittable.point.should_return_none_for_origin_coordinates",
    );

    expect(
      addPoints(Point(x: 3.0, y: 4.0), Point(x: 5.0, y: 6.0)),
      Point(x: 8.0, y: 10.0),
      reason: "case:records.blittable.point.should_add_values",
    );

    expect(tryMakePoint(0.0, 0.0), isNull);
    expect(
      MathUtils.distanceBetween(Point(x: 0.0, y: 0.0), Point(x: 3.0, y: 4.0)),
      closeTo(5.0, 0.0001),
    );

    final midpoint = MathUtils.midpoint(
      Point(x: 0.0, y: 0.0),
      Point(x: 2.0, y: 4.0),
    );
    expect(midpoint.x, 1.0);
    expect(midpoint.y, 2.0);

    final color = Color(r: 0xAA, g: 0xBB, b: 0xCC, a: 0xDD);
    expect(
      echoColor(color),
      color,
      reason: "case:records.blittable.color.should_roundtrip_value",
    );

    final madeColor = makeColor(1, 2, 3, 4);
    expect(
      madeColor,
      Color(r: 1, g: 2, b: 3, a: 4),
      reason: "case:records.blittable.color.should_make_from_channels",
    );

    final locations = generateLocations(3);
    expect(
      locations.length,
      3,
      reason: "case:records.blittable.locations.should_generate_sample_vector",
    );
    expect(
      processLocations(locations),
      3,
      reason: "case:records.blittable.locations.should_count_vector_items",
    );
    expect(
      processLocations([]),
      0,
      reason: "case:records.blittable.locations.should_count_empty_vector",
    );

    final hostLocations = [
      Location(
        id: 1,
        lat: 1.0,
        lng: 2.0,
        rating: 3.5,
        reviewCount: 4,
        isOpen: true,
      ),
      Location(
        id: 2,
        lat: 5.0,
        lng: 6.0,
        rating: 2.5,
        reviewCount: 8,
        isOpen: false,
      ),
    ];
    expect(
      processLocations(hostLocations),
      2,
      reason:
          "case:records.blittable.locations.should_count_host_constructed_vector",
    );

    expect(
      sumRatings(locations),
      closeTo(9.3, 0.0001),
      reason: "case:records.blittable.locations.should_sum_generated_ratings",
    );
    expect(
      sumRatings(hostLocations),
      closeTo(6.0, 0.0001),
      reason:
          "case:records.blittable.locations.should_sum_host_constructed_ratings",
    );

    final trades = generateTrades(3);
    expect(
      trades.length,
      3,
      reason: "case:records.blittable.trades.should_generate_sample_vector",
    );
    expect(
      sumTradeVolumes(trades),
      3000,
      reason: "case:records.blittable.trades.should_sum_volumes",
    );
    expect(
      aggregateLocationTradeStats(locations, trades),
      3002,
      reason: "case:records.blittable.trades.should_aggregate_with_locations",
    );

    final particles = generateParticles(3);
    expect(
      particles.length,
      3,
      reason: "case:records.blittable.particles.should_generate_sample_vector",
    );
    expect(
      sumParticleMasses(particles),
      closeTo(3.003, 0.0001),
      reason: "case:records.blittable.particles.should_sum_masses",
    );

    final readings = generateSensorReadings(3);
    expect(
      readings.length,
      3,
      reason:
          "case:records.blittable.sensor_readings.should_generate_sample_vector",
    );
    expect(
      avgSensorTemperature(readings),
      closeTo(21.0, 0.0001),
      reason:
          "case:records.blittable.sensor_readings.should_average_generated_temperatures",
    );
    expect(
      avgSensorTemperature([]),
      closeTo(0.0, 0.0001),
      reason:
          "case:records.blittable.sensor_readings.should_average_empty_vector_as_zero",
    );

    final foundLoc = findLocation(7);
    expect(foundLoc, isNotNull);
    expect(
      foundLoc!.id,
      7,
      reason:
          "case:records.blittable.locations.find_location.should_return_some_for_positive_id",
    );

    expect(
      findLocation(0),
      isNull,
      reason:
          "case:records.blittable.locations.find_location.should_return_none_for_non_positive_id",
    );

    final foundLocs = findLocations(3);
    expect(
      foundLocs!.length,
      3,
      reason:
          "case:records.blittable.locations.find_locations.should_return_some_vector_for_positive_count",
    );
    expect(
      findLocations(0),
      isNull,
      reason:
          "case:records.blittable.locations.find_locations.should_return_none_for_non_positive_count",
    );
  });
}
