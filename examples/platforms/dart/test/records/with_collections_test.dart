import 'dart:typed_data';

import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('records with collections', () {
    final polygonPoints = [
      Point(x: 0.0, y: 0.0),
      Point(x: 1.0, y: 0.0),
      Point(x: 0.0, y: 1.0),
    ];
    final polygon = makePolygon(polygonPoints);

    expect(
      polygon.points,
      polygonPoints,
      reason: "case:records.with_collections.polygon.should_make_from_points",
    );

    expect(
      echoPolygon(polygon),
      polygon,
      reason:
          "case:records.with_collections.polygon.should_roundtrip_point_vector",
    );

    expect(
      polygonVertexCount(polygon),
      3,
      reason:
          "case:records.with_collections.polygon.should_report_vertex_count",
    );

    final centroid = polygonCentroid(polygon);
    expect(
      centroid,
      isA<Point>()
          .having(
            (p) => p.x,
            "centroid x coordinate",
            closeTo(1.0 / 3.0, 0.0001),
          )
          .having(
            (p) => p.y,
            "centroid y coordinate",
            closeTo(1.0 / 3.0, 0.0001),
          ),
      reason: "case:records.with_collections.polygon.should_compute_centroid",
    );

    final team = makeTeam('devs', ['Alice', 'Bob']);
    expect(
      team,
      isA<Team>()
          .having((t) => t.name, "team name property", equals('devs'))
          .having(
            (t) => t.members,
            "team members property",
            equals(['Alice', 'Bob']),
          ),
      reason: "case:records.with_collections.team.should_make_from_members",
    );

    expect(
      echoTeam(team),
      team,
      reason:
          "case:records.with_collections.team.should_roundtrip_member_vector",
    );
    expect(
      teamSize(team),
      2,
      reason: "case:records.with_collections.team.should_report_member_count",
    );

    final classroomStudents = [
      Person(name: 'Mia', age: 10),
      Person(name: 'Leo', age: 11),
    ];

    final classroom = Classroom(students: classroomStudents);
    expect(classroom.students.length, 2);
    expect(classroom.students[0].name, 'Mia');

    expect(
      echoClassroom(classroom),
      classroom,
      reason:
          "case:records.with_collections.classroom.should_roundtrip_student_vector",
    );

    expect(
      makeClassroom(classroomStudents),
      isA<Classroom>().having(
        (c) => c.students,
        "classroom students property",
        equals(classroomStudents),
      ),
      reason:
          "case:records.with_collections.classroom.should_make_from_students",
    );

    final taggedScoresLabel = 'math';
    final taggedScoresScores = Float64List.fromList([90.0, 85.5]);
    expect(
      echoTaggedScores(
        TaggedScores(label: taggedScoresLabel, scores: taggedScoresScores),
      ),
      isA<TaggedScores>()
          .having(
            (t) => t.label,
            "`TaggedScores` label property",
            equals(taggedScoresLabel),
          )
          .having(
            (t) => t.scores,
            "`TaggedScores` scores property",
            equals(taggedScoresScores),
          ),
      reason:
          "case:records.with_collections.tagged_scores.should_roundtrip_score_vector",
    );

    expect(
      averageScore(
        TaggedScores(label: 'x', scores: Float64List.fromList([80.0, 100.0])),
      ),
      closeTo(90.0, 0.0001),
      reason:
          "case:records.with_collections.tagged_scores.should_average_scores",
    );

    final profiles = generateUserProfiles(4);
    expect(
      profiles.length,
      4,
      reason:
          "case:records.with_collections.user_profiles.should_generate_profiles",
    );
    expect(profiles[0].id, 0);
    expect(profiles[3].id, 3);

    final scoreSum = sumUserScores(profiles);
    var expectedScoreSum = 0.0;
    for (final p in profiles) {
      expectedScoreSum += p.score;
    }
    expect(
      scoreSum,
      closeTo(expectedScoreSum, 0.0001),
      reason: "case:records.with_collections.user_profiles.should_sum_scores",
    );

    final active = countActiveUsers(profiles);
    var expectedActive = 0;
    for (final p in profiles) {
      if (p.isActive) {
        expectedActive++;
      }
    }
    expect(
      active,
      expectedActive,
      reason:
          "case:records.with_collections.user_profiles.should_count_active_users",
    );
  });
}
