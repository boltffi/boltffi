import 'dart:typed_data';

import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  test('vecs', () {
    final ints = echoVecI32(Int32List.fromList([1, 2, 3]));
    expect(
      ints.length,
      3,
      reason: "case:primitives.vecs.i32.should_roundtrip_non_empty",
    );
    expect(ints[0], 1);
    expect(ints[1], 2);
    expect(ints[2], 3);

    final empty = echoVecI32(Int32List.fromList([]));
    expect(
      empty.length,
      0,
      reason: "case:primitives.vecs.i32.should_roundtrip_empty",
    );

    final i8s = echoVecI8(Int8List.fromList([-1, 0, 7]));
    expect(
      i8s.length,
      3,
      reason: "case:primitives.vecs.i8.should_roundtrip_values",
    );
    expect(i8s[0], -1);
    expect(i8s[2], 7);

    final u8s = echoVecU8(Uint8List.fromList([0, 1, 2, 3]));
    expect(
      u8s.length,
      4,
      reason: "case:primitives.vecs.u8.should_roundtrip_values",
    );
    expect(u8s[0], 0);
    expect(u8s[3], 3);

    final i16s = echoVecI16(Int16List.fromList([-3, 0, 9]));
    expect(
      i16s.length,
      3,
      reason: "case:primitives.vecs.i16.should_roundtrip_values",
    );
    expect(i16s[0], -3);
    expect(i16s[2], 9);

    final u16s = echoVecU16(Uint16List.fromList([0, 10, 20]));
    expect(
      u16s.length,
      3,
      reason: "case:primitives.vecs.u16.should_roundtrip_values",
    );
    expect(u16s[0], 0);
    expect(u16s[2], 20);

    final u32s = echoVecU32(Uint32List.fromList([0, 10, 20]));
    expect(
      u32s.length,
      3,
      reason: "case:primitives.vecs.u32.should_roundtrip_values",
    );
    expect(u32s[0], 0);
    expect(u32s[2], 20);

    final i64s = echoVecI64(Int64List.fromList([-5, 0, 8]));
    expect(
      i64s.length,
      3,
      reason: "case:primitives.vecs.i64.should_roundtrip_values",
    );
    expect(i64s[0], -5);
    expect(i64s[2], 8);

    final u64s = echoVecU64(Uint64List.fromList([0, 1, 2]));
    expect(
      u64s.length,
      3,
      reason: "case:primitives.vecs.u64.should_roundtrip_values",
    );
    expect(u64s[0], 0);
    expect(u64s[2], 2);

    final isizes = echoVecIsize(Int64List.fromList([-2, 0, 5]));
    expect(
      isizes.length,
      3,
      reason: "case:primitives.vecs.isize.should_roundtrip_values",
    );
    expect(isizes[0], -2);
    expect(isizes[2], 5);

    final usizes = echoVecUsize(Uint64List.fromList([0, 2, 4]));
    expect(
      usizes.length,
      3,
      reason: "case:primitives.vecs.usize.should_roundtrip_values",
    );
    expect(usizes[0], 0);
    expect(usizes[2], 4);

    expect(
      sumVecI32(Int32List.fromList([10, 20, 30])),
      60,
      reason: "case:primitives.vecs.i32.should_sum_values",
    );
    expect(sumVecI32(Int32List.fromList([])), 0);

    final doubles = echoVecF64(Float64List.fromList([1.5, 2.5]));
    expect(
      doubles.length,
      2,
      reason: "case:primitives.vecs.f64.should_roundtrip_values",
    );
    expect(doubles[0], closeTo(1.5, 0.0001));
    expect(doubles[1], closeTo(2.5, 0.0001));

    final bools = echoVecBool($$BoltBoolList.fromList([true, false, true]));
    expect(
      bools.length,
      3,
      reason: "case:primitives.vecs.bool.should_roundtrip_values",
    );
    expect(bools[0], isTrue);
    expect(bools[1], isFalse);
    expect(bools[2], isTrue);

    final f32s = echoVecF32(Float32List.fromList([1.25, -2.5]));
    expect(
      f32s.length,
      2,
      reason: "case:primitives.vecs.f32.should_roundtrip_values_with_tolerance",
    );
    expect(f32s[0], closeTo(1.25, 0.0001));
    expect(f32s[1], closeTo(-2.5, 0.0001));

    final range = makeRange(0, 5);
    expect(
      range.length,
      5,
      reason: "case:primitives.vecs.i32.should_make_range",
    );
    expect(range[0], 0);
    expect(range[4], 4);

    final reversed = reverseVecI32(Int32List.fromList([1, 2, 3]));
    expect(
      reversed[0],
      3,
      reason: "case:primitives.vecs.i32.should_reverse_values",
    );
    expect(reversed[1], 2);
    expect(reversed[2], 1);

    final genI32 = generateI32Vec(4);
    expect(
      genI32.length,
      4,
      reason: "case:primitives.vecs.i32.should_generate_sequence",
    );
    expect(genI32[0], 0);
    expect(genI32[1], 1);
    expect(genI32[2], 2);
    expect(genI32[3], 3);

    expect(
      sumI32Vec(Int32List.fromList([4, 5, 6])),
      15,
      reason: "case:primitives.vecs.i32.should_sum_benchmark_values",
    );

    final genF64 = generateF64Vec(3);
    expect(
      genF64.length,
      3,
      reason: "case:primitives.vecs.f64.should_generate_sequence",
    );
    expect(genF64[0], closeTo(0.0, 0.0001));
    expect(genF64[1], closeTo(0.1, 0.0001));
    expect(genF64[2], closeTo(0.2, 0.0001));

    expect(
      sumF64Vec(Float64List.fromList([1.5, 2.5, 4.0])),
      closeTo(8.0, 0.0001),
      reason: "case:primitives.vecs.f64.should_sum_values",
    );

    final u64Buf = Uint64List.fromList([10, 20, 30]);
    incU64(u64Buf);
    expect(
      u64Buf,
      [11, 20, 30],
      reason: "case:primitives.vecs.u64.should_increment_first_value_in_place",
    );

    expect(
      incU64Value(41),
      42,
      reason: "case:primitives.vecs.u64.should_increment_value",
    );

    final strings = echoVecString(['hello', 'world']);
    expect(
      strings.length,
      2,
      reason: "case:primitives.vecs.string.should_roundtrip_values",
    );
    expect(strings[0], 'hello');
    expect(strings[1], 'world');

    final emptyStrings = echoVecString([]);
    expect(emptyStrings, isEmpty);

    final lengths = vecStringLengths(['hi', 'café']);
    expect(lengths.length, 2);
    expect(
      lengths,
      [2, 5],
      reason: "case:primitives.vecs.string.should_report_utf8_byte_lengths",
    );

    final vvi = echoVecVecI32([
      Int32List.fromList([1, 2, 3]),
      Int32List.fromList([]),
      Int32List.fromList([4, 5]),
    ]);
    expect(
      vvi.length,
      3,
      reason: "case:primitives.vecs.nested_i32.should_roundtrip_values",
    );
    expect(vvi[0].length, 3);
    expect(vvi[0][0], 1);
    expect(vvi[0][2], 3);
    expect(vvi[1].length, 0);
    expect(vvi[2].length, 2);
    expect(vvi[2][0], 4);
    expect(vvi[2][1], 5);

    final vviEmpty = echoVecVecI32([]);
    expect(
      vviEmpty,
      isEmpty,
      reason: "case:primitives.vecs.nested_i32.should_roundtrip_empty_outer",
    );

    final vvb = echoVecVecBool([
      $$BoltBoolList.fromList([true, false, true]),
      $$BoltBoolList.fromList([]),
      $$BoltBoolList.fromList([false]),
    ]);
    expect(
      vvb.length,
      3,
      reason: "case:primitives.vecs.nested_bool.should_roundtrip_values",
    );
    expect(vvb[0].length, 3);
    expect(vvb[0][0], isTrue);
    expect(vvb[0][1], isFalse);
    expect(vvb[0][2], isTrue);
    expect(vvb[1].length, 0);
    expect(vvb[2].length, 1);
    expect(vvb[2][0], isFalse);

    final vvisize = echoVecVecIsize([
      Int64List.fromList([-2, 0, 5]),
      Int64List.fromList([]),
      Int64List.fromList([9]),
    ]);
    expect(
      vvisize.length,
      3,
      reason: "case:primitives.vecs.nested_isize.should_roundtrip_values",
    );
    expect(vvisize[0].length, 3);
    expect(vvisize[0][0], -2);
    expect(vvisize[0][2], 5);
    expect(vvisize[1].length, 0);
    expect(vvisize[2].length, 1);
    expect(vvisize[2][0], 9);

    final vvusize = echoVecVecUsize([
      Uint64List.fromList([0, 2, 4]),
      Uint64List.fromList([]),
      Uint64List.fromList([8]),
    ]);
    expect(
      vvusize.length,
      3,
      reason: "case:primitives.vecs.nested_usize.should_roundtrip_values",
    );
    expect(vvusize[0].length, 3);
    expect(vvusize[0][0], 0);
    expect(vvusize[0][2], 4);
    expect(vvusize[1].length, 0);
    expect(vvusize[2].length, 1);
    expect(vvusize[2][0], 8);

    final vvs = echoVecVecString([
      ['hello', 'world'],
      [],
      ['café', '🌍'],
    ]);
    expect(
      vvs.length,
      3,
      reason: "case:primitives.vecs.nested_string.should_roundtrip_utf8_values",
    );
    expect(vvs[0], ['hello', 'world']);
    expect(vvs[1], isEmpty);
    expect(vvs[2], ['café', '🌍']);

    final flat = flattenVecVecI32([
      Int32List.fromList([1, 2]),
      Int32List.fromList([3]),
      Int32List.fromList([]),
      Int32List.fromList([4, 5]),
    ]);
    expect(
      flat.length,
      5,
      reason: "case:primitives.vecs.nested_i32.should_flatten_values",
    );
    expect(flat[0], 1);
    expect(flat[1], 2);
    expect(flat[2], 3);
    expect(flat[3], 4);
    expect(flat[4], 5);

    expect(
      flattenVecVecI32([]).length,
      0,
      reason: "case:primitives.vecs.nested_i32.should_flatten_empty",
    );
  });
}
