import { assert, assertArrayEqual, assertPoint, demo } from "../support/index.mjs";

export async function run() {
  // case:options.complex.string.some_none
  assert.equal(demo.echoOptionalString("hello"), "hello");
  assert.equal(demo.echoOptionalString(null), null);
  assert.equal(demo.isSomeString("x"), true);
  assert.equal(demo.isSomeString(null), false);

  // case:options.complex.point.some_none
  assertPoint(demo.echoOptionalPoint({ x: 1, y: 2 }), { x: 1, y: 2 });
  assert.equal(demo.echoOptionalPoint(null), null);
  assertPoint(demo.makeSomePoint(3, 4), { x: 3, y: 4 });
  assert.equal(demo.makeNonePoint(), null);

  // case:options.complex.status.some_none
  assert.equal(demo.echoOptionalStatus(demo.Status.Active), demo.Status.Active);
  assert.equal(demo.echoOptionalStatus(null), null);
  // case:options.complex.vec.some_none
  assertArrayEqual(demo.echoOptionalVec([1, 2, 3]), [1, 2, 3]);
  assert.equal(demo.echoOptionalVec(null), null);
  assert.equal(demo.optionalVecLength([9, 8]), 2);
  assert.equal(demo.optionalVecLength(null), null);
  assert.deepEqual(demo.findApiResult(0), { tag: "Success" });
  assert.deepEqual(demo.findApiResult(1), { tag: "ErrorCode", value0: -1 });
  assert.equal(demo.findApiResult(99), null);

  // case:options.complex.vec_optional_i32.mixed
  assert.deepEqual(demo.echoVecOptionalI32([1, null, 2, null, 3]), [1, null, 2, null, 3]);
  assert.deepEqual(demo.echoVecOptionalI32([]), []);
}
