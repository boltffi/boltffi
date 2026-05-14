import { assert, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:records.with_enums.task.make_urgent");
  const task = demo.makeTask("ship bindings", demo.Priority.Critical);
  assert.equal(task.completed, false);
  assert.equal(demo.isUrgent(task), true);

  globalThis.demoCase("case:records.with_enums.task.echo");
  assert.deepEqual(demo.echoTask(task), task);

  globalThis.demoCase("case:records.with_enums.notification.echo");
  const notification = { message: "heads up", priority: demo.Priority.High, read: false };
  assert.deepEqual(demo.echoNotification(notification), notification);

  globalThis.demoCase("case:records.with_enums.holder.triangle");
  const triangle = demo.makeTriangleHolder();
  assert.equal(triangle.shape.tag, "Triangle");
  assert.deepEqual(demo.echoHolder(triangle), triangle);

  globalThis.demoCase("case:records.with_enums.task_header.roundtrip");
  const header = demo.makeCriticalTaskHeader(42n);
  assert.equal(header.id, 42n);
  assert.equal(header.priority, demo.Priority.Critical);
  assert.equal(header.completed, false);
  assert.deepEqual(demo.echoTaskHeader(header), header);

  globalThis.demoCase("case:records.with_enums.log_entry.roundtrip");
  const logEntry = demo.makeErrorLogEntry(1234567890n, 42);
  assert.equal(logEntry.timestamp, 1234567890n);
  assert.equal(logEntry.level, demo.LogLevel.Error);
  assert.equal(logEntry.code, 42);
  assert.deepEqual(demo.echoLogEntry(logEntry), logEntry);
}
