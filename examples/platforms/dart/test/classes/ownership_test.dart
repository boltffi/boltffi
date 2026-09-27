import 'package:test/test.dart';
import 'package:demo/demo.dart';
import 'package:demo_dart/testing.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('ownership survives success, Rust errors, and cancelled borrows', () async {
    final drops = MessageDrops();
    final message = OwnedMessage('hello', drops);
    expect(consumeMessage(message), 5);
    message.dispose$();
    expect(drops.count(), 1);
    expect(() => message.length(), throwsA(isA<Exception>()));
    final rejected = OwnedMessage('', drops);
    expect(() => consumeMessageResult(rejected), throwsA(isA<Exception>()));
    rejected.dispose$();
    expect(drops.count(), 2);
    final duplicate = OwnedMessage('duplicate', drops);
    expect(
      () => consumeMessages(duplicate, duplicate),
      throwsBoltException('Object has been disposed'),
    );
    duplicate.dispose$();
    expect(drops.count(), 3);
    final store = MessageStore();
    final stored = OwnedMessage('stored', drops);
    await store.$set(stored, {'trace': 'context'});
    stored.dispose$();
    expect(store.count(), 7);
    expect(drops.count(), 4);
    store.dispose$();
    final retained = OwnedMessage('retained', drops);
    final cancellation = $$BoltCancellationToken();
    final pending = holdMessage(retained, cancellationToken: cancellation);
    final deadline = DateTime.now().add(const Duration(seconds: 5));
    while (drops.borrowCount() == 0 && DateTime.now().isBefore(deadline)) {
      await Future<void>.delayed(Duration.zero);
    }
    expect(drops.borrowCount(), 1);
    expect(consumeMessage(retained), 0);
    retained.dispose$();
    expect(drops.count(), 4);
    final cancelled = expectLater(pending, throwsA(isA<$$BoltCancelledException>()));
    cancellation.cancel();
    await cancelled;
    expect(drops.borrowCount(), 0);
    expect(drops.count(), 5);
    retained.dispose$();
    expect(drops.count(), 5);
    drops.dispose$();
  });
}
