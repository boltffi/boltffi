import { assert, assertArrayEqual, assertPoint, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:options.complex.string.should_roundtrip_some");
  assert.equal(demo.echoOptionalString("hello"), "hello");
  globalThis.demoCase("case:options.complex.string.should_roundtrip_none");
  assert.equal(demo.echoOptionalString(null), null);
  globalThis.demoCase("case:options.complex.string.should_report_some");
  assert.equal(demo.isSomeString("x"), true);
  globalThis.demoCase("case:options.complex.string.should_report_none");
  assert.equal(demo.isSomeString(null), false);

  globalThis.demoCase("case:options.complex.point.should_roundtrip_some");
  assertPoint(demo.echoOptionalPoint({ x: 1, y: 2 }), { x: 1, y: 2 });
  globalThis.demoCase("case:options.complex.point.should_roundtrip_none");
  assert.equal(demo.echoOptionalPoint(null), null);
  globalThis.demoCase("case:options.complex.point.should_make_some");
  assertPoint(demo.makeSomePoint(3, 4), { x: 3, y: 4 });
  globalThis.demoCase("case:options.complex.point.should_make_none");
  assert.equal(demo.makeNonePoint(), null);

  globalThis.demoCase("case:options.complex.status.should_roundtrip_some");
  assert.equal(demo.echoOptionalStatus(demo.Status.Active), demo.Status.Active);
  globalThis.demoCase("case:options.complex.status.should_roundtrip_none");
  assert.equal(demo.echoOptionalStatus(null), null);
  globalThis.demoCase("case:options.complex.vec.should_roundtrip_some");
  assertArrayEqual(demo.echoOptionalVec([1, 2, 3]), [1, 2, 3]);
  globalThis.demoCase("case:options.complex.vec.should_roundtrip_none");
  assert.equal(demo.echoOptionalVec(null), null);
  globalThis.demoCase("case:options.complex.vec.should_roundtrip_empty_some");
  assertArrayEqual(demo.echoOptionalVec([]), []);
  globalThis.demoCase("case:options.complex.vec.should_report_length_for_some");
  assert.equal(demo.optionalVecLength([9, 8]), 2);
  globalThis.demoCase("case:options.complex.vec.should_return_none_for_absent_length");
  assert.equal(demo.optionalVecLength(null), null);
  globalThis.demoCase("case:options.complex.string.should_find_name_for_positive_id");
  assert.equal(demo.findName(7), "Name_7");
  globalThis.demoCase("case:options.complex.string.should_return_none_for_non_positive_id");
  assert.equal(demo.findName(0), null);
  globalThis.demoCase("case:options.complex.vec.should_find_numbers_for_positive_count");
  assertArrayEqual(demo.findNumbers(3), [0, 1, 2]);
  globalThis.demoCase("case:options.complex.vec.should_return_none_for_non_positive_number_count");
  assert.equal(demo.findNumbers(0), null);
  globalThis.demoCase("case:options.complex.vec_string.should_find_names_for_positive_count");
  assertArrayEqual(demo.findNames(2), ["Name_0", "Name_1"]);
  globalThis.demoCase("case:options.complex.vec_string.should_return_none_for_non_positive_name_count");
  assert.equal(demo.findNames(0), null);
  globalThis.demoCase("case:options.complex.api_result.should_find_success_variant");
  assert.deepEqual(demo.findApiResult(0), { tag: "Success" });
  globalThis.demoCase("case:options.complex.api_result.should_find_error_code_variant");
  assert.deepEqual(demo.findApiResult(1), { tag: "ErrorCode", value0: -1 });
  globalThis.demoCase("case:options.complex.api_result.should_find_error_with_data_variant");
  assert.deepEqual(demo.findApiResult(2), { tag: "ErrorWithData", code: -1, detail: -2 });
  globalThis.demoCase("case:options.complex.api_result.should_return_none_for_unknown_code");
  assert.equal(demo.findApiResult(99), null);

  globalThis.demoCase("case:options.complex.vec_optional_i32.should_roundtrip_mixed_presence");
  assert.deepEqual(demo.echoVecOptionalI32([1, null, 2, null, 3]), [1, null, 2, null, 3]);
  globalThis.demoCase("case:options.complex.vec_optional_i32.should_roundtrip_empty");
  assert.deepEqual(demo.echoVecOptionalI32([]), []);
  globalThis.demoCase("case:options.complex.vec_optional_i32.should_roundtrip_all_none");
  assert.deepEqual(demo.echoVecOptionalI32([null, null, null]), [null, null, null]);

  globalThis.demoCase("case:options.complex.shape.should_return_radius_for_circle");
  assert.equal(demo.radiusIfCircle({ tag: "Circle", radius: 5 }), 5);
  globalThis.demoCase("case:options.complex.shape.should_return_none_for_non_circle");
  assert.equal(demo.radiusIfCircle({ tag: "Rectangle", width: 3, height: 4 }), null);
}
