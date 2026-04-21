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

  assert.deepEqual(demo.echoFilter({ tag: "None" }), { tag: "None" });
  assert.deepEqual(demo.echoFilter(nameFilter), nameFilter);
  assert.deepEqual(demo.echoFilter(groupFilter), groupFilter);
  assert.equal(demo.describeFilter(nameFilter), "filter by name: ali");
  assert.equal(demo.describeFilter(pointFilter), "filter by 2 anchor points");
  assert.equal(demo.describeFilter({ tag: "ByTags", tags: ["ffi", "jni"] }), "filter by 2 tags");
  assert.equal(demo.describeFilter(groupFilter), "filter by 3 groups");
  assert.equal(demo.describeFilter({ tag: "ByRange", min: 1, max: 5 }), "filter by range: 1..5");

  const success = { tag: "Success", data: "ok" };
  const redirect = { tag: "Redirect", url: "https://example.com" };
  assert.deepEqual(demo.echoApiResponse(success), success);
  assert.deepEqual(demo.echoApiResponse(redirect), redirect);
  assert.equal(demo.isSuccess(success), true);
  assert.equal(demo.isSuccess({ tag: "Empty" }), false);
}
