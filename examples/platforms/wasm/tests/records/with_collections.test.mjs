import { assert, assertApprox, assertArrayEqual, assertPoint, demo } from "../support/index.mjs";

export async function run() {
  // case:records.with_collections.polygon.basic
  const polygon = demo.makePolygon([{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 0, y: 1 }]);
  assert.deepEqual(demo.echoPolygon(polygon), polygon);
  assert.equal(demo.polygonVertexCount(polygon), 3);
  assertPoint(demo.polygonCentroid(polygon), { x: 1 / 3, y: 1 / 3 }, 1e-6);

  // case:records.with_collections.team.basic
  const team = demo.makeTeam("devs", ["Ali", "Mia"]);
  assert.deepEqual(demo.echoTeam(team), team);
  assert.equal(demo.teamSize(team), 2);

  // case:records.with_collections.classroom.basic
  const classroom = demo.makeClassroom([{ name: "Mia", age: 10 }, { name: "Leo", age: 11 }]);
  assert.deepEqual(demo.echoClassroom(classroom), classroom);

  // case:records.with_collections.tagged_scores.basic
  const taggedScores = { label: "math", scores: [90, 85.5] };
  const echoedTaggedScores = demo.echoTaggedScores(taggedScores);
  assert.equal(echoedTaggedScores.label, "math");
  assertArrayEqual(echoedTaggedScores.scores, [90, 85.5]);
  assertApprox(demo.averageScore({ label: "x", scores: [80, 100] }), 90, 1e-12);
}
