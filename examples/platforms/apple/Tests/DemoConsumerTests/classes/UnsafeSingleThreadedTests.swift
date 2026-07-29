import Demo
import XCTest

final class UnsafeSingleThreadedTests: DemoTestCase {
    func testMapViewReturnsMarkerHandle() throws {
        demoCase("case:classes.unsafe_single_threaded.map_view.add_marker.should_return_single_threaded_marker_handle")
        let mapView = MapView()
        let marker = mapView.addMarker(options: MarkerOptions(id: 7, title: "harbor"))
        XCTAssertEqual(marker.id(), 7)
        XCTAssertEqual(marker.title(), "harbor")
        XCTAssertEqual(mapView.markerCount(), 1)
    }

    func testStateHolderSyncMethods() throws {
        let stateHolder = StateHolder(label: "local")
        let incrementer = makeIncrementingCallback(delta: 3)
        XCTAssertEqual(stateHolder.getLabel(), "local")
        XCTAssertEqual(stateHolder.getValue(), 0)
        stateHolder.setValue(value: 5)
        XCTAssertEqual(stateHolder.getValue(), 5)
        XCTAssertEqual(stateHolder.increment(), 6)
        stateHolder.addItem(item: "a")
        stateHolder.addItem(item: "b")
        XCTAssertEqual(stateHolder.itemCount(), 2)
        XCTAssertEqual(stateHolder.getItems(), ["a", "b"])
        XCTAssertEqual(stateHolder.removeLast(), "b")
        XCTAssertEqual(stateHolder.transformValue(f: { $0 / 2 }), 3)
        XCTAssertEqual(stateHolder.applyValueCallback(callback: incrementer), 6)
        stateHolder.clear()
        XCTAssertEqual(stateHolder.getValue(), 0)
        XCTAssertEqual(stateHolder.getItems(), [])

        let counter = CounterSingleThreaded()
        counter.set(value: 5)
        XCTAssertEqual(counter.get(), 5)
        counter.increment()
        XCTAssertEqual(counter.get(), 6)

        let accumulator = AccumulatorSingleThreaded()
        accumulator.add(amount: 4)
        accumulator.add(amount: 6)
        XCTAssertEqual(accumulator.get(), 10)
        accumulator.reset()
        XCTAssertEqual(accumulator.get(), 0)
    }
}
