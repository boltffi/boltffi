import { wireErr, wireOk } from "@boltffi/runtime";
import { assert, assertThrowsWithCode, demo } from "../support/index.mjs";

export async function run() {
  const drops = demo.MessageDrops.new();
  const receiver = {
    first: null,
    second: null,
    fail: false,
    attach(handle, callback) {
      assert.equal(callback, 42);
      this.first = handle;
    },
    optional(handle) { this.first = handle; },
    pair(first, label, second) {
      assert.equal(label, "pair");
      this.first = first;
      this.second = second;
      return this.fail ? wireErr(demo.MathError.NegativeInput) : wireOk(first.length() + (second?.length() ?? 0));
    },
  };

  globalThis.demoCase("case:callbacks.class_handles.should_retain_after_return");
  demo.deliverMessage(receiver, drops);
  assert.equal(drops.count(), 0);
  assert.equal(receiver.first.length(), 9);
  receiver.first.dispose();
  receiver.first.dispose();
  assert.equal(drops.count(), 1);

  globalThis.demoCase("case:callbacks.class_handles.should_deliver_multiple_and_optional");
  assert.equal(demo.deliverMessagePair(receiver, drops, true), 11);
  assert.equal(drops.count(), 1);
  assert.equal(receiver.first.length(), 5);
  assert.equal(receiver.second.length(), 6);
  receiver.first.dispose();
  assert.equal(drops.count(), 2);
  receiver.second.dispose();
  assert.equal(drops.count(), 3);
  assert.equal(demo.deliverMessagePair(receiver, drops, false), 5);
  assert.equal(receiver.second, null);
  receiver.first.dispose();
  assert.equal(drops.count(), 4);

  globalThis.demoCase("case:callbacks.class_handles.should_retain_after_error");
  receiver.fail = true;
  assertThrowsWithCode(() => demo.deliverMessagePair(receiver, drops, true), demo.MathErrorException, demo.MathError.NegativeInput);
  assert.equal(drops.count(), 4);
  assert.equal(receiver.first.length(), 5);
  assert.equal(receiver.second.length(), 6);
  receiver.first.dispose();
  receiver.second.dispose();
  assert.equal(drops.count(), 6);

  globalThis.demoCase("case:callbacks.class_handles.should_consume_in_rust_callback");
  const measuring = demo.makeMessageReceiver();
  const moved = demo.OwnedMessage.new("moved", drops);
  measuring.attach(moved, 42);
  assert.throws(() => moved.length(), /disposed/);
  moved.dispose();
  assert.equal(drops.count(), 7);
  const optional = demo.OwnedMessage.new("optional", drops);
  measuring.optional(optional);
  optional.dispose();
  measuring.optional(null);
  assert.equal(drops.count(), 8);
  measuring.dispose();

  const rejected = demo.OwnedMessage.new("undelivered", drops);
  assert.throws(() => measuring.attach(rejected, 42), /disposed/);
  rejected.dispose();
  assert.equal(drops.count(), 9);

  assert.throws(() => demo.deliverMessagePair({}, drops, true));
  assert.equal(drops.count(), 11);
  assert.throws(() => demo.deliverMessagePair({
    get pair() { throw new Error("method lookup failed"); },
  }, drops, true));
  assert.equal(drops.count(), 13);

  const fromHandle = demo.OwnedMessage._fromHandle;
  let wrapped = 0;
  demo.OwnedMessage._fromHandle = function (handle) {
    if (++wrapped === 2) throw new Error("second wrapper failed");
    return fromHandle.call(this, handle);
  };
  try {
    assert.throws(() => demo.deliverMessagePair(receiver, drops, true));
    assert.equal(wrapped, 2);
    assert.equal(drops.count(), 15);
  } finally {
    demo.OwnedMessage._fromHandle = fromHandle;
  }
  drops.dispose();
}
