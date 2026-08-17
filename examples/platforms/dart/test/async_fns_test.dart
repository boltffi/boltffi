import 'dart:typed_data';

import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('async functions', () async {
    expect(
      await asyncAdd(3, 7),
      10,
      reason: "case:async_fns.basic.add.should_return_sum",
    );
    expect(
      await asyncEcho('hello async'),
      'Echo: hello async',
      reason: "case:async_fns.basic.echo.should_prefix_message",
    );

    final doubled = await asyncDoubleAll(Int32List.fromList([1, 2, 3]));
    expect(
      doubled.length,
      3,
      reason: "case:async_fns.basic.double_all.should_double_i32_vector",
    );
    expect(doubled[0], 2);
    expect(doubled[1], 4);
    expect(doubled[2], 6);

    expect(
      await asyncFindPositive(Int32List.fromList([-1, 0, 5, 3])),
      5,
      reason: "case:async_fns.basic.find_positive.should_return_first_positive",
    );
    expect(
      await asyncFindPositive(Int32List.fromList([-1, -2, -3])),
      isNull,
      reason:
          "case:async_fns.basic.find_positive.should_return_none_for_all_negative",
    );
    expect(
      await asyncConcat(['a', 'b', 'c']),
      'a, b, c',
      reason: "case:async_fns.basic.concat.should_join_string_vector",
    );

    expect(
      await tryComputeAsync(21),
      42,
      reason: "case:async_fns.results.try_compute.should_return_doubled_value",
    );
    expect(
      () => tryComputeAsync(-3),
      throwsA(ComputeError.overflow(value: -3, limit: 0)),
      reason:
          "case:async_fns.results.try_compute.should_return_overflow_for_negative_value",
    );
    expect(
      () => tryComputeAsync(0),
      throwsA(ComputeError.invalidInput(value0: -999)),
      reason:
          "case:async_fns.results.try_compute.should_return_invalid_input_for_zero",
    );

    expect(
      await fetchData(7),
      70,
      reason:
          "case:async_fns.results.fetch_data.should_return_scaled_positive_id",
    );
    expect(
      () => fetchData(0),
      throwsBoltException("invalid id"),
      reason: "case:async_fns.results.fetch_data.should_reject_non_positive_id",
    );

    final counting = await asyncGetNumbers(4);
    expect(
      counting.length,
      4,
      reason:
          "case:async_fns.basic.get_numbers.should_return_counting_sequence",
    );
    expect(counting[0], 0);
    expect(counting[3], 3);

    final record = sampleMixedRecord();
    expect(
      await asyncEchoMixedRecord(record),
      record,
      reason: "case:async_fns.mixed_record.echo.should_roundtrip_record",
    );
    expect(
      await asyncMakeMixedRecord(
        record.name,
        record.anchor,
        record.priority,
        record.shape,
        record.parameters,
      ),
      record,
      reason: "case:async_fns.mixed_record.make.should_construct_record",
    );
  });
}
