import { assert, assertApprox, assertArrayEqual, assertPoint, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:records.with_collections.polygon.should_make_from_points");
  const polygon = demo.makePolygon([{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 0, y: 1 }]);
  globalThis.demoCase("case:records.with_collections.polygon.should_roundtrip_point_vector");
  assert.deepEqual(demo.echoPolygon(polygon), polygon);
  globalThis.demoCase("case:records.with_collections.polygon.should_report_vertex_count");
  assert.equal(demo.polygonVertexCount(polygon), 3);
  globalThis.demoCase("case:records.with_collections.polygon.should_compute_centroid");
  assertPoint(demo.polygonCentroid(polygon), { x: 1 / 3, y: 1 / 3 }, 1e-6);

  globalThis.demoCase("case:records.with_collections.team.should_make_from_members");
  const team = demo.makeTeam("devs", ["Ali", "Mia"]);
  globalThis.demoCase("case:records.with_collections.team.should_roundtrip_member_vector");
  assert.deepEqual(demo.echoTeam(team), team);
  globalThis.demoCase("case:records.with_collections.team.should_report_member_count");
  assert.equal(demo.teamSize(team), 2);

  globalThis.demoCase("case:records.with_collections.classroom.should_make_from_students");
  const classroom = demo.makeClassroom([{ name: "Mia", age: 10 }, { name: "Leo", age: 11 }]);
  globalThis.demoCase("case:records.with_collections.classroom.should_roundtrip_student_vector");
  assert.deepEqual(demo.echoClassroom(classroom), classroom);

  globalThis.demoCase("case:records.with_collections.tagged_scores.should_roundtrip_score_vector");
  const taggedScores = { label: "math", scores: [90, 85.5] };
  const echoedTaggedScores = demo.echoTaggedScores(taggedScores);
  assert.equal(echoedTaggedScores.label, "math");
  assertArrayEqual(echoedTaggedScores.scores, [90, 85.5]);
  globalThis.demoCase("case:records.with_collections.tagged_scores.should_average_scores");
  assertApprox(demo.averageScore({ label: "x", scores: [80, 100] }), 90, 1e-12);
}
