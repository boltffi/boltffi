import Demo
import XCTest

final class ConstructorCoverageMatrixTests: XCTestCase {
    func testConstructorCoverageMatrixConstructors() throws {
        let base = ConstructorCoverageMatrix()
        XCTAssertEqual(base.constructorVariant(), "new")
        XCTAssertEqual(base.summary(), "default")
        XCTAssertEqual(base.payloadChecksum(), 0)
        XCTAssertEqual(base.vectorCount(), 0)

        let scalarMix = ConstructorCoverageMatrix(withScalarMix: 7, enabled: true, priority: .high)
        XCTAssertEqual(scalarMix.constructorVariant(), "with_scalar_mix")
        XCTAssertEqual(scalarMix.summary(), "version=7;enabled=true;priority=high")
        XCTAssertEqual(scalarMix.payloadChecksum(), 0)
        XCTAssertEqual(scalarMix.vectorCount(), 0)

        let stringAndBytes = ConstructorCoverageMatrix(withStringAndBytes: "bolt", payload: Data([1, 2, 3, 4]))
        XCTAssertEqual(stringAndBytes.constructorVariant(), "with_string_and_bytes")
        XCTAssertEqual(stringAndBytes.summary(), "label=bolt;bytes=4")
        XCTAssertEqual(stringAndBytes.payloadChecksum(), 10)
        XCTAssertEqual(stringAndBytes.vectorCount(), 4)

        let blittableAndRecord = ConstructorCoverageMatrix(withBlittableAndRecord: Point(x: 1.5, y: 2.5), person: Person(name: "Alice", age: 31))
        XCTAssertEqual(blittableAndRecord.constructorVariant(), "with_blittable_and_record")
        XCTAssertEqual(blittableAndRecord.summary(), "origin=1.5:2.5;person=Alice#31")
        XCTAssertEqual(blittableAndRecord.payloadChecksum(), 0)
        XCTAssertEqual(blittableAndRecord.vectorCount(), 1)

        let optionalProfileAndCursor = ConstructorCoverageMatrix(withOptionalProfileAndCursor: UserProfile(name: "John", age: 29, email: "john@example.com", score: 9.5), nextCursor: "cursor-7")
        XCTAssertEqual(optionalProfileAndCursor.constructorVariant(), "with_optional_profile_and_cursor")
        XCTAssertEqual(optionalProfileAndCursor.summary(), "profile=John#29#john@example.com#9.5;cursor=cursor-7")
        XCTAssertEqual(optionalProfileAndCursor.payloadChecksum(), 0)
        XCTAssertEqual(optionalProfileAndCursor.vectorCount(), 2)

        let vectorsAndPolygon = ConstructorCoverageMatrix(withVectorsAndPolygon: ["ffi", "swift"], anchors: [Point(x: 0, y: 0), Point(x: 1, y: 1)], polygon: Polygon(points: [Point(x: 0, y: 0), Point(x: 2, y: 0), Point(x: 1, y: 1)]))
        XCTAssertEqual(vectorsAndPolygon.constructorVariant(), "with_vectors_and_polygon")
        XCTAssertEqual(vectorsAndPolygon.summary(), "tags=ffi|swift;anchors=2;polygon=3")
        XCTAssertEqual(vectorsAndPolygon.payloadChecksum(), 0)
        XCTAssertEqual(vectorsAndPolygon.vectorCount(), 7)

        let collectionRecords = ConstructorCoverageMatrix(withCollectionRecords: Team(name: "Platform", members: ["Alice", "John"]), classroom: Classroom(students: [Person(name: "Alice", age: 20), Person(name: "John", age: 21)]), polygon: Polygon(points: [Point(x: 0, y: 0), Point(x: 1, y: 0), Point(x: 1, y: 1)]))
        XCTAssertEqual(collectionRecords.constructorVariant(), "with_collection_records")
        XCTAssertEqual(collectionRecords.summary(), "team=Platform;members=2;students=2;polygon=3")
        XCTAssertEqual(collectionRecords.payloadChecksum(), 0)
        XCTAssertEqual(collectionRecords.vectorCount(), 7)

        let enumMix = ConstructorCoverageMatrix(withEnumMix: .byGroups(groups: [["café", "🌍"], [], ["common"]]), message: .image(url: "https://example.com/image.png", width: 640, height: 480), task: Task(title: "ship", priority: .critical, completed: false))
        XCTAssertEqual(enumMix.constructorVariant(), "with_enum_mix")
        XCTAssertEqual(enumMix.summary(), "filter=groups:3;message=image:https://example.com/image.png#640x480;task=ship#critical")
        XCTAssertEqual(enumMix.payloadChecksum(), 0)
        XCTAssertEqual(enumMix.vectorCount(), 1)

        let everything = ConstructorCoverageMatrix(withEverything: Person(name: "Alice", age: 31), address: Address(street: "Main", city: "AMS", zip: "1000"), profile: UserProfile(name: "John", age: 29, email: "john@example.com", score: 9.5), searchResult: SearchResult(query: "route", total: 5, nextCursor: "next-9", maxScore: 7.5), payload: Data([4, 5, 6]), filter: .byRange(min: 1, max: 3), tags: ["alpha", "beta"])
        XCTAssertEqual(everything.constructorVariant(), "with_everything")
        XCTAssertEqual(everything.summary(), "person=Alice#31;city=AMS;profile=profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0;tags=alpha|beta")
        XCTAssertEqual(everything.payloadChecksum(), 15)
        XCTAssertEqual(everything.vectorCount(), 10)
        XCTAssertEqual(
            everything.summarizeBorrowedInputs(
                profile: UserProfile(name: "John", age: 29, email: "john@example.com", score: 9.5),
                searchResult: SearchResult(query: "route", total: 5, nextCursor: "next-9", maxScore: 7.5),
                filter: .byRange(min: 1, max: 3)
            ),
            "profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0"
        )

        let fallible = try ConstructorCoverageMatrix(tryWithPayloadAndSearchResult: Data([7, 8]), searchResult: SearchResult(query: "search", total: 4, nextCursor: "cursor-4", maxScore: nil), filter: .byName(name: "ali"))
        XCTAssertEqual(fallible.constructorVariant(), "try_with_payload_and_search_result")
        XCTAssertEqual(fallible.summary(), "query=search;cursor=cursor-4;filter=name:ali")
        XCTAssertEqual(fallible.payloadChecksum(), 15)
        XCTAssertEqual(fallible.vectorCount(), 6)

        assertThrowsMessageContains(
            "payload must not be empty",
            try ConstructorCoverageMatrix(
                tryWithPayloadAndSearchResult: Data(),
                searchResult: SearchResult(query: "search", total: 4, nextCursor: nil, maxScore: nil),
                filter: .none
            )
        )
    }
}
