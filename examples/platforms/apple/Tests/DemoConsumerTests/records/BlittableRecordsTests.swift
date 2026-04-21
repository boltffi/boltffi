import Demo
import XCTest

final class BlittableRecordsTests: XCTestCase {
    func testPointFnsAndMethods() throws {
        XCTAssertEqual(Point.new(x: 1.0, y: 2.0), Point(x: 1.0, y: 2.0))
        assertPointEquals(Point.origin(), 0.0, 0.0)
        assertPointEquals(Point(fromPolar: 2.0, theta: .pi / 2.0), 0.0, 2.0, accuracy: 1e-6)
        assertPointEquals(try Point(tryUnit: 3.0, y: 4.0), 0.6, 0.8, accuracy: 1e-6)
        assertThrowsMessageContains("cannot normalize zero vector", try Point(tryUnit: 0.0, y: 0.0))
        XCTAssertEqual(Point(checkedUnit: 2.0, y: 0.0), Point(x: 1.0, y: 0.0))
        XCTAssertNil(Point(checkedUnit: 0.0, y: 0.0))
        XCTAssertEqual(Point(x: 3.0, y: 4.0).distance(), 5.0, accuracy: 1e-6)
        var scaledPoint = Point(x: 3.0, y: 4.0)
        scaledPoint.scale(factor: 2.0)
        XCTAssertEqual(scaledPoint, Point(x: 6.0, y: 8.0))
        XCTAssertEqual(Point(x: 1.0, y: 2.0).add(other: Point(x: 3.0, y: 4.0)), Point(x: 4.0, y: 6.0))
        XCTAssertEqual(Point.dimensions(), 2)
        XCTAssertEqual(echoPoint(p: Point(x: 1.5, y: 2.5)), Point(x: 1.5, y: 2.5))
        XCTAssertEqual(tryMakePoint(x: 1.0, y: 2.0), Point(x: 1.0, y: 2.0))
        XCTAssertNil(tryMakePoint(x: 0.0, y: 0.0))
        XCTAssertEqual(makePoint(x: 3.0, y: 4.0), Point(x: 3.0, y: 4.0))
        XCTAssertEqual(addPoints(a: Point(x: 1.0, y: 2.0), b: Point(x: 3.0, y: 4.0)), Point(x: 4.0, y: 6.0))
    }

    func testColorFns() {
        XCTAssertEqual(echoColor(c: Color(r: 1, g: 2, b: 3, a: 4)), Color(r: 1, g: 2, b: 3, a: 4))
        XCTAssertEqual(makeColor(r: 10, g: 20, b: 30, a: 40), Color(r: 10, g: 20, b: 30, a: 40))
    }

    func testBenchmarkRecordFns() {
        let locations = generateLocations(count: 3)
        XCTAssertEqual(locations.count, 3)
        XCTAssertEqual(processLocations(locations: locations), 3)
        XCTAssertEqual(sumRatings(locations: locations), 9.3, accuracy: 1e-9)

        let trades = generateTrades(count: 3)
        XCTAssertEqual(trades.count, 3)
        XCTAssertEqual(sumTradeVolumes(trades: trades), 3000)
        XCTAssertEqual(aggregateLocationTradeStats(locations: locations, trades: trades), 3002)

        let particles = generateParticles(count: 3)
        XCTAssertEqual(particles.count, 3)
        XCTAssertEqual(sumParticleMasses(particles: particles), 3.003, accuracy: 1e-9)

        let readings = generateSensorReadings(count: 3)
        XCTAssertEqual(readings.count, 3)
        XCTAssertEqual(avgSensorTemperature(readings: readings), 21.0, accuracy: 1e-9)

        XCTAssertEqual(
            findLocation(id: 7),
            Location(id: 7, lat: 37.7749, lng: -122.4194, rating: 4.5, reviewCount: 100, isOpen: true)
        )
        XCTAssertNil(findLocation(id: 0))
        XCTAssertEqual(findLocations(count: 2)?.count, 2)
        XCTAssertNil(findLocations(count: 0))
    }
}
