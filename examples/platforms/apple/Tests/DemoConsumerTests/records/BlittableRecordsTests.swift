import Demo
import XCTest

final class BlittableRecordsTests: DemoTestCase {
    func testPointFnsAndMethods() throws {
        demoCase("case:records.blittable.point.should_construct_with_static_new")
        XCTAssertEqual(Point.new(x: 1.0, y: 2.0), Point(x: 1.0, y: 2.0))
        demoCase("case:records.blittable.point.should_return_origin")
        assertPointEquals(Point.origin(), 0.0, 0.0)
        demoCase("case:records.blittable.point.should_construct_from_polar_coordinates")
        assertPointEquals(Point(fromPolar: 2.0, theta: .pi / 2.0), 0.0, 2.0, accuracy: 1e-6)
        demoCase("case:records.blittable.point.should_normalize_unit_vector")
        assertPointEquals(try Point(tryUnit: 3.0, y: 4.0), 0.6, 0.8, accuracy: 1e-6)
        demoCase("case:records.blittable.point.should_reject_zero_unit_vector")
        assertThrowsMessageContains("cannot normalize zero vector", try Point(tryUnit: 0.0, y: 0.0))
        demoCase("case:records.blittable.point.should_return_some_for_checked_unit")
        XCTAssertEqual(Point(checkedUnit: 2.0, y: 0.0), Point(x: 1.0, y: 0.0))
        demoCase("case:records.blittable.point.should_return_none_for_zero_checked_unit")
        XCTAssertNil(Point(checkedUnit: 0.0, y: 0.0))
        demoCase("case:records.blittable.point.should_compute_distance")
        XCTAssertEqual(Point(x: 3.0, y: 4.0).distance(), 5.0, accuracy: 1e-6)
        var scaledPoint = Point(x: 3.0, y: 4.0)
        demoCase("case:records.blittable.point.should_scale_coordinates")
        scaledPoint.scale(factor: 2.0)
        XCTAssertEqual(scaledPoint, Point(x: 6.0, y: 8.0))
        demoCase("case:records.blittable.point.should_add_coordinates")
        XCTAssertEqual(Point(x: 1.0, y: 2.0).add(other: Point(x: 3.0, y: 4.0)), Point(x: 4.0, y: 6.0))
        demoCase("case:records.blittable.point.should_compute_path_length")
        XCTAssertEqual(
            Point.pathLength(points: [Point(x: 0.0, y: 0.0), Point(x: 3.0, y: 4.0), Point(x: 6.0, y: 8.0)]),
            10.0,
            accuracy: 1e-6
        )
        demoCase("case:records.blittable.point.should_report_dimension_count")
        XCTAssertEqual(Point.dimensions(), 2)
        demoCase("case:records.blittable.point.should_roundtrip_value")
        XCTAssertEqual(echoPoint(p: Point(x: 1.5, y: 2.5)), Point(x: 1.5, y: 2.5))
        demoCase("case:records.blittable.point.should_make_from_coordinates")
        XCTAssertEqual(makePoint(x: 3.0, y: 4.0), Point(x: 3.0, y: 4.0))
        demoCase("case:records.blittable.point.should_add_values")
        XCTAssertEqual(addPoints(a: Point(x: 1.0, y: 2.0), b: Point(x: 3.0, y: 4.0)), Point(x: 4.0, y: 6.0))

        demoCase("case:records.blittable.point.should_return_some_for_nonzero_coordinates")
        XCTAssertEqual(tryMakePoint(x: 1.0, y: 2.0), Point(x: 1.0, y: 2.0))
        demoCase("case:records.blittable.point.should_return_none_for_origin_coordinates")
        XCTAssertNil(tryMakePoint(x: 0.0, y: 0.0))
    }

    func testColorFns() {
        demoCase("case:records.blittable.color.should_roundtrip_value")
        XCTAssertEqual(echoColor(c: Color(r: 1, g: 2, b: 3, a: 4)), Color(r: 1, g: 2, b: 3, a: 4))
        demoCase("case:records.blittable.color.should_make_from_channels")
        XCTAssertEqual(makeColor(r: 10, g: 20, b: 30, a: 40), Color(r: 10, g: 20, b: 30, a: 40))
    }

    func testBenchmarkRecordFns() {
        demoCase("case:records.blittable.locations.should_generate_sample_vector")
        let locations = generateLocations(count: 3)
        XCTAssertEqual(locations.count, 3)
        demoCase("case:records.blittable.locations.should_count_vector_items")
        XCTAssertEqual(processLocations(locations: locations), 3)
        demoCase("case:records.blittable.locations.should_count_empty_vector")
        XCTAssertEqual(processLocations(locations: []), 0)
        demoCase("case:records.blittable.locations.should_sum_generated_ratings")
        XCTAssertEqual(sumRatings(locations: locations), 9.3, accuracy: 1e-9)

        let handmadeLocations = [
            Location(id: 100, lat: 40.0, lng: -70.0, rating: 2.5, reviewCount: 5, isOpen: true),
            Location(id: 101, lat: 40.5, lng: -70.5, rating: 4.0, reviewCount: 50, isOpen: false),
        ]
        demoCase("case:records.blittable.locations.should_count_host_constructed_vector")
        XCTAssertEqual(processLocations(locations: handmadeLocations), 2)
        demoCase("case:records.blittable.locations.should_sum_host_constructed_ratings")
        XCTAssertEqual(sumRatings(locations: handmadeLocations), 6.5, accuracy: 1e-9)

        demoCase("case:records.blittable.trades.should_generate_sample_vector")
        let trades = generateTrades(count: 3)
        XCTAssertEqual(trades.count, 3)
        demoCase("case:records.blittable.trades.should_sum_volumes")
        XCTAssertEqual(sumTradeVolumes(trades: trades), 3000)
        demoCase("case:records.blittable.trades.should_aggregate_with_locations")
        XCTAssertEqual(aggregateLocationTradeStats(locations: locations, trades: trades), 3002)

        demoCase("case:records.blittable.particles.should_generate_sample_vector")
        let particles = generateParticles(count: 3)
        XCTAssertEqual(particles.count, 3)
        demoCase("case:records.blittable.particles.should_sum_masses")
        XCTAssertEqual(sumParticleMasses(particles: particles), 3.003, accuracy: 1e-9)

        demoCase("case:records.blittable.sensor_readings.should_generate_sample_vector")
        let readings = generateSensorReadings(count: 3)
        XCTAssertEqual(readings.count, 3)
        demoCase("case:records.blittable.sensor_readings.should_average_generated_temperatures")
        XCTAssertEqual(avgSensorTemperature(readings: readings), 21.0, accuracy: 1e-9)
        demoCase("case:records.blittable.sensor_readings.should_average_empty_vector_as_zero")
        XCTAssertEqual(avgSensorTemperature(readings: []), 0.0)

        demoCase("case:records.blittable.locations.find_location.should_return_some_for_positive_id")
        XCTAssertEqual(
            findLocation(id: 7),
            Location(id: 7, lat: 37.7749, lng: -122.4194, rating: 4.5, reviewCount: 100, isOpen: true)
        )
        demoCase("case:records.blittable.locations.find_location.should_return_none_for_non_positive_id")
        XCTAssertNil(findLocation(id: 0))
        demoCase("case:records.blittable.locations.find_locations.should_return_some_vector_for_positive_count")
        XCTAssertEqual(findLocations(count: 2)?.count, 2)
        demoCase("case:records.blittable.locations.find_locations.should_return_none_for_non_positive_count")
        XCTAssertNil(findLocations(count: 0))
    }
}
