import Demo
import XCTest

final class WithOptionsRecordsTests: DemoTestCase {
    func testUserProfileFns() {
        demoCase("case:records.with_options.user_profile.should_roundtrip_present_options")
        XCTAssertEqual(echoUserProfile(profile: UserProfile(name: "Ali", age: 30, email: "a@example.com", score: 9.5)), UserProfile(name: "Ali", age: 30, email: "a@example.com", score: 9.5))
        demoCase("case:records.with_options.user_profile.should_make_with_absent_options")
        XCTAssertEqual(makeUserProfile(name: "Ali", age: 30, email: nil, score: nil), UserProfile(name: "Ali", age: 30, email: nil, score: nil))
        demoCase("case:records.with_options.user_profile.should_display_name_when_email_absent")
        XCTAssertEqual(userDisplayName(profile: UserProfile(name: "Ali", age: 30, email: nil, score: nil)), "Ali")
        demoCase("case:records.with_options.user_profile.should_make_with_present_options")
        let profileWithEmail = makeUserProfile(name: "Alice", age: 30, email: "alice@example.com", score: 98.5)
        demoCase("case:records.with_options.user_profile.should_display_email_when_present")
        XCTAssertEqual(userDisplayName(profile: profileWithEmail), "Alice <alice@example.com>")
    }

    func testSearchResultFns() {
        demoCase("case:records.with_options.search_result.should_roundtrip_present_options")
        XCTAssertEqual(echoSearchResult(result: SearchResult(query: "ffi", total: 3, nextCursor: "next", maxScore: 0.9)), SearchResult(query: "ffi", total: 3, nextCursor: "next", maxScore: 0.9))
        demoCase("case:records.with_options.search_result.should_report_more_results_when_cursor_present")
        XCTAssertEqual(hasMoreResults(result: SearchResult(query: "ffi", total: 3, nextCursor: "next", maxScore: 0.9)), true)
        demoCase("case:records.with_options.search_result.should_report_no_more_results_without_cursor")
        XCTAssertEqual(hasMoreResults(result: SearchResult(query: "rust ffi", total: 12, nextCursor: nil, maxScore: nil)), false)
    }
}
