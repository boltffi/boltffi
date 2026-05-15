import { assert, assertArrayEqual, demo } from "../support/index.mjs";

export async function run() {
  globalThis.demoCase("case:primitives.vecs.i32.should_roundtrip_non_empty");
  assertArrayEqual(demo.echoVecI32([1, 2, 3]), [1, 2, 3]);
  globalThis.demoCase("case:primitives.vecs.i32.should_roundtrip_empty");
  assertArrayEqual(demo.echoVecI32([]), []);
  globalThis.demoCase("case:primitives.vecs.i8.should_roundtrip_values");
  assertArrayEqual(demo.echoVecI8([-1, 0, 7]), [-1, 0, 7]);
  globalThis.demoCase("case:primitives.vecs.u8.should_roundtrip_values");
  assertArrayEqual(demo.echoVecU8(Uint8Array.from([0, 1, 2, 3])), [0, 1, 2, 3]);
  globalThis.demoCase("case:primitives.vecs.i16.should_roundtrip_values");
  assertArrayEqual(demo.echoVecI16([-3, 0, 9]), [-3, 0, 9]);
  globalThis.demoCase("case:primitives.vecs.u16.should_roundtrip_values");
  assertArrayEqual(demo.echoVecU16([0, 10, 20]), [0, 10, 20]);
  globalThis.demoCase("case:primitives.vecs.u32.should_roundtrip_values");
  assertArrayEqual(demo.echoVecU32([0, 10, 20]), [0, 10, 20]);
  globalThis.demoCase("case:primitives.vecs.i64.should_roundtrip_values");
  assertArrayEqual(demo.echoVecI64([-5n, 0n, 8n]), [-5n, 0n, 8n]);
  globalThis.demoCase("case:primitives.vecs.u64.should_roundtrip_values");
  assertArrayEqual(demo.echoVecU64([0n, 1n, 2n]), [0n, 1n, 2n]);
  globalThis.demoCase("case:primitives.vecs.isize.should_roundtrip_values");
  assertArrayEqual(demo.echoVecIsize([-2, 0, 5]), [-2, 0, 5]);
  globalThis.demoCase("case:primitives.vecs.usize.should_roundtrip_values");
  assertArrayEqual(demo.echoVecUsize([0, 2, 4]), [0, 2, 4]);
  globalThis.demoCase("case:primitives.vecs.f32.should_roundtrip_values_with_tolerance");
  assertArrayEqual(demo.echoVecF32([1.25, -2.5]), [1.25, -2.5]);
  assert.equal(demo.sumVecI32([10, 20, 30]), 60n, "case:primitives.vecs.i32.should_sum_values");
  globalThis.demoCase("case:primitives.vecs.f64.should_roundtrip_values");
  assertArrayEqual(demo.echoVecF64([1.5, 2.5]), [1.5, 2.5]);
  globalThis.demoCase("case:primitives.vecs.bool.should_roundtrip_values");
  assertArrayEqual(demo.echoVecBool([true, false, true]), [true, false, true]);
  globalThis.demoCase("case:primitives.vecs.echo_string.basic");
  assertArrayEqual(demo.echoVecString(["hello", "world"]), ["hello", "world"]);
  globalThis.demoCase("case:primitives.vecs.string_lengths.utf8");
  assertArrayEqual(demo.vecStringLengths(["hi", "café"]), [2, 5]);
  globalThis.demoCase("case:primitives.vecs.i32.should_make_range");
  assertArrayEqual(demo.makeRange(0, 5), [0, 1, 2, 3, 4]);
  globalThis.demoCase("case:primitives.vecs.i32.should_reverse_values");
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
