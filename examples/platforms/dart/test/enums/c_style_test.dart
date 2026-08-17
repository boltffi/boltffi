import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('C-style enums', () {
    expect(echoStatus(Status.active), Status.active);
    expect(echoStatus(Status.inactive), Status.inactive);
    expect(
      echoStatus(Status.pending),
      Status.pending,
      reason: "case:enums.c_style.status.should_roundtrip_values",
    );

    expect(statusToString(Status.active), 'active');
    expect(
      statusToString(Status.inactive),
      'inactive',
      reason: "case:enums.c_style.status.should_render_labels",
    );

    expect(isActive(Status.active), isTrue);
    expect(
      isActive(Status.pending),
      isFalse,
      reason: "case:enums.c_style.status.should_identify_active_values",
    );

    final statuses = echoVecStatus([
      Status.active,
      Status.pending,
      Status.inactive,
    ]);
    expect(statuses.length, 3);
    expect(statuses, [
      Status.active,
      Status.pending,
      Status.inactive,
    ], reason: "case:enums.c_style.status.should_roundtrip_vectors");

    final levels = echoVecLogLevel([
      LogLevel.trace,
      LogLevel.info,
      LogLevel.error,
    ]);
    expect(
      levels.length,
      3,
      reason: "case:enums.repr_int.log_level.should_roundtrip_vectors",
    );
    expect(levels[0], LogLevel.trace);
    expect(levels[1], LogLevel.info);
    expect(levels[2], LogLevel.error);

    final directions = [
      Direction.$new(0),
      Direction.$new(1),
      Direction.$new(2),
      Direction.$new(3),
      Direction.$new(42),
    ];
    expect(
      directions,
      [
        Direction.north,
        Direction.south,
        Direction.east,
        Direction.west,
        Direction.north,
      ],
      reason: "case:enums.c_style.direction.should_construct_from_raw_value",
    );
    expect(
      echoDirection(Direction.north),
      Direction.north,
      reason: "case:enums.c_style.direction.should_roundtrip_value",
    );
    expect(oppositeDirection(Direction.north), Direction.south);
    expect(
      oppositeDirection(Direction.east),
      Direction.west,
      reason:
          "case:enums.c_style.direction.should_return_opposite_from_free_function",
    );

    expect(
      Direction.cardinal(),
      Direction.north,
      reason: "case:enums.c_style.direction.should_return_cardinal_value",
    );
    expect(Direction.fromDegrees(90.0), Direction.east);
    expect(
      Direction.fromDegrees(225.0),
      Direction.west,
      reason: "case:enums.c_style.direction.should_construct_from_degrees",
    );

    expect(
      Direction.north.opposite(),
      Direction.south,
      reason: "case:enums.c_style.direction.should_return_opposite_from_method",
    );
    expect(Direction.north.horizontalOr(Direction.west), Direction.west);
    expect(
      Direction.east.horizontalOr(Direction.west),
      Direction.east,
      reason:
          "case:enums.c_style.direction.should_return_method_parameter_value",
    );

    expect(Direction.west.isHorizontal(), isTrue);
    expect(
      Direction.north.isHorizontal(),
      isFalse,
      reason: "case:enums.c_style.direction.should_identify_horizontal_values",
    );

    expect(
      Direction.south.label(),
      'S',
      reason: "case:enums.c_style.direction.should_render_compass_label",
    );
    expect(
      Direction.count(),
      4,
      reason: "case:enums.c_style.direction.should_report_variant_count",
    );

    expect(0, Direction.north.value);
    expect(1, Direction.south.value);
    expect(2, Direction.east.value);
    expect(3, Direction.west.value);

    expect(directionToDegrees(Direction.north), 0);
    expect(directionToDegrees(Direction.east), 90);
    expect(directionToDegrees(Direction.south), 180);
    expect(
      directionToDegrees(Direction.west),
      270,
      reason: "case:enums.c_style.direction.should_return_degrees",
    );

    final generated = generateDirections(6);
    expect(
      generated.length,
      6,
      reason: "case:enums.c_style.direction.should_generate_sequence",
    );
    expect(generated[0], Direction.north);
    expect(generated[4], Direction.north);

    expect(
      countNorth(generated),
      2,
      reason: "case:enums.c_style.direction.should_count_north_values",
    );
    expect(countNorth([Direction.east, Direction.south]), 0);

    expect(
      findDirection(2),
      Direction.south,
      reason:
          "case:enums.c_style.direction.find_direction.should_return_some_for_known_id",
    );
    expect(
      findDirection(99),
      isNull,
      reason:
          "case:enums.c_style.direction.find_direction.should_return_none_for_unknown_id",
    );

    final foundSeq = findDirections(3);
    expect(
      foundSeq,
      isNotNull,
      reason:
          "case:enums.c_style.direction.find_directions.should_return_sequence_for_positive_count",
    );
    expect(foundSeq!.length, 3);
    expect(foundSeq[0], Direction.north);
    expect(
      findDirections(0),
      isNull,
      reason:
          "case:enums.c_style.direction.find_directions.should_return_none_for_non_positive_count",
    );

    expect(
      echoPriority(Priority.high),
      Priority.high,
      reason: "case:enums.repr_int.priority.should_roundtrip_value",
    );
    expect(
      priorityLabel(Priority.low),
      'low',
      reason: "case:enums.repr_int.priority.should_render_label",
    );
    expect(isHighPriority(Priority.critical), isTrue);
    expect(
      isHighPriority(Priority.low),
      isFalse,
      reason: "case:enums.repr_int.priority.should_identify_high_priority",
    );

    expect(
      echoLogLevel(LogLevel.info),
      LogLevel.info,
      reason: "case:enums.repr_int.log_level.should_roundtrip_value",
    );
    expect(
      shouldLog(LogLevel.error, LogLevel.warn),
      isTrue,
      reason: "case:enums.repr_int.log_level.should_compare_against_minimum",
    );
    expect(shouldLog(LogLevel.debug, LogLevel.info), isFalse);

    expect(HttpCode.ok.value, 200);
    expect(HttpCode.notFound.value, 404);
    expect(
      HttpCode.serverError.value,
      500,
      reason: "case:enums.repr_int.http_code.should_expose_discriminant_values",
    );

    expect(
      httpCodeNotFound(),
      HttpCode.notFound,
      reason: "case:enums.repr_int.http_code.should_return_not_found",
    );
    expect(echoHttpCode(HttpCode.ok), HttpCode.ok);
    expect(
      echoHttpCode(HttpCode.serverError),
      HttpCode.serverError,
      reason: "case:enums.repr_int.http_code.should_roundtrip_values",
    );

    expect(Sign.negative.value, -1);
    expect(Sign.zero.value, 0);
    expect(
      Sign.positive.value,
      1,
      reason:
          "case:enums.repr_int.sign.should_expose_signed_discriminant_values",
    );

    expect(
      signNegative(),
      Sign.negative,
      reason: "case:enums.repr_int.sign.should_return_negative",
    );
    expect(echoSign(Sign.negative), Sign.negative);
    expect(
      echoSign(Sign.positive),
      Sign.positive,
      reason: "case:enums.repr_int.sign.should_roundtrip_signed_values",
    );
  });
}
