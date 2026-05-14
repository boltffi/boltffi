import { assert, assertArrayEqual, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:enums.repr_int.priority.basic");
  assert.equal(demo.echoPriority(demo.Priority.High), demo.Priority.High);
  assert.equal(demo.priorityLabel(demo.Priority.Low), "low");
  assert.equal(demo.isHighPriority(demo.Priority.Critical), true);
  assert.equal(demo.isHighPriority(demo.Priority.Low), false);
  globalThis.demoCase("case:enums.repr_int.log_level.basic");
  assert.equal(demo.echoLogLevel(demo.LogLevel.Info), demo.LogLevel.Info);
  assert.equal(demo.shouldLog(demo.LogLevel.Error, demo.LogLevel.Warn), true);
  globalThis.demoCase("case:enums.repr_int.log_level.vec");
  assertArrayEqual(
    demo.echoVecLogLevel(Uint8Array.from([demo.LogLevel.Trace, demo.LogLevel.Info, demo.LogLevel.Error])),
    [demo.LogLevel.Trace, demo.LogLevel.Info, demo.LogLevel.Error],
  );

  globalThis.demoCase("case:enums.repr_int.http_code.discriminants");
  assert.equal(demo.HttpCode.Ok, 200);
  assert.equal(demo.HttpCode.NotFound, 404);
  assert.equal(demo.HttpCode.ServerError, 500);
  assert.equal(demo.httpCodeNotFound(), demo.HttpCode.NotFound);
  assert.equal(demo.echoHttpCode(demo.HttpCode.Ok), demo.HttpCode.Ok);
  assert.equal(demo.echoHttpCode(demo.HttpCode.ServerError), demo.HttpCode.ServerError);

  globalThis.demoCase("case:enums.repr_int.sign.discriminants");
  assert.equal(demo.Sign.Negative, -1);
  assert.equal(demo.Sign.Zero, 0);
  assert.equal(demo.Sign.Positive, 1);
  assert.equal(demo.signNegative(), demo.Sign.Negative);
  assert.equal(demo.echoSign(demo.Sign.Negative), demo.Sign.Negative);
  assert.equal(demo.echoSign(demo.Sign.Positive), demo.Sign.Positive);
}
