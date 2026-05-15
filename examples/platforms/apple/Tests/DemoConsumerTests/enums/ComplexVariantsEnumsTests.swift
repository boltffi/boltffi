import Demo
import XCTest

final class ComplexVariantsEnumsTests: DemoTestCase {
    func testFilterFns() {
        let nameFilter = Filter.byName(name: "ali")
        let pointFilter = Filter.byPoints(anchors: [Point(x: 0.0, y: 0.0), Point(x: 1.0, y: 1.0)])
        let groupFilter = Filter.byGroups(groups: [["café", "🌍"], [], ["common"]])
        demoCase("case:enums.complex_variants.filter.should_roundtrip_variants")
        XCTAssertEqual(echoFilter(f: .none), .none)
        XCTAssertEqual(echoFilter(f: nameFilter), nameFilter)
        XCTAssertEqual(echoFilter(f: groupFilter), groupFilter)
        demoCase("case:enums.complex_variants.filter.should_describe_variants")
        XCTAssertEqual(describeFilter(f: nameFilter), "filter by name: ali")
        XCTAssertEqual(describeFilter(f: pointFilter), "filter by 2 anchor points")
        XCTAssertEqual(describeFilter(f: .byTags(tags: ["ffi", "jni"])), "filter by 2 tags")
        XCTAssertEqual(describeFilter(f: groupFilter), "filter by 3 groups")
        XCTAssertEqual(describeFilter(f: .byRange(min: 1.0, max: 5.0)), "filter by range: 1..5")
    }

    func testApiResponseFns() {
        let success = ApiResponse.success(data: "ok")
        let redirect = ApiResponse.redirect(url: "https://example.com")
        demoCase("case:enums.complex_variants.api_response.should_roundtrip_variants")
        XCTAssertEqual(echoApiResponse(response: success), success)
        XCTAssertEqual(echoApiResponse(response: redirect), redirect)
        demoCase("case:enums.complex_variants.api_response.should_identify_success")
        XCTAssertEqual(isSuccess(response: success), true)
        XCTAssertEqual(isSuccess(response: .empty), false)
    }
}
