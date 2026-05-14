import Demo
import XCTest

final class WithOptionsRecordsTests: XCTestCase {
    func testUserProfileFns() {
        // case:records.with_options.user_profile.some_none
        XCTAssertEqual(echoUserProfile(profile: UserProfile(name: "Ali", age: 30, email: "a@example.com", score: 9.5)), UserProfile(name: "Ali", age: 30, email: "a@example.com", score: 9.5))
        XCTAssertEqual(makeUserProfile(name: "Ali", age: 30, email: nil, score: nil), UserProfile(name: "Ali", age: 30, email: nil, score: nil))
        XCTAssertEqual(userDisplayName(profile: UserProfile(name: "Ali", age: 30, email: nil, score: nil)), "Ali")
        let profileWithEmail = makeUserProfile(name: "Alice", age: 30, email: "alice@example.com", score: 98.5)
        XCTAssertEqual(userDisplayName(profile: profileWithEmail), "Alice <alice@example.com>")
    }

    func testSearchResultFns() {
        // case:records.with_options.search_result.some_none
        XCTAssertEqual(echoSearchResult(result: SearchResult(query: "ffi", total: 3, nextCursor: "next", maxScore: 0.9)), SearchResult(query: "ffi", total: 3, nextCursor: "next", maxScore: 0.9))
        XCTAssertEqual(hasMoreResults(result: SearchResult(query: "ffi", total: 3, nextCursor: "next", maxScore: 0.9)), true)
        XCTAssertEqual(hasMoreResults(result: SearchResult(query: "rust ffi", total: 12, nextCursor: nil, maxScore: nil)), false)
    }
}
