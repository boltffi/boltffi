import { assert, demo, sampleMixedRecord } from "../support/index.mjs";

export async function run() {
  const record = sampleMixedRecord();

  globalThis.demoCase("case:records.mixed.should_roundtrip_composed_record");
  assert.deepEqual(demo.echoMixedRecord(record), record);
  globalThis.demoCase("case:records.mixed.should_make_from_composed_parts");
  assert.deepEqual(
    demo.makeMixedRecord(
      record.name,
      record.anchor,
      record.priority,
      record.shape,
      record.parameters,
    ),
    record,
  );
}
