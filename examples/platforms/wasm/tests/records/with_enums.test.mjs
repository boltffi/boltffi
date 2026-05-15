import { assert, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:records.with_enums.task.should_make_incomplete_task");
  const task = demo.makeTask("ship bindings", demo.Priority.Critical);
  assert.equal(task.completed, false);
  globalThis.demoCase("case:records.with_enums.task.should_detect_urgent_priority");
  assert.equal(demo.isUrgent(task), true);

  globalThis.demoCase("case:records.with_enums.task.should_roundtrip_priority_field");
  assert.deepEqual(demo.echoTask(task), task);

  globalThis.demoCase("case:records.with_enums.notification.should_roundtrip_priority_field");
  const notification = { message: "heads up", priority: demo.Priority.High, read: false };
  assert.deepEqual(demo.echoNotification(notification), notification);

  globalThis.demoCase("case:records.with_enums.holder.should_make_triangle_variant");
  const triangle = demo.makeTriangleHolder();
  assert.equal(triangle.shape.tag, "Triangle");
  globalThis.demoCase("case:records.with_enums.holder.should_roundtrip_data_enum_field");
  assert.deepEqual(demo.echoHolder(triangle), triangle);

  globalThis.demoCase("case:records.with_enums.task_header.should_make_critical_header");
  const header = demo.makeCriticalTaskHeader(42n);
  assert.equal(header.id, 42n);
  assert.equal(header.priority, demo.Priority.Critical);
  assert.equal(header.completed, false);
  globalThis.demoCase("case:records.with_enums.task_header.should_roundtrip_repr_enum_field");
  assert.deepEqual(demo.echoTaskHeader(header), header);

  globalThis.demoCase("case:records.with_enums.log_entry.should_make_error_entry");
  const logEntry = demo.makeErrorLogEntry(1234567890n, 42);
  assert.equal(logEntry.timestamp, 1234567890n);
  assert.equal(logEntry.level, demo.LogLevel.Error);
  assert.equal(logEntry.code, 42);
  globalThis.demoCase("case:records.with_enums.log_entry.should_roundtrip_u8_enum_field");
  assert.deepEqual(demo.echoLogEntry(logEntry), logEntry);
}
