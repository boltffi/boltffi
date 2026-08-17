import 'dart:typed_data';

import 'package:test/test.dart';
import 'package:demo/demo.dart';

/// `NativeCallable.listener` keeps the isolate alive. Call from each test
/// file's `main` so `dart test` can exit.
void boltffiTestHooks() {
  tearDownAll(shutdownBoltffi);
}

Matcher throwsBoltException(String messageContent) {
  return throwsA(
    isA<$$BoltException>().having(
      (e) => e.message,
      "exception message",
      contains(messageContent),
    ),
  );
}

MixedRecordParameters sampleMixedRecordParameters() {
  return MixedRecordParameters(
    tags: ['alpha', 'beta'],
    checkpoints: [Point(x: 1.0, y: 2.0), Point(x: 3.0, y: 5.0)],
    fallbackAnchor: Point(x: -1.0, y: -2.0),
    maxRetries: 4,
    previewOnly: true,
  );
}

MixedRecord sampleMixedRecord() {
  return MixedRecord(
    name: 'outline',
    anchor: Point(x: 10.0, y: 20.0),
    priority: Priority.critical,
    shape: Shape.rectangle(width: 3.0, height: 4.0),
    parameters: sampleMixedRecordParameters(),
  );
}

String foreignKindName(ForeignKind kind) {
  switch (kind) {
    case ForeignKind.standard:
      return 'standard';
    case ForeignKind.express:
      return 'express';
    case ForeignKind.archive:
      return 'archive';
  }
}

final class ForeignLabelerImpl implements ForeignLabeler {
  @override
  String label(ForeignUser user, ForeignKind kind) {
    return 'label:${user.name}:${foreignKindName(kind)}';
  }
}

final class DoublerValueCallbackImpl implements ValueCallback {
  @override
  int onValue(int value) => value * 2;
}

final class TriplerValueCallbackImpl implements ValueCallback {
  @override
  int onValue(int value) => value * 3;
}

final class PointTransformerImpl implements PointTransformer {
  @override
  Point transform(Point point) => Point(x: point.x + 10.0, y: point.y + 20.0);
}

final class StatusMapperImpl implements StatusMapper {
  @override
  Status mapStatus(Status status) =>
      status == Status.pending ? Status.active : Status.inactive;
}

final class MultiMethodCallbackImpl implements MultiMethodCallback {
  @override
  int methodA(int x) => x + 1;

  @override
  int methodB(int x, int y) => x * y;

  @override
  int methodC() => 5;
}

final class OptionCallbackImpl implements OptionCallback {
  @override
  int? findValue(int key) => key > 0 ? key * 10 : null;
}

final class ResultCallbackImpl implements ResultCallback {
  @override
  int compute(int value) {
    if (value < 0) {
      throw MathError.negativeInput;
    }
    return value * 10;
  }
}

final class FalliblePointTransformerImpl implements FalliblePointTransformer {
  @override
  Point transformPoint(Point point, Status status) {
    if (status == Status.inactive) {
      throw MathError.negativeInput;
    }
    return Point(x: point.x + 100.0, y: point.y + 200.0);
  }
}

final class OffsetCallbackImpl implements OffsetCallback {
  @override
  int offset(int value, int delta) => value + delta;
}

final class VecProcessorImpl implements VecProcessor {
  @override
  Int32List process(Int32List values) =>
      Int32List.fromList(values.map((v) => v * v).toList());
}

final class MessageFormatterImpl implements MessageFormatter {
  @override
  String formatMessage(String scope, String message) =>
      '$scope::${message.toUpperCase()}';
}

final class AsyncOptionalMessageFormatterImpl
    implements AsyncOptionalMessageFetcher {
  @override
  Future<String?> findMessage(int key) {
    return Future.value(key > 0 ? 'async-message:$key' : null);
  }
}

final class OptionalMessageCallbackImpl implements OptionalMessageCallback {
  @override
  String? findMessage(int key) => key > 0 ? 'message:$key' : null;
}

final class ResultMessageCallbackImpl implements ResultMessageCallback {
  @override
  String renderMessage(int key) {
    if (key < 0) {
      throw MathError.negativeInput;
    }
    return 'message:$key';
  }
}

final class StringResultMessageCallbackImpl
    implements StringResultMessageCallback {
  @override
  String renderMessage(int key) {
    if (key < 0) {
      throw $$BoltException('negative key: $key');
    }
    return 'message:$key';
  }
}

final class AsyncFetcherImpl implements AsyncFetcher {
  @override
  Future<int> fetchValue(int key) => Future.value(key * 100);

  @override
  Future<String> fetchString(String input) => Future.value(input.toUpperCase());

  @override
  Future<String> fetchJoinedMessage(String scope, String message) =>
      Future.value('$scope::${message.toUpperCase()}');
}

final class AsyncPointTransformerImpl implements AsyncPointTransformer {
  @override
  Future<Point> transformPoint(Point point) =>
      Future.value(Point(x: point.x + 50.0, y: point.y + 60.0));
}

final class AsyncOptionFetcherImpl implements AsyncOptionFetcher {
  @override
  Future<int?> find(int key) => Future.value(key > 0 ? key * 1000 : null);
}

final class AsyncResultFormatterImpl implements AsyncResultFormatter {
  @override
  Future<String> renderMessage(String scope, String message) {
    if (scope.isEmpty) {
      throw MathError.negativeInput;
    }
    return Future.value('$scope::${message.toUpperCase()}');
  }

  @override
  Future<Point> transformPoint(Point point, Status status) {
    if (status == Status.inactive) {
      throw MathError.negativeInput;
    }
    return Future.value(Point(x: point.x + 500.0, y: point.y + 600.0));
  }
}

final class DataProviderImpl implements DataProvider {
  @override
  int getCount() {
    return 5;
  }

  @override
  DataPoint getItem(int index) {
    return DataPoint(x: 1.0 * index, y: 1.0 * index, timestamp: 0);
  }
}
