import 'dart:async';
import 'dart:js_interop';
import 'dart:typed_data';
import 'probe.dart';

final class DartAdder implements Adder {
  @override
  int add(int left, int right) => left + right;
}

@JS('probeDone')
external void done();
@JS('largeInteger')
external JSBigInt largeInteger();

void check(bool condition, String message) {
  if (!condition) throw StateError(message);
}

Future<void> main() async {
  check(boltffiInt64FromJS(boltffiInt64ToJS(42)) == 42, 'bigint round trip');
  try { boltffiInt64FromJS(largeInteger()); throw StateError('lost precision'); }
  on RangeError { }
  final values = Int32List.fromList([1, -2, 3]);
  check(echoValues(values)[1] == -2, 'direct numeric vector');
  final scores = echoScores(Scores(values: values));
  check(scores.values[1] == -2, 'nested numeric vector');
  check(scores == Scores(values: values), 'record value equality');
  check(scores.hashCode == Scores(values: values).hashCode, 'record value hash');
  check(echoBools($$BoltBoolList.fromList([true, false]))[0], 'bool vector');
  check(echoLongs([42, -3])[1] == -3, 'bigint vector');
  final point = Point(x: 1, y: 2);
  mutatePoint(point);
  check(point.x == 42, 'mutable record');
  try { mutateFallible(point); throw StateError('expected typed error'); }
  on Failure catch (error) { check(error.message == 'failure', 'error payload'); }
  check(point.x == 43, 'writeback on error');
  try { fallible(); throw StateError('expected typed error'); }
  on Failure { }
  try { failString(); throw StateError('expected string error'); }
  on $$BoltException catch (error) { check(error.message == 'failure', 'string error'); }
  check(greeting() == 'world' && greeting(name: 'Dart') == 'Dart', 'defaults');
  check(Options().name == 'hello' && Options().count == 3, 'record defaults');
  check(interleaved(5, middle: 9) == 17, 'required parameter after default');
  check(defaultUrl() == Uri.parse('https://example.com/'), 'runtime builtin default');
  check(defaultUuid().toString() == 'ffffffff-ffff-ffff-ffff-ffffffffffff', 'UUID default');
  check(Counter(3).value() == 3, 'unnamed constructor');
  check(Counter.fromValue(4).value() == 4, 'named constructor');
  check(Counter(0).nullable(null, 7) == 7, 'nullable method arguments');
  final uuid = $$BoltUUIDValue.parse('ffffffff-ffff-ffff-ffff-ffffffffffff');
  check(uuid.toString() == 'ffffffff-ffff-ffff-ffff-ffffffffffff', 'lossless UUID');
  check(echoUuid(uuid) == uuid, 'UUID wire conversion');
  final url = Uri.parse('https://example.com/a');
  check(echoUrl(url) == url, 'URL wire conversion');
  final time = DateTime.utc(2026, 1, 2);
  check(echoTime(time) == time, 'SystemTime wire conversion');
  const duration = Duration(seconds: 3, microseconds: 1234);
  check(echoDuration(duration) == duration, 'Duration wire conversion');
  check(callAdder(DartAdder()) == 7, 'Dart callback adapter');
  check(await maybeValue(false) == null && await maybeValue(true) == 42, 'async optional return');
  try { await asyncFallible(); throw StateError('expected async typed error'); }
  on Failure { }
  check(await collision() == 3 && await collision(cancellationToken: 4) == 4, 'cancellation parameter collision');
  final counter = Counter(0);
  final batch = counter.batches();
  check(batch.popBatch(2).join(',') == '1,2', 'batch stream API');
  batch.cancel();
  final streamValues = <int>[];
  final complete = Completer<void>();
  late StreamSubscription<int> subscription;
  subscription = counter.values().listen((value) {
    streamValues.add(value);
    if (value == 1) subscription.pause();
    if (value == 3) { subscription.cancel().then((_) => complete.complete()); }
  });
  await Future<void>.delayed(const Duration(milliseconds: 10));
  check(streamValues.join(',') == '1', 'paused stream must stop delivery');
  subscription.resume();
  await complete.future.timeout(const Duration(seconds: 1));
  check(streamValues.join(',') == '1,2,3', 'stream resumes in order');
  final notifications = <int>[];
  final callbackSubscription = counter.notifications(notifications.add);
  await Future<void>.delayed(const Duration(milliseconds: 10));
  check(notifications.join(',') == '1,2,3', 'callback stream API');
  await callbackSubscription.cancel();
  final token = $$BoltCancellationToken();
  try {
    await pending(() => token.cancel(), cancellationToken: token).timeout(const Duration(seconds: 1));
    throw StateError('expected cancellation');
  } on $$BoltCancelledException { }
  done();
}
