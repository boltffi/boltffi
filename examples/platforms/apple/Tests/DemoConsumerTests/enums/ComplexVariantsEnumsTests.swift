import Demo
import XCTest

final class ComplexVariantsEnumsTests: DemoTestCase {
    func testFilterFns() {
        let nameFilter = Filter.byName(name: "ali")
        let pointFilter = Filter.byPoints(anchors: [Point(x: 0.0, y: 0.0), Point(x: 1.0, y: 1.0)])
        let tagsFilter = Filter.byTags(tags: ["café", "🌍"])
        let groupFilter = Filter.byGroups(groups: [["café", "🌍"], [], ["common"]])
        demoCase("case:enums.complex_variants.filter.none.should_roundtrip_unit_variant")
        XCTAssertEqual(echoFilter(f: .none), .none)
        demoCase("case:enums.complex_variants.filter.by_name.should_roundtrip_string_payload")
        XCTAssertEqual(echoFilter(f: nameFilter), nameFilter)
        demoCase("case:enums.complex_variants.filter.by_points.should_roundtrip_record_vector_payload")
        XCTAssertEqual(echoFilter(f: pointFilter), pointFilter)
        demoCase("case:enums.complex_variants.filter.by_tags.should_roundtrip_string_vector_payload")
        XCTAssertEqual(echoFilter(f: tagsFilter), tagsFilter)
        demoCase("case:enums.complex_variants.filter.by_groups.should_roundtrip_nested_string_vectors")
        XCTAssertEqual(echoFilter(f: groupFilter), groupFilter)
        demoCase("case:enums.complex_variants.filter.by_name.should_describe_string_payload")
        XCTAssertEqual(describeFilter(f: nameFilter), "filter by name: ali")
        demoCase("case:enums.complex_variants.filter.by_points.should_describe_record_vector_payload")
        XCTAssertEqual(describeFilter(f: pointFilter), "filter by 2 anchor points")
        demoCase("case:enums.complex_variants.filter.by_tags.should_describe_string_vector_payload")
        XCTAssertEqual(describeFilter(f: .byTags(tags: ["ffi", "jni"])), "filter by 2 tags")
        demoCase("case:enums.complex_variants.filter.by_groups.should_describe_nested_string_vectors")
        XCTAssertEqual(describeFilter(f: groupFilter), "filter by 3 groups")
        demoCase("case:enums.complex_variants.filter.by_range.should_describe_numeric_bounds")
        XCTAssertEqual(describeFilter(f: .byRange(min: 1.0, max: 5.0)), "filter by range: 1..5")
    }

    func testApiResponseFns() {
        let success = ApiResponse.success(data: "ok")
        let redirect = ApiResponse.redirect(url: "https://example.com")
        demoCase("case:enums.complex_variants.api_response.success.should_roundtrip_string_payload")
        XCTAssertEqual(echoApiResponse(response: success), success)
        demoCase("case:enums.complex_variants.api_response.redirect.should_roundtrip_url_payload")
        XCTAssertEqual(echoApiResponse(response: redirect), redirect)
        demoCase("case:enums.complex_variants.api_response.success.should_identify_success")
        XCTAssertEqual(isSuccess(response: success), true)
        demoCase("case:enums.complex_variants.api_response.empty.should_not_identify_as_success")
        XCTAssertEqual(isSuccess(response: .empty), false)
    }
}
