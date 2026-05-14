import Demo
import XCTest

extension MixedRecordParameters {
    static func sample() -> Self {
        MixedRecordParameters(
            tags: ["alpha", "beta"],
            checkpoints: [Point(x: 1.0, y: 2.0), Point(x: 3.0, y: 5.0)],
            fallbackAnchor: Point(x: -1.0, y: -2.0),
            maxRetries: 4,
            previewOnly: true
        )
    }
}

extension MixedRecord {
    static func sample() -> Self {
        MixedRecord(
            name: "outline",
            anchor: Point(x: 10.0, y: 20.0),
            priority: .critical,
            shape: .rectangle(width: 3.0, height: 4.0),
            parameters: .sample()
        )
    }
}

final class MixedRecordsTests: DemoTestCase {
    func testMixedRecordFns() {
        demoCase("case:records.mixed.basic")
        let record = MixedRecord.sample()

        XCTAssertEqual(echoMixedRecord(record: record), record)
        XCTAssertEqual(
            makeMixedRecord(
                name: record.name,
                anchor: record.anchor,
                priority: record.priority,
                shape: record.shape,
                parameters: record.parameters
            ),
            record
        )
    }
}
