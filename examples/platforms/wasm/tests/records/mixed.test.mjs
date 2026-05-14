import { assert, demo, sampleMixedRecord } from "../support/index.mjs";

export async function run() {
  // case:records.mixed.basic
  const record = sampleMixedRecord();

  assert.deepEqual(demo.echoMixedRecord(record), record);
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
