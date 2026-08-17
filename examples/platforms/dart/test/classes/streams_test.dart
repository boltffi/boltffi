import 'dart:typed_data';

import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('streams', () async {
    // Encoded record items stream test
    {
      final received = <StreamMessage>[];
      final bus = EventBus();
      try {
        final subscription = bus.subscribeMessages().listen((message) {
          received.add(message);
        });
        final first = StreamMessage(
          text: 'alpha',
          values: Int32List.fromList([1, 2]),
        );
        final second = StreamMessage(
          text: 'beta',
          values: Int32List.fromList([3, 4, 5]),
        );
        bus.emitMessage(first);
        bus.emitMessage(second);

        await Future.delayed(Duration(milliseconds: 100));
        expect(
          received.length,
          2,
          reason:
              "case:classes.streams.event_bus.subscribe_messages.should_deliver_encoded_record_items",
        );
        expect(received, contains(first));
        expect(received, contains(second));
        subscription.cancel();
      } finally {
        bus.dispose$();
      }
    }

    // Async mode stream test
    {
      final received = <int>[];
      final bus = EventBus();
      try {
        final subscription = bus.subscribeValues().listen((value) {
          received.add(value);
        });
        bus.emitValue(10);
        bus.emitValue(20);
        bus.emitValue(30);

        await Future.delayed(Duration(milliseconds: 100));
        expect(received.length, greaterThanOrEqualTo(3));
        expect(received, containsAll([10, 20, 30]));
        subscription.cancel();
      } finally {
        bus.dispose$();
      }
    }

    // Batch mode stream test
    {
      final bus = EventBus();
      try {
        final subscription = bus.subscribeValuesBatch();
        bus.emitValue(100);
        bus.emitValue(200);
        bus.emitValue(300);

        await Future.delayed(Duration(milliseconds: 100));

        final batch = subscription.popBatch(16);
        expect(batch.length, greaterThanOrEqualTo(3));
        expect(batch, containsAll([100, 200, 300]));
        subscription.cancel();
      } finally {
        bus.dispose$();
      }
    }

    // Callback mode stream test
    {
      final received = <int>[];
      final bus = EventBus();
      try {
        final subscription = bus.subscribeValuesCallback((value) {
          received.add(value);
        });
        bus.emitValue(1000);
        bus.emitValue(2000);
        bus.emitValue(3000);

        await Future.delayed(Duration(milliseconds: 100));
        expect(received.length, greaterThanOrEqualTo(3));
        expect(received, containsAll([1000, 2000, 3000]));
        subscription.cancel();
      } finally {
        bus.dispose$();
      }
    }
  });
}
