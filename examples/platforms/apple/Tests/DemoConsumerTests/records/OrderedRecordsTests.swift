import Demo
import XCTest

final class OrderedRecordsTests: DemoTestCase {
    func testRankComparesByItsDerivedOrd() throws {
        demoCase("case:records.ordered.rank.should_compare_by_rust_ord")
        let low = Rank(tier: 1, score: 9)
        let high = Rank(tier: 2, score: 0)

        XCTAssertEqual(low.compareTo(other: high), -1)
        XCTAssertEqual(high.compareTo(other: low), 1)
        XCTAssertEqual(low.compareTo(other: Rank(tier: 1, score: 9)), 0)
        XCTAssertTrue(low < high)
        XCTAssertTrue(high >= Rank(tier: 2, score: 0))
        XCTAssertEqual([high, Rank(tier: 1, score: 10), low].sorted(), [low, Rank(tier: 1, score: 10), high])
    }

    func testVersionComparesByItsHandWrittenOrd() throws {
        demoCase("case:records.ordered.semantic_version.should_compare_by_rust_ord")
        let alpha = SemanticVersion(major: 1, minor: 2, patch: 3, pre: "alpha")
        let beta = SemanticVersion(major: 1, minor: 2, patch: 3, pre: "beta")
        let release = SemanticVersion(major: 1, minor: 2, patch: 3, pre: nil)
        let nextPatch = SemanticVersion(major: 1, minor: 2, patch: 4, pre: "alpha")

        XCTAssertTrue(alpha < beta)
        XCTAssertTrue(beta < release)
        XCTAssertTrue(release < nextPatch)
        XCTAssertEqual(release.compareTo(other: SemanticVersion(major: 1, minor: 2, patch: 3, pre: nil)), 0)
        XCTAssertEqual([nextPatch, release, beta, alpha].sorted(), [alpha, beta, release, nextPatch])
        XCTAssertEqual([beta, release, alpha].max(), release)
    }
}
