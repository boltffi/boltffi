import { assert, assertArrayEqual, demo } from "../support/index.mjs";

export async function run() {
  assertArrayEqual(demo.echoVecI32([1, 2, 3]), [1, 2, 3]);
  assertArrayEqual(demo.echoVecI8([-1, 0, 7]), [-1, 0, 7]);
  assertArrayEqual(demo.echoVecU8(Uint8Array.from([0, 1, 2, 3])), [0, 1, 2, 3]);
  assertArrayEqual(demo.echoVecI16([-3, 0, 9]), [-3, 0, 9]);
  assertArrayEqual(demo.echoVecU16([0, 10, 20]), [0, 10, 20]);
  assertArrayEqual(demo.echoVecU32([0, 10, 20]), [0, 10, 20]);
  assertArrayEqual(demo.echoVecI64([-5n, 0n, 8n]), [-5n, 0n, 8n]);
  assertArrayEqual(demo.echoVecU64([0n, 1n, 2n]), [0n, 1n, 2n]);
  assertArrayEqual(demo.echoVecIsize([-2, 0, 5]), [-2, 0, 5]);
  assertArrayEqual(demo.echoVecUsize([0, 2, 4]), [0, 2, 4]);
  assertArrayEqual(demo.echoVecF32([1.25, -2.5]), [1.25, -2.5]);
  assert.equal(demo.sumVecI32([10, 20, 30]), 60n);
  assertArrayEqual(demo.echoVecF64([1.5, 2.5]), [1.5, 2.5]);
  assertArrayEqual(demo.echoVecBool([true, false, true]), [true, false, true]);
  assertArrayEqual(demo.echoVecString(["hello", "world"]), ["hello", "world"]);
  assertArrayEqual(demo.vecStringLengths(["hi", "café"]), [2, 5]);
  assertArrayEqual(demo.makeRange(0, 5), [0, 1, 2, 3, 4]);
  assertArrayEqual(demo.reverseVecI32([1, 2, 3]), [3, 2, 1]);
  assert.equal(demo.incU64(BigUint64Array.from([1n, 2n])), undefined);

  const vvi = demo.echoVecVecI32([[1, 2, 3], [], [4, 5]]);
  assert.equal(vvi.length, 3);
  assertArrayEqual(vvi[0], [1, 2, 3]);
  assertArrayEqual(vvi[1], []);
  assertArrayEqual(vvi[2], [4, 5]);
  assert.equal(demo.echoVecVecI32([]).length, 0);

  const vvb = demo.echoVecVecBool([[true, false, true], [], [false]]);
  assert.equal(vvb.length, 3);
  assertArrayEqual(vvb[0], [true, false, true]);
  assertArrayEqual(vvb[1], []);
  assertArrayEqual(vvb[2], [false]);

  assert.deepEqual(
    demo.echoVecVecString([["hello", "world"], [], ["café", "🌍"]]),
    [["hello", "world"], [], ["café", "🌍"]],
  );

  assertArrayEqual(demo.flattenVecVecI32([[1, 2], [3], [], [4, 5]]), [1, 2, 3, 4, 5]);
  assertArrayEqual(demo.flattenVecVecI32([]), []);
}
