import { assert, assertArrayEqual, demo } from "../support/index.mjs";

const DIRECTION_CYCLE = [
  demo.Direction.North,
  demo.Direction.East,
  demo.Direction.South,
  demo.Direction.West,
];

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
  globalThis.demoCase("case:enums.c_style.direction.should_return_degrees");
  assert.equal(demo.directionToDegrees(demo.Direction.East), 90);
  globalThis.demoCase("case:enums.c_style.direction.should_generate_sequence");
  assertArrayEqual(demo.generateDirections(5), [
    demo.Direction.North,
    demo.Direction.East,
    demo.Direction.South,
    demo.Direction.West,
    demo.Direction.North,
  ]);
  globalThis.demoCase("case:enums.c_style.direction.should_count_north_values");
  assert.equal(demo.countNorth(DIRECTION_CYCLE), 1);
  globalThis.demoCase("case:enums.c_style.direction.find_direction.should_return_some_for_known_id");
  assert.equal(demo.findDirection(2), demo.Direction.South);
  globalThis.demoCase("case:enums.c_style.direction.find_direction.should_return_none_for_unknown_id");
  assert.equal(demo.findDirection(99), null);
  globalThis.demoCase("case:enums.c_style.direction.find_directions.should_return_sequence_for_positive_count");
  assertArrayEqual(demo.findDirections(3), [
    demo.Direction.North,
    demo.Direction.East,
    demo.Direction.South,
  ]);
  globalThis.demoCase("case:enums.c_style.direction.find_directions.should_return_none_for_non_positive_count");
  assert.equal(demo.findDirections(0), null);
}
