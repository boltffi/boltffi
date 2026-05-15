import { assert, assertArrayEqual, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:enums.c_style.status.should_roundtrip_values");
  assert.equal(demo.echoStatus(demo.Status.Active), demo.Status.Active);
  globalThis.demoCase("case:enums.c_style.status.should_render_labels");
  assert.equal(demo.statusToString(demo.Status.Active), "active");
  globalThis.demoCase("case:enums.c_style.status.should_identify_active_values");
  assert.equal(demo.isActive(demo.Status.Pending), false);
  globalThis.demoCase("case:enums.c_style.status.should_roundtrip_vectors");
  assertArrayEqual(demo.echoVecStatus([demo.Status.Active, demo.Status.Pending]), [demo.Status.Active, demo.Status.Pending]);
  globalThis.demoCase("case:enums.c_style.direction.should_roundtrip_value");
  assert.equal(demo.echoDirection(demo.Direction.East), demo.Direction.East);
  globalThis.demoCase("case:enums.c_style.direction.should_return_opposite_from_free_function");
  assert.equal(demo.oppositeDirection(demo.Direction.East), demo.Direction.West);
  globalThis.demoCase("case:enums.c_style.direction.should_construct_from_raw_value");
  assert.equal(demo.Direction.fromRaw(2), demo.Direction.East);
  globalThis.demoCase("case:enums.c_style.direction.should_return_cardinal_value");
  assert.equal(demo.Direction.cardinal(), demo.Direction.North);
  globalThis.demoCase("case:enums.c_style.direction.should_construct_from_degrees");
  assert.equal(demo.Direction.fromDegrees(90), demo.Direction.East);
  globalThis.demoCase("case:enums.c_style.direction.should_return_opposite_from_method");
  assert.equal(demo.Direction.opposite(demo.Direction.East), demo.Direction.West);
  globalThis.demoCase("case:enums.c_style.direction.should_identify_horizontal_values");
  assert.equal(demo.Direction.isHorizontal(demo.Direction.East), true);
  globalThis.demoCase("case:enums.c_style.direction.should_render_compass_label");
  assert.equal(demo.Direction.label(demo.Direction.South), "S");
  globalThis.demoCase("case:enums.c_style.direction.should_report_variant_count");
  assert.equal(demo.Direction.count(), 4);
}
