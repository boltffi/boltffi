import 'dart:typed_data';

import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('complex options', () {
    expect(
      echoOptionalString('hello'),
      'hello',
      reason: "case:options.complex.string.should_roundtrip_some",
    );
    expect(
      echoOptionalString(null),
      isNull,
      reason: "case:options.complex.string.should_roundtrip_none",
    );
    expect(
      isSomeString('x'),
      isTrue,
      reason: "case:options.complex.string.should_report_some",
    );
    expect(
      isSomeString(null),
      isFalse,
      reason: "case:options.complex.string.should_report_none",
    );

    final optPoint = echoOptionalPoint(Point(x: 1.0, y: 2.0));
    expect(
      optPoint,
      Point(x: 1.0, y: 2.0),
      reason: "case:options.complex.point.should_roundtrip_some",
    );
    expect(
      echoOptionalPoint(null),
      isNull,
      reason: "case:options.complex.point.should_roundtrip_none",
    );
    expect(
      makeSomePoint(3.0, 4.0),
      Point(x: 3.0, y: 4.0),
      reason: "case:options.complex.point.should_make_some",
    );
    expect(
      makeNonePoint(),
      isNull,
      reason: "case:options.complex.point.should_make_none",
    );

    expect(
      echoOptionalStatus(Status.active),
      Status.active,
      reason: "case:options.complex.status.should_roundtrip_some",
    );
    expect(
      echoOptionalStatus(null),
      isNull,
      reason: "case:options.complex.status.should_roundtrip_none",
    );

    expect(
      echoOptionalVec(Int32List.fromList([1, 2, 3])),
      [1, 2, 3],
      reason: "case:options.complex.vec.should_roundtrip_some",
    );
    expect(
      echoOptionalVec(null),
      isNull,
      reason: "case:options.complex.vec.should_roundtrip_none",
    );
    expect(
      echoOptionalVec(Int32List.fromList([])),
      [],
      reason: "case:options.complex.vec.should_roundtrip_empty_some",
    );

    expect(
      optionalVecLength(Int32List.fromList([9, 8])),
      2,
      reason: "case:options.complex.vec.should_report_length_for_some",
    );
    expect(
      optionalVecLength(null),
      isNull,
      reason: "case:options.complex.vec.should_return_none_for_absent_length",
    );

    expect(
      radiusIfCircle(Shape.circle(radius: 5.0)),
      5.0,
      reason: "case:options.complex.shape.should_return_radius_for_circle",
    );
    expect(
      radiusIfCircle(Shape.rectangle(width: 2.0, height: 3.0)),
      isNull,
      reason: "case:options.complex.shape.should_return_none_for_non_circle",
    );

    expect(
      findName(3),
      'Name_3',
      reason: "case:options.complex.string.should_find_name_for_positive_id",
    );
    expect(
      findName(0),
      isNull,
      reason:
          "case:options.complex.string.should_return_none_for_non_positive_id",
    );

    final foundNumbers = findNumbers(4);
    expect(
      foundNumbers,
      [0, 1, 2, 3],
      reason: "case:options.complex.vec.should_find_numbers_for_positive_count",
    );
    expect(
      findNumbers(0),
      isNull,
      reason:
          "case:options.complex.vec.should_return_none_for_non_positive_number_count",
    );

    expect(
      findNames(2),
      ['Name_0', 'Name_1'],
      reason:
          "case:options.complex.vec_string.should_find_names_for_positive_count",
    );
    expect(
      findNames(-1),
      isNull,
      reason:
          "case:options.complex.vec_string.should_return_none_for_non_positive_name_count",
    );

    expect(
      findApiResult(0),
      isA<ApiResult$Success>(),
      reason: "case:options.complex.api_result.should_find_success_variant",
    );
    final apiErrorCode = findApiResult(1);
    expect(
      apiErrorCode,
      isA<ApiResult$ErrorCode>(),
      reason: "case:options.complex.api_result.should_find_error_code_variant",
    );
    final apiErrorData = findApiResult(2);
    expect(
      apiErrorData,
      isA<ApiResult$ErrorWithData>(),
      reason:
          "case:options.complex.api_result.should_find_error_with_data_variant",
    );
    expect(
      findApiResult(99),
      isNull,
      reason:
          "case:options.complex.api_result.should_return_none_for_unknown_code",
    );

    final mixed = [1, null, 3, null, 5];
    final echoedMixed = echoVecOptionalI32(mixed);
    expect(
      echoedMixed,
      mixed,
      reason:
          "case:options.complex.vec_optional_i32.should_roundtrip_mixed_presence",
    );
    expect(
      echoVecOptionalI32([]),
      isEmpty,
      reason: "case:options.complex.vec_optional_i32.should_roundtrip_empty",
    );
    final allNull = [null, null, null];
    expect(
      echoVecOptionalI32(allNull),
      allNull,
      reason: "case:options.complex.vec_optional_i32.should_roundtrip_all_none",
    );

    final emptySome = echoOptionalVec(Int32List.fromList([]));
    expect(emptySome, isNotNull);
    expect(emptySome!.length, 0);

    final allNone = [null, null, null];
    final echoedAllNone = echoVecOptionalI32(allNone);
    expect(echoedAllNone.length, 3);
    for (final v in echoedAllNone) {
      expect(v, isNull);
    }
  });
}
