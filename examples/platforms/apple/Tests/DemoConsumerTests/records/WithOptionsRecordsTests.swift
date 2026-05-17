import Demo
import XCTest

final class WithOptionsRecordsTests: DemoTestCase {
    func testUserProfileFns() {
        let allPresent = UserProfile(name: "Ali", age: 30, email: "a@example.com", score: 9.5)
        demoCase("case:records.with_options.user_profile.should_roundtrip_present_options")
        XCTAssertEqual(echoUserProfile(profile: allPresent), allPresent)
        demoCase("case:records.with_options.user_profile.should_make_with_absent_options")
        let allAbsent = makeUserProfile(name: "Ali", age: 30, email: nil, score: nil)
        XCTAssertEqual(allAbsent, UserProfile(name: "Ali", age: 30, email: nil, score: nil))
        demoCase("case:records.with_options.user_profile.should_roundtrip_absent_options")
        XCTAssertEqual(echoUserProfile(profile: allAbsent), allAbsent)
        let mixed = UserProfile(name: "Carol", age: 40, email: "carol@example.com", score: nil)
        demoCase("case:records.with_options.user_profile.should_roundtrip_mixed_options")
        XCTAssertEqual(echoUserProfile(profile: mixed), mixed)
        let emoji = UserProfile(name: "🌍 User", age: 42, email: "café@example.com", score: 3.14)
        demoCase("case:records.with_options.user_profile.should_roundtrip_utf8_optional_string")
        XCTAssertEqual(echoUserProfile(profile: emoji), emoji)
        demoCase("case:records.with_options.user_profile.should_display_name_when_email_absent")
        XCTAssertEqual(userDisplayName(profile: UserProfile(name: "Ali", age: 30, email: nil, score: nil)), "Ali")
        demoCase("case:records.with_options.user_profile.should_make_with_present_options")
        let profileWithEmail = makeUserProfile(name: "Alice", age: 30, email: "alice@example.com", score: 98.5)
        demoCase("case:records.with_options.user_profile.should_display_email_when_present")
        XCTAssertEqual(userDisplayName(profile: profileWithEmail), "Alice <alice@example.com>")
    }

    func testSearchResultFns() {
        let present = SearchResult(query: "ffi", total: 3, nextCursor: "next", maxScore: 0.9)
        demoCase("case:records.with_options.search_result.should_roundtrip_present_options")
        XCTAssertEqual(echoSearchResult(result: present), present)
        demoCase("case:records.with_options.search_result.should_report_more_results_when_cursor_present")
        XCTAssertEqual(hasMoreResults(result: present), true)
        let absent = SearchResult(query: "rust ffi", total: 12, nextCursor: nil, maxScore: nil)
        demoCase("case:records.with_options.search_result.should_roundtrip_absent_options")
        XCTAssertEqual(echoSearchResult(result: absent), absent)
        demoCase("case:records.with_options.search_result.should_report_no_more_results_without_cursor")
        XCTAssertEqual(hasMoreResults(result: absent), false)
    }
}
