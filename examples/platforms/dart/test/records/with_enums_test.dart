import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('records with enums', () {
    final logEntry = makeErrorLogEntry(1234567890, 42);
    expect(
      logEntry.level,
      LogLevel.error,
      reason: "case:records.with_enums.log_entry.should_make_error_entry",
    );
    expect(logEntry.timestamp, 1234567890);
    expect(logEntry.code, 42);

    expect(
      echoLogEntry(logEntry),
      logEntry,
      reason:
          "case:records.with_enums.log_entry.should_roundtrip_u8_enum_field",
    );

    final withEmailAndScore = makeUserProfile(
      'Alice',
      30,
      'alice@example.com',
      98.5,
    );
    expect(
      withEmailAndScore.email,
      'alice@example.com',
      reason:
          "case:records.with_options.user_profile.should_make_with_present_options",
    );
    expect(
      withEmailAndScore,
      isA<UserProfile>()
          .having(
            (p) => p.email,
            "`UserProfile` email property",
            equals('alice@example.com'),
          )
          .having((p) => p.score, "`UserProfile` score property", equals(98.5)),
      reason:
          "case:records.with_options.user_profile.should_roundtrip_present_options",
    );

    expect(
      userDisplayName(withEmailAndScore),
      'Alice <alice@example.com>',
      reason:
          "case:records.with_options.user_profile.should_display_email_when_present",
    );

    final noEmailAndScore = makeUserProfile('Bob', 22, null, null);
    expect(
      noEmailAndScore.email,
      isNull,
      reason:
          "case:records.with_options.user_profile.should_make_with_absent_options",
    );
    expect(
      noEmailAndScore,
      isA<UserProfile>()
          .having((p) => p.email, "`UserProfile` email property", isNull)
          .having((p) => p.score, "`UserProfile` score property", isNull),
      reason:
          "case:records.with_options.user_profile.should_roundtrip_absent_options",
    );
    expect(
      userDisplayName(noEmailAndScore),
      'Bob',
      reason:
          'case:records.with_options.user_profile.should_display_name_when_email_absent',
    );

    final noEmail = makeUserProfile('Bob', 22, null, 70.5);
    expect(
      noEmail,
      isA<UserProfile>()
          .having((p) => p.email, "`UserProfile` email property", isNull)
          .having((p) => p.score, "`UserProfile` score property", equals(70.5)),
      reason:
          "case:records.with_options.user_profile.should_roundtrip_mixed_options",
    );

    final utf8Email = makeUserProfile('Kitty', 22, 'hello🐱@email.com', 70.5);
    expect(
      utf8Email,
      isA<UserProfile>().having(
        (p) => p.email,
        "`UserProfile` email property",
        equals('hello🐱@email.com'),
      ),
      reason:
          "case:records.with_options.user_profile.should_roundtrip_utf8_optional_string",
    );

    expect(echoUserProfile(withEmailAndScore).email, 'alice@example.com');

    final echoedAbsent = echoUserProfile(noEmailAndScore);
    expect(echoedAbsent.email, isNull);
    expect(echoedAbsent.score, isNull);
    expect(echoedAbsent.name, 'Bob');
    expect(echoedAbsent.age, 22);

    final mixedProfile = UserProfile(
      name: 'Cara',
      age: 27,
      email: 'cara@example.com',
      score: null,
    );
    final echoedMixedProfile = echoUserProfile(mixedProfile);
    expect(echoedMixedProfile.email, 'cara@example.com');
    expect(echoedMixedProfile.score, isNull);

    final utf8 = UserProfile(
      name: 'Élodie',
      age: 34,
      email: 'élodie@café.example',
      score: 7.5,
    );
    final echoedUtf8 = echoUserProfile(utf8);
    expect(echoedUtf8.email, 'élodie@café.example');
    expect(echoedUtf8.name, 'Élodie');

    final withCursor = echoSearchResult(
      SearchResult(
        query: 'rust ffi',
        total: 12,
        nextCursor: 'cursor-1',
        maxScore: 0.99,
      ),
    );
    expect(
      withCursor,
      isA<SearchResult>()
          .having(
            (r) => r.nextCursor,
            "`SearchResult` nextCursor property",
            equals('cursor-1'),
          )
          .having(
            (r) => r.maxScore,
            "`SearchResult` maxScore property",
            equals(0.99),
          ),
      reason:
          'case:records.with_options.search_result.should_roundtrip_present_options',
    );

    expect(
      hasMoreResults(withCursor),
      isTrue,
      reason:
          'case:records.with_options.search_result.should_report_more_results_when_cursor_present',
    );

    final withoutCursor = echoSearchResult(
      SearchResult(
        query: 'rust ffi',
        total: 12,
        nextCursor: null,
        maxScore: null,
      ),
    );
    expect(
      withoutCursor,
      isA<SearchResult>()
          .having(
            (r) => r.nextCursor,
            "`SearchResult` nextCursor property",
            isNull,
          )
          .having(
            (r) => r.maxScore,
            "`SearchResult` maxScore property",
            isNull,
          ),
      reason:
          'case:records.with_options.search_result.should_roundtrip_absent_options',
    );

    expect(
      hasMoreResults(withoutCursor),
      isFalse,
      reason:
          'case:records.with_options.search_result.should_report_no_more_results_without_cursor',
    );

    final task = Task(title: 'ship it', priority: Priority.high, completed: false);
    expect(
      echoTask(task),
      task,
      reason: "case:records.with_enums.task.should_roundtrip_priority_field",
    );

    final madeTask = makeTask('deploy', Priority.medium);
    expect(madeTask.title, 'deploy');
    expect(madeTask.priority, Priority.medium);
    expect(
      madeTask.completed,
      isFalse,
      reason: "case:records.with_enums.task.should_make_incomplete_task",
    );

    expect(
      isUrgent(Task(title: 'fire', priority: Priority.critical, completed: false)),
      isTrue,
      reason: "case:records.with_enums.task.should_detect_urgent_priority",
    );
    expect(
      isUrgent(Task(title: 'later', priority: Priority.low, completed: false)),
      isFalse,
    );

    final notification = Notification(
      message: 'build finished',
      priority: Priority.medium,
      read: false,
    );
    expect(
      echoNotification(notification),
      notification,
      reason:
          "case:records.with_enums.notification.should_roundtrip_priority_field",
    );

    final holder = Holder(
      shape: Shape.circle(radius: 3.0),
    );
    expect(
      echoHolder(holder),
      holder,
      reason:
          "case:records.with_enums.holder.should_roundtrip_data_enum_field",
    );

    final triangleHolder = makeTriangleHolder();
    expect(
      triangleHolder.shape,
      isA<Shape$Triangle>(),
      reason: "case:records.with_enums.holder.should_make_triangle_variant",
    );
    final triangle = triangleHolder.shape as Shape$Triangle;
    expect(triangle.a, Point(x: 0.0, y: 0.0));
    expect(triangle.b, Point(x: 4.0, y: 0.0));
    expect(triangle.c, Point(x: 0.0, y: 3.0));

    final taskHeader = TaskHeader(id: 7, priority: Priority.high, completed: true);
    expect(
      echoTaskHeader(taskHeader),
      taskHeader,
      reason:
          "case:records.with_enums.task_header.should_roundtrip_repr_enum_field",
    );

    final criticalHeader = makeCriticalTaskHeader(9);
    expect(criticalHeader.id, 9);
    expect(
      criticalHeader.priority,
      Priority.critical,
      reason: "case:records.with_enums.task_header.should_make_critical_header",
    );
    expect(criticalHeader.completed, isFalse);
  });
}
