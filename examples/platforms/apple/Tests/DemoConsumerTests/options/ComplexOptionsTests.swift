import Demo
import XCTest

final class ComplexOptionsTests: DemoTestCase {
    func testComplexOptionFns() {
        demoCase("case:options.complex.string.should_roundtrip_some")
        XCTAssertEqual(echoOptionalString(v: "hello"), "hello")
        demoCase("case:options.complex.string.should_roundtrip_none")
        XCTAssertNil(echoOptionalString(v: nil))
        demoCase("case:options.complex.string.should_report_some")
        XCTAssertEqual(isSomeString(v: "x"), true)
        demoCase("case:options.complex.string.should_report_none")
        XCTAssertEqual(isSomeString(v: nil), false)

        demoCase("case:options.complex.point.should_roundtrip_some")
        XCTAssertEqual(echoOptionalPoint(v: Point(x: 1.0, y: 2.0)), Point(x: 1.0, y: 2.0))
        demoCase("case:options.complex.point.should_roundtrip_none")
        XCTAssertNil(echoOptionalPoint(v: nil))
        demoCase("case:options.complex.point.should_make_some")
        XCTAssertEqual(makeSomePoint(x: 3.0, y: 4.0), Point(x: 3.0, y: 4.0))
        demoCase("case:options.complex.point.should_make_none")
        XCTAssertNil(makeNonePoint())

        demoCase("case:options.complex.status.should_roundtrip_some")
        XCTAssertEqual(echoOptionalStatus(v: .active), .active)
        demoCase("case:options.complex.status.should_roundtrip_none")
        XCTAssertNil(echoOptionalStatus(v: nil))
        demoCase("case:options.complex.vec.should_roundtrip_some")
        XCTAssertEqual(echoOptionalVec(v: [1, 2, 3]), [1, 2, 3])
        demoCase("case:options.complex.vec.should_roundtrip_empty_some")
        XCTAssertEqual(echoOptionalVec(v: []), [])
        demoCase("case:options.complex.vec.should_roundtrip_none")
        XCTAssertNil(echoOptionalVec(v: nil))
        demoCase("case:options.complex.vec.should_report_length_for_some")
        XCTAssertEqual(optionalVecLength(v: [9, 8]), 2)
        demoCase("case:options.complex.vec.should_return_none_for_absent_length")
        XCTAssertNil(optionalVecLength(v: nil))
        demoCase("case:options.complex.string.should_find_name_for_positive_id")
        XCTAssertEqual(findName(id: 1), "Name_1")
        demoCase("case:options.complex.string.should_return_none_for_non_positive_id")
        XCTAssertNil(findName(id: 0))
        demoCase("case:options.complex.vec.should_find_numbers_for_positive_count")
        XCTAssertEqual(findNumbers(count: 3), [0, 1, 2])
        demoCase("case:options.complex.vec.should_return_none_for_non_positive_number_count")
        XCTAssertNil(findNumbers(count: 0))
        demoCase("case:options.complex.vec_string.should_find_names_for_positive_count")
        XCTAssertEqual(findNames(count: 2), ["Name_0", "Name_1"])
        demoCase("case:options.complex.vec_string.should_return_none_for_non_positive_name_count")
        XCTAssertNil(findNames(count: 0))
        demoCase("case:options.complex.api_result.should_find_success_variant")
        XCTAssertEqual(findApiResult(code: 0), .success)
        demoCase("case:options.complex.api_result.should_find_error_code_variant")
        XCTAssertEqual(findApiResult(code: 1), .errorCode(-1))
        demoCase("case:options.complex.api_result.should_find_error_with_data_variant")
        XCTAssertEqual(findApiResult(code: 2), .errorWithData(code: -1, detail: -2))
        demoCase("case:options.complex.api_result.should_return_none_for_unknown_code")
        XCTAssertNil(findApiResult(code: 99))

        // Vec<Option<T>>: each element carries its own present/absent
        // tag, exercising the encoded-array path composed with the
        // Option codec inside a single wire payload.
        demoCase("case:options.complex.vec_optional_i32.should_roundtrip_mixed_presence")
        let mixed: [Int32?] = [1, nil, 3, nil, 5]
        XCTAssertEqual(echoVecOptionalI32(v: mixed), mixed)
        demoCase("case:options.complex.vec_optional_i32.should_roundtrip_empty")
        let empty: [Int32?] = []
        XCTAssertEqual(echoVecOptionalI32(v: empty), empty)
        demoCase("case:options.complex.vec_optional_i32.should_roundtrip_all_none")
        let allNone: [Int32?] = [nil, nil, nil]
        XCTAssertEqual(echoVecOptionalI32(v: allNone), allNone)
    }
}
