import { assert, demo } from "./support/index.mjs";

export async function run() {
  assert.equal(demo.wasmTrue, true);
  assert.equal(demo.wasmFalse, false);
  const uuids = new Set(Array.from({ length: 10_000 }, () => demo.wasmUuidV4()));
  assert.equal(uuids.size, 10_000);
  uuids.forEach((uuid) => assert.match(uuid, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/));
  assert.match(demo.wasmUuidV7(), /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);

  const before = BigInt(Date.now());
  const timestamp = demo.wasmCurrentTime();
  assert.ok(timestamp >= before && timestamp <= BigInt(Date.now()));
  assert.equal(demo.wasmLocalOffset(), -new Date().getTimezoneOffset() * 60);
  assert.equal(demo.wasmStartCount(), 1);
  assert.equal(globalThis.boltffiStartupCount, 1);
  assert.equal(demo.wasmLocalSnippet(41), 42);
  assert.equal(demo.wasmInlineSnippet(), 73);

  Array.from({ length: 1_000 }, (_, index) => {
    assert.equal(demo.wasmJsonAccepts('{"value":42}'), true);
    assert.equal(demo.wasmJsonAccepts("{broken"), false);
    assert.equal(demo.wasmJsClosure(index), index * 3);
  });
  const messages = Array.from({ length: 32 }, (_, index) => `promise ${index}: 日本語 🦀`);
  assert.deepEqual(await Promise.all(messages.map((message) => demo.wasmAwaitPromise(message))), messages);

  Array.from({ length: 200_000 }, () => {
    assert.equal(demo.wasmUuidV4().length, 36);
  });
  const largeString = "🦀".repeat(2_000_000);
  assert.equal(demo.echoString(largeString), largeString);
  assert.equal(demo.wasmJsClosure(7), 21);
  assert.equal(demo.wasmUuidV4().length, 36);
  assert.equal(await demo.wasmAwaitPromise("after memory growth"), "after memory growth");
  const trapped = await Promise.allSettled([
    demo.wasmAsyncPanic(),
    demo.wasmAwaitPromise("pending during a trap"),
  ]);
  trapped.forEach((result) => {
    assert.equal(result.status, "rejected");
    assert.ok(result.reason instanceof WebAssembly.RuntimeError);
  });
}
