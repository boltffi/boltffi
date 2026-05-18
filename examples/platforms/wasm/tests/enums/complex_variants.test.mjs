import { assert, demo } from "../support/index.mjs";

export async function run() {
  const nameFilter = { tag: "ByName", name: "ali" };
  const pointFilter = {
    tag: "ByPoints",
    anchors: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
  };
  const groupFilter = {
    tag: "ByGroups",
    groups: [["café", "🌍"], [], ["common"]],
  };

  globalThis.demoCase("case:enums.complex_variants.filter.none.should_roundtrip_unit_variant");
  assert.deepEqual(demo.echoFilter({ tag: "None" }), { tag: "None" });
  globalThis.demoCase("case:enums.complex_variants.filter.by_name.should_roundtrip_string_payload");
  assert.deepEqual(demo.echoFilter(nameFilter), nameFilter);
  globalThis.demoCase("case:enums.complex_variants.filter.by_tags.should_roundtrip_string_vector_payload");
  const tagFilter = { tag: "ByTags", tags: ["ffi", "jni", "café"] };
  assert.deepEqual(demo.echoFilter(tagFilter), tagFilter);
  globalThis.demoCase("case:enums.complex_variants.filter.by_points.should_roundtrip_record_vector_payload");
  assert.deepEqual(demo.echoFilter(pointFilter), pointFilter);
  globalThis.demoCase("case:enums.complex_variants.filter.by_groups.should_roundtrip_nested_string_vectors");
  assert.deepEqual(demo.echoFilter(groupFilter), groupFilter);
  globalThis.demoCase("case:enums.complex_variants.filter.by_name.should_describe_string_payload");
  assert.equal(demo.describeFilter(nameFilter), "filter by name: ali");
  globalThis.demoCase("case:enums.complex_variants.filter.by_points.should_describe_record_vector_payload");
  assert.equal(demo.describeFilter(pointFilter), "filter by 2 anchor points");
  globalThis.demoCase("case:enums.complex_variants.filter.by_tags.should_describe_string_vector_payload");
  assert.equal(demo.describeFilter({ tag: "ByTags", tags: ["ffi", "jni"] }), "filter by 2 tags");
  globalThis.demoCase("case:enums.complex_variants.filter.by_groups.should_describe_nested_string_vectors");
  assert.equal(demo.describeFilter(groupFilter), "filter by 3 groups");
  globalThis.demoCase("case:enums.complex_variants.filter.by_range.should_describe_numeric_bounds");
  assert.equal(demo.describeFilter({ tag: "ByRange", min: 1, max: 5 }), "filter by range: 1..5");

  const success = { tag: "Success", data: "ok" };
  const redirect = { tag: "Redirect", url: "https://example.com" };
  globalThis.demoCase("case:enums.complex_variants.api_response.success.should_roundtrip_string_payload");
  assert.deepEqual(demo.echoApiResponse(success), success);
  globalThis.demoCase("case:enums.complex_variants.api_response.redirect.should_roundtrip_url_payload");
  assert.deepEqual(demo.echoApiResponse(redirect), redirect);
  globalThis.demoCase("case:enums.complex_variants.api_response.success.should_identify_success");
  assert.equal(demo.isSuccess(success), true);
  globalThis.demoCase("case:enums.complex_variants.api_response.empty.should_not_identify_as_success");
  assert.equal(demo.isSuccess({ tag: "Empty" }), false);
}
