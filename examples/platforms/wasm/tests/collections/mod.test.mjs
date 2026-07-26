import { assert, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:collections.hash_map.should_return_values");
  const produced = demo.makeHashMap();
  assert.equal(produced.size, 2);
  assert.equal(produced.get("first"), 10);
  assert.equal(produced.get("second"), 20);

  globalThis.demoCase("case:collections.hash_map.should_roundtrip_empty");
  assert.deepEqual(demo.echoHashMap(new Map()), new Map());

  globalThis.demoCase("case:collections.hash_map.should_roundtrip_nested_values");
  const echoed = demo.echoHashMap(
    new Map([
      ["odd", [1, 3, 5]],
      ["even", [2, 4]],
    ])
  );
  assert.deepEqual(Array.from(echoed.get("odd")), [1, 3, 5]);
  assert.deepEqual(Array.from(echoed.get("even")), [2, 4]);
}
