import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('multi-crate exports', () {
    final user = ForeignUser(name: 'Ada', age: 37);
    final session = ForeignSession(
      id: 7,
      user: user,
      kind: ForeignKind.express,
    );

    final labeler = ForeignLabelerImpl();

    expect(multiEchoKind(ForeignKind.archive), ForeignKind.archive);
    expect(multiKindLabel(ForeignKind.express), 'express');
    expect(
      multiShiftPoint(ForeignPoint(x: 1.0, y: 2.0), 3.0, 4.0),
      ForeignPoint(x: 4.0, y: 6.0),
    );
    expect(
      multiPointSum(ForeignPoint(x: 2.0, y: 5.0)),
      closeTo(7.0, 0.0000001),
    );
    expect(multiUserSummary(user), 'Ada#37');
    expect(multiEchoCode('mc-7'), 'mc-7');
    expect(multiCodeValue('mc-7'), 'mc-7');
    expect(multiStateSummary(ForeignState.ready()), 'ready');
    expect(multiStateSummary(ForeignState.busy(reason: 'sync')), 'busy:sync');
    expect(multiMakeSession(7, user, ForeignKind.express), session);
    expect(multiSessionSummary(session), '7#Ada#37#express');
    expect(
      multiTotalAge([
        session,
        ForeignSession(
          id: 8,
          user: ForeignUser(name: 'Grace', age: 41),
          kind: ForeignKind.archive,
        ),
      ]),
      78,
    );
    expect(multiOptionalUserName(session), 'Ada');
    expect(multiOptionalUserName(null), isNull);
    expect(
      multiEventSummary(SessionEvent.started(session: session)),
      'started:7#Ada#37#express',
    );
    expect(multiEventSummary(SessionEvent.stopped()), 'stopped');
    expect(
      multiTrySession(9, user, ForeignKind.archive),
      ForeignSession(id: 9, user: user, kind: ForeignKind.archive),
    );
    expect(
      () => multiTrySession(0, user, ForeignKind.standard),
      throwsBoltException("session id must be positive"),
    );

    expect(
      multiBorrowedSummary(user, session, ForeignKind.archive),
      'Ada#37#7#express#archive',
    );
    expect(
      multiFormatWithLabeler(labeler, user, ForeignKind.express),
      'label:Ada:express',
    );

    expect(modelEchoKind(ForeignKind.standard), ForeignKind.standard);
    expect(modelKindLabel(ForeignKind.archive), 'archive');
    expect(
      modelShiftPoint(ForeignPoint(x: 2.0, y: 3.0), 5.0, 7.0),
      ForeignPoint(x: 7.0, y: 10.0),
    );
    expect(
      modelPointSum(ForeignPoint(x: 4.0, y: 6.0)),
      closeTo(10.0, 0.0000001),
    );
    expect(modelUserSummary(user), 'Ada#37');
    expect(modelEchoCode('dep'), 'dep');
    expect(modelCodeValue('dep'), 'dep');
    expect(modelStateSummary(ForeignState.busy(reason: 'dep')), 'busy:dep');
    expect(
      modelFormatWithLabeler(labeler, user, ForeignKind.archive),
      'label:Ada:archive',
    );

    expect(sessionMake(7, user, ForeignKind.express), session);
    expect(sessionSummary(session), '7#Ada#37#express');
    expect(sessionTotalAge([session]), 37);
    expect(sessionOptionalUserName(session), 'Ada');
    expect(sessionOptionalUserName(null), isNull);
    expect(
      sessionEventSummary(SessionEvent.started(session: session)),
      'started:7#Ada#37#express',
    );
    expect(
      sessionTryMake(7, user, ForeignKind.standard),
      ForeignSession(id: 7, user: user, kind: ForeignKind.standard),
    );
    expect(
      sessionApplyLabeler(labeler, user, ForeignKind.standard),
      'label:Ada:standard',
    );

    final counter = ForeignCounter(10);
    expect(counter.add(5), 15);
    expect(counter.$get(), 15);
    expect(
      counter.summarizeUser(user, ForeignKind.standard),
      'Ada#37#standard#15',
    );

    final emptyBook = SessionBook();
    expect(emptyBook.count(), 0);
    expect(emptyBook.summarizeFirst(ForeignKind.archive), 'empty#archive');

    final book = SessionBook.withSession(session);
    expect(book.count(), 1);
    expect(
      book.addSession(
        ForeignSession(
          id: 8,
          user: ForeignUser(name: 'Grace', age: 41),
          kind: ForeignKind.archive,
        ),
      ),
      2,
    );
    expect(book.summarizeFirst(ForeignKind.standard), '7#Ada#37#express');
    expect(
      book.summarizeBorrowed(user, session, ForeignKind.archive),
      'Ada#37#7#express#archive',
    );
    final metrics = book.metricsForPoints([
      ForeignPoint(x: 1.0, y: 2.0),
      ForeignPoint(x: 3.0, y: 4.0),
    ]);
    expect(metrics.score, closeTo(10.0, 0.0001));
    expect(metrics.count, 2);
  });
}
