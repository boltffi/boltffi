import { assert, assertApprox, assertPoint, assertThrowsWithMessage, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:records.blittable.point.should_roundtrip_value");
  assertPoint(demo.echoPoint({ x: 1, y: 2 }), { x: 1, y: 2 });
  globalThis.demoCase("case:records.blittable.point.should_make_from_coordinates");
  assertPoint(demo.makePoint(1, 2), { x: 1, y: 2 });
  globalThis.demoCase("case:records.blittable.point.should_add_values");
  assertPoint(demo.addPoints({ x: 3, y: 4 }, { x: 5, y: 6 }), { x: 8, y: 10 });

  globalThis.demoCase("case:records.blittable.point.should_return_some_for_nonzero_coordinates");
  assertPoint(demo.tryMakePoint(2, 3), { x: 2, y: 3 });
  globalThis.demoCase("case:records.blittable.point.should_return_none_for_origin_coordinates");
  assert.equal(demo.tryMakePoint(0, 0), null);

  globalThis.demoCase("case:records.blittable.point.should_construct_with_static_new");
  assertPoint(demo.Point.new(1, 2), { x: 1, y: 2 });
  globalThis.demoCase("case:records.blittable.point.should_return_origin");
  assertPoint(demo.Point.origin(), { x: 0, y: 0 });
  globalThis.demoCase("case:records.blittable.point.should_construct_from_polar_coordinates");
  assertPoint(demo.Point.fromPolar(2, Math.PI / 2), { x: 0, y: 2 }, 1e-9);
  globalThis.demoCase("case:records.blittable.point.should_normalize_unit_vector");
  assertPoint(demo.Point.tryUnit(3, 4), { x: 0.6, y: 0.8 });
  globalThis.demoCase("case:records.blittable.point.should_reject_zero_unit_vector");
  assertThrowsWithMessage(() => demo.Point.tryUnit(0, 0), "cannot normalize zero vector");
  globalThis.demoCase("case:records.blittable.point.should_return_none_for_zero_checked_unit");
  assert.equal(demo.Point.checkedUnit(0, 0), null);
  globalThis.demoCase("case:records.blittable.point.should_return_some_for_checked_unit");
  assertPoint(demo.Point.checkedUnit(3, 4), { x: 0.6, y: 0.8 });
  globalThis.demoCase("case:records.blittable.point.should_compute_distance");
  assert.equal(demo.Point.distance({ x: 3, y: 4 }), 5);
  globalThis.demoCase("case:records.blittable.point.should_scale_coordinates");
  assertPoint(demo.Point.scale({ x: 3, y: 4 }, 2), { x: 6, y: 8 });
  globalThis.demoCase("case:records.blittable.point.should_add_coordinates");
  assertPoint(demo.Point.add({ x: 3, y: 4 }, { x: 5, y: 6 }), { x: 8, y: 10 });
  globalThis.demoCase("case:records.blittable.point.should_compute_path_length");
  assert.equal(demo.Point.pathLength([{ x: 0, y: 0 }, { x: 3, y: 4 }, { x: 6, y: 8 }]), 10);
  globalThis.demoCase("case:records.blittable.point.should_report_dimension_count");
  assert.equal(demo.Point.dimensions(), 2);

  globalThis.demoCase("case:records.blittable.color.should_roundtrip_value");
  const color = { r: 1, g: 2, b: 3, a: 255 };
  assert.deepEqual(demo.echoColor(color), color);
  globalThis.demoCase("case:records.blittable.color.should_make_from_channels");
  assert.deepEqual(demo.makeColor(9, 8, 7, 6), { r: 9, g: 8, b: 7, a: 6 });

  globalThis.demoCase("case:records.blittable.locations.should_generate_sample_vector");
  const locations = demo.generateLocations(3);
  assert.equal(locations.length, 3);
  globalThis.demoCase("case:records.blittable.locations.should_count_vector_items");
  assert.equal(demo.processLocations(locations), 3);
  globalThis.demoCase("case:records.blittable.locations.should_count_empty_vector");
  assert.equal(demo.processLocations([]), 0);
  const hostLocations = [
    { id: 1n, lat: 1.0, lng: 2.0, rating: 3.5, reviewCount: 4, isOpen: true },
    { id: 2n, lat: 5.0, lng: 6.0, rating: 2.5, reviewCount: 8, isOpen: false },
  ];
  globalThis.demoCase("case:records.blittable.locations.should_count_host_constructed_vector");
  assert.equal(demo.processLocations(hostLocations), 2);
  globalThis.demoCase("case:records.blittable.locations.should_sum_generated_ratings");
  assertApprox(demo.sumRatings(locations), 9.3, 1e-4);
  globalThis.demoCase("case:records.blittable.locations.should_sum_host_constructed_ratings");
  assertApprox(demo.sumRatings(hostLocations), 6.0, 1e-4);

  globalThis.demoCase("case:records.blittable.trades.should_generate_sample_vector");
  const trades = demo.generateTrades(3);
  assert.equal(trades.length, 3);
  globalThis.demoCase("case:records.blittable.trades.should_sum_volumes");
  assert.equal(demo.sumTradeVolumes(trades), 3_000n);
  globalThis.demoCase("case:records.blittable.trades.should_aggregate_with_locations");
  assert.equal(demo.aggregateLocationTradeStats(hostLocations, trades), 3_001n);

  globalThis.demoCase("case:records.blittable.particles.should_generate_sample_vector");
  const particles = demo.generateParticles(3);
  assert.equal(particles.length, 3);
  globalThis.demoCase("case:records.blittable.particles.should_sum_masses");
  assertApprox(demo.sumParticleMasses(particles), 3.003, 1e-4);

  globalThis.demoCase("case:records.blittable.sensor_readings.should_generate_sample_vector");
  const readings = demo.generateSensorReadings(3);
  assert.equal(readings.length, 3);
  globalThis.demoCase("case:records.blittable.sensor_readings.should_average_generated_temperatures");
  assertApprox(demo.avgSensorTemperature(readings), 21.0, 1e-4);
  globalThis.demoCase("case:records.blittable.sensor_readings.should_average_empty_vector_as_zero");
  assert.equal(demo.avgSensorTemperature([]), 0.0);

  globalThis.demoCase("case:records.blittable.locations.find_location.should_return_some_for_positive_id");
  const foundLocation = demo.findLocation(7);
  assert.ok(foundLocation);
  assert.equal(foundLocation.id, 7n);
  globalThis.demoCase("case:records.blittable.locations.find_location.should_return_none_for_non_positive_id");
  assert.equal(demo.findLocation(0), null);
  globalThis.demoCase("case:records.blittable.locations.find_locations.should_return_some_vector_for_positive_count");
  const foundLocations = demo.findLocations(3);
  assert.ok(foundLocations);
  assert.equal(foundLocations.length, 3);
  globalThis.demoCase("case:records.blittable.locations.find_locations.should_return_none_for_non_positive_count");
  assert.equal(demo.findLocations(0), null);
}
