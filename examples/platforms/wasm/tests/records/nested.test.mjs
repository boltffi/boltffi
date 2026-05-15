import { assert, assertApprox, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:records.nested.line.should_make_from_coordinates");
  const line = demo.makeLine(0, 0, 3, 4);
  globalThis.demoCase("case:records.nested.line.should_roundtrip_nested_points");
  assert.deepEqual(demo.echoLine(line), line);
  globalThis.demoCase("case:records.nested.line.should_compute_length");
  assertApprox(demo.lineLength(line), 5, 1e-12);

  globalThis.demoCase("case:records.nested.rect.should_roundtrip_nested_records");
  const rect = {
    origin: { x: 1, y: 2 },
    dimensions: { width: 3, height: 4 },
  };
  assert.deepEqual(demo.echoRect(rect), rect);
  globalThis.demoCase("case:records.nested.rect.should_compute_area");
  assertApprox(demo.rectArea(rect), 12, 1e-12);
}
