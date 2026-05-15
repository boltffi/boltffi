import { assert, assertArrayEqual, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:enums.repr_int.priority.should_roundtrip_value");
  assert.equal(demo.echoPriority(demo.Priority.High), demo.Priority.High);
  globalThis.demoCase("case:enums.repr_int.priority.should_render_label");
  assert.equal(demo.priorityLabel(demo.Priority.Low), "low");
  globalThis.demoCase("case:enums.repr_int.priority.should_identify_high_priority");
  assert.equal(demo.isHighPriority(demo.Priority.Critical), true);
  assert.equal(demo.isHighPriority(demo.Priority.Low), false);
  globalThis.demoCase("case:enums.repr_int.log_level.should_roundtrip_value");
  assert.equal(demo.echoLogLevel(demo.LogLevel.Info), demo.LogLevel.Info);
  globalThis.demoCase("case:enums.repr_int.log_level.should_compare_against_minimum");
  assert.equal(demo.shouldLog(demo.LogLevel.Error, demo.LogLevel.Warn), true);
  globalThis.demoCase("case:enums.repr_int.log_level.should_roundtrip_vectors");
  assertArrayEqual(
    demo.echoVecLogLevel(Uint8Array.from([demo.LogLevel.Trace, demo.LogLevel.Info, demo.LogLevel.Error])),
    [demo.LogLevel.Trace, demo.LogLevel.Info, demo.LogLevel.Error],
  );

  globalThis.demoCase("case:enums.repr_int.http_code.should_expose_discriminant_values");
  assert.equal(demo.HttpCode.Ok, 200);
  assert.equal(demo.HttpCode.NotFound, 404);
  assert.equal(demo.HttpCode.ServerError, 500);
  globalThis.demoCase("case:enums.repr_int.http_code.should_return_not_found");
  assert.equal(demo.httpCodeNotFound(), demo.HttpCode.NotFound);
  globalThis.demoCase("case:enums.repr_int.http_code.should_roundtrip_values");
  assert.equal(demo.echoHttpCode(demo.HttpCode.Ok), demo.HttpCode.Ok);
  assert.equal(demo.echoHttpCode(demo.HttpCode.ServerError), demo.HttpCode.ServerError);

  globalThis.demoCase("case:enums.repr_int.sign.should_expose_signed_discriminant_values");
  assert.equal(demo.Sign.Negative, -1);
  assert.equal(demo.Sign.Zero, 0);
  assert.equal(demo.Sign.Positive, 1);
  globalThis.demoCase("case:enums.repr_int.sign.should_return_negative");
  assert.equal(demo.signNegative(), demo.Sign.Negative);
  globalThis.demoCase("case:enums.repr_int.sign.should_roundtrip_signed_values");
  assert.equal(demo.echoSign(demo.Sign.Negative), demo.Sign.Negative);
  assert.equal(demo.echoSign(demo.Sign.Positive), demo.Sign.Positive);
}
