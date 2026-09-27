import 'package:demo/demo.dart';
import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';

final class Receiver implements MessageReceiver, FallibleMessageReceiver {
  OwnedMessage? first;
  OwnedMessage? second;
  bool fail = false;

  @override
  void attach(OwnedMessage handle, int callback) {
    expect(callback, 42);
    first = handle;
  }

  @override
  void optional(OwnedMessage? handle) => first = handle;

  @override
  int pair(OwnedMessage first, String label, OwnedMessage? second) {
    expect(label, 'pair');
    this.first = first;
    this.second = second;
    if (fail) throw MathError.negativeInput;
    return first.length() + (second?.length() ?? 0);
  }
}

void main() {
  tearDownAll(shutdownBoltffi);
  test('callback messages retain independent ownership', () {
    final drops = MessageDrops();
    final receiver = Receiver();
    deliverMessage(receiver, drops);
    expect(drops.count(), 0);
    expect(
      receiver.first!.length(),
      9,
      reason: 'case:callbacks.class_handles.should_retain_after_return',
    );
    receiver.first!.dispose$();
    receiver.first!.dispose$();
    expect(drops.count(), 1);

    expect(
      deliverMessagePair(receiver, drops, true),
      11,
      reason:
          'case:callbacks.class_handles.should_deliver_multiple_and_optional',
    );
    expect(drops.count(), 1);
    expect(receiver.first!.length(), 5);
    expect(receiver.second!.length(), 6);
    receiver.first!.dispose$();
    expect(drops.count(), 2);
    receiver.second!.dispose$();
    expect(drops.count(), 3);
    expect(deliverMessagePair(receiver, drops, false), 5);
    expect(receiver.second, isNull);
    receiver.first!.dispose$();
    expect(drops.count(), 4);

    receiver.fail = true;
    expect(
      () => deliverMessagePair(receiver, drops, true),
      throwsA(MathError.negativeInput),
      reason: 'case:callbacks.class_handles.should_retain_after_error',
    );
    expect(drops.count(), 4);
    expect(receiver.first!.length(), 5);
    expect(receiver.second!.length(), 6);
    receiver.first!.dispose$();
    receiver.second!.dispose$();
    expect(drops.count(), 6);

    final measuring = makeMessageReceiver();
    final moved = OwnedMessage('moved', drops);
    measuring.attach(moved, 42);
    moved.dispose$();
    expect(
      drops.count(),
      7,
      reason: 'case:callbacks.class_handles.should_consume_in_rust_callback',
    );
    final optional = OwnedMessage('optional', drops);
    measuring.optional(optional);
    optional.dispose$();
    measuring.optional(null);
    expect(drops.count(), 8);
    drops.dispose$();
  });

  test('Rust callbacks reject disposed optional messages', () {
    final drops = MessageDrops();
    final receiver = makeMessageReceiver();
    final message = OwnedMessage('disposed', drops);
    message.dispose$();
    expect(drops.count(), 1);
    expect(
      () => receiver.optional(message),
      throwsBoltException('Object has been disposed'),
    );
    receiver.optional(null);
    expect(drops.count(), 1);
    drops.dispose$();
  });

  test('Rust callbacks reject messages whose ownership was already moved', () {
    final drops = MessageDrops();
    final receiver = makeMessageReceiver();
    final message = OwnedMessage('moved', drops);
    receiver.attach(message, 42);
    expect(drops.count(), 1);
    expect(
      () => receiver.attach(message, 42),
      throwsBoltException('Object has been disposed'),
    );
    message.dispose$();
    expect(drops.count(), 1);
    drops.dispose$();
  });
}
