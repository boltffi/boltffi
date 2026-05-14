import { assert, assertApprox, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:records.nested.line.basic");
  const line = demo.makeLine(0, 0, 3, 4);
  assert.deepEqual(demo.echoLine(line), line);
  assertApprox(demo.lineLength(line), 5, 1e-12);

  globalThis.demoCase("case:records.nested.rect.basic");
  const rect = {
    origin: { x: 1, y: 2 },
    dimensions: { width: 3, height: 4 },
  };
  assert.deepEqual(demo.echoRect(rect), rect);
  assertApprox(demo.rectArea(rect), 12, 1e-12);
}
