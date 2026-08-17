import 'dart:typed_data';

import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('callbacks', () async {
    final asyncFetcher = AsyncFetcherImpl();
    final asyncPointTransformer = AsyncPointTransformerImpl();
    final asyncOptionFetcher = AsyncOptionFetcherImpl();
    final asyncOptionalMessageFetcher = AsyncOptionalMessageFormatterImpl();
    final asyncResultFormatter = AsyncResultFormatterImpl();

    expect(await fetchWithAsyncCallback(asyncFetcher, 5), 500);
    expect(
      await fetchStringWithAsyncCallback(asyncFetcher, 'boltffi'),
      'BOLTFFI',
    );
    expect(
      await fetchJoinedMessageWithAsyncCallback(
        asyncFetcher,
        'async',
        'borrowed strings',
      ),
      'async::BORROWED STRINGS',
    );
    final asyncPoint = await transformPointWithAsyncCallback(
      asyncPointTransformer,
      Point(x: 1.0, y: 2.0),
    );
    expect(asyncPoint.x, 51.0);
    expect(asyncPoint.y, 62.0);

    expect(await invokeAsyncOptionFetcher(asyncOptionFetcher, 7), 7000);
    expect(await invokeAsyncOptionFetcher(asyncOptionFetcher, 0), isNull);
    expect(
      await invokeAsyncOptionalMessageFetcher(asyncOptionalMessageFetcher, 9),
      'async-message:9',
    );
    expect(
      await invokeAsyncOptionalMessageFetcher(asyncOptionalMessageFetcher, 0),
      isNull,
    );
    expect(
      await renderMessageWithAsyncResultCallback(
        asyncResultFormatter,
        'async',
        'result',
      ),
      'async::RESULT',
    );

    final asyncResultPoint = await transformPointWithAsyncResultCallback(
      asyncResultFormatter,
      Point(x: 3.0, y: 4.0),
      Status.active,
    );
    expect(asyncResultPoint.x, 503.0);
    expect(asyncResultPoint.y, 604.0);

    await expectLater(
      renderMessageWithAsyncResultCallback(
        asyncResultFormatter,
        '',
        'result',
      ),
      throwsA(MathError.negativeInput),
    );
    await expectLater(
      transformPointWithAsyncResultCallback(
        asyncResultFormatter,
        Point(x: 3.0, y: 4.0),
        Status.inactive,
      ),
      throwsA(MathError.negativeInput),
    );

    final doubler = DoublerValueCallbackImpl();
    final tripler = TriplerValueCallbackImpl();
    final incrementer = makeIncrementingCallback(5);
    final pointTransformer = PointTransformerImpl();
    final statusMapper = StatusMapperImpl();
    final flipper = makeStatusFlipper();
    final multiMethod = MultiMethodCallbackImpl();
    final optionCallback = OptionCallbackImpl();
    final resultCallback = ResultCallbackImpl();
    final falliblePointTransformer = FalliblePointTransformerImpl();
    final offsetCallback = OffsetCallbackImpl();
    final vecProcessor = VecProcessorImpl();
    final messageFormatter = MessageFormatterImpl();
    final optionalMessageCallback = OptionalMessageCallbackImpl();
    final resultMessageCallback = ResultMessageCallbackImpl();
    final stringResultMessageCallback = StringResultMessageCallbackImpl();

    expect(invokeValueCallback(doubler, 4), 8);
    expect(invokeValueCallbackTwice(doubler, 3, 4), 14);
    expect(invokeBoxedValueCallback(doubler, 5), 10);
    expect(incrementer.onValue(4), 9);
    expect(invokeValueCallback(incrementer, 4), 9);
    expect(invokeOptionalValueCallback(doubler, 4), 8);
    expect(invokeOptionalValueCallback(null, 4), 4);
    expect(mapStatus(statusMapper, Status.pending), Status.active);
    expect(flipper.mapStatus(Status.active), Status.inactive);
    expect(mapStatus(flipper, Status.inactive), Status.pending);
    expect(
      formatMessageWithCallback(messageFormatter, 'sync', 'borrowed strings'),
      'sync::BORROWED STRINGS',
    );
    expect(
      formatMessageWithBoxedCallback(
        messageFormatter,
        'boxed',
        'borrowed strings',
      ),
      'boxed::BORROWED STRINGS',
    );
    expect(
      formatMessageWithOptionalCallback(
        messageFormatter,
        'optional',
        'borrowed strings',
      ),
      'optional::BORROWED STRINGS',
    );
    expect(
      formatMessageWithOptionalCallback(null, 'fallback', 'message'),
      'fallback::message',
    );
    final prefixer = makeMessagePrefixer('prefix');
    expect(
      prefixer.formatMessage('scope', 'message'),
      'prefix::scope::message',
    );
    expect(
      formatMessageWithCallback(prefixer, 'sync', 'formatter'),
      'prefix::sync::formatter',
    );
    expect(
      invokeOptionalMessageCallback(optionalMessageCallback, 7),
      'message:7',
    );
    expect(invokeOptionalMessageCallback(optionalMessageCallback, 0), isNull);
    expect(invokeResultMessageCallback(resultMessageCallback, 8), 'message:8');
    expect(
      () => invokeResultMessageCallback(resultMessageCallback, -1),
      throwsA(MathError.negativeInput),
    );

    expect(
      invokeStringResultMessageCallback(stringResultMessageCallback, 8),
      'message:8',
      reason:
          "case:callbacks.sync_traits.string_result_message_callback.should_return_encoded_success",
    );
    expect(
      () => invokeStringResultMessageCallback(stringResultMessageCallback, -1),
      throwsBoltException("negative key: -1"),
      reason:
          "case:callbacks.sync_traits.string_result_message_callback.should_report_string_error",
    );

    final processed = processVec(vecProcessor, Int32List.fromList([1, 2, 3]));
    expect(processed.length, 3);
    expect(processed[0], 1);
    expect(processed[1], 4);
    expect(processed[2], 9);

    expect(invokeMultiMethod(multiMethod, 3, 4), 21);
    expect(invokeMultiMethodBoxed(multiMethod, 3, 4), 21);
    expect(invokeTwoCallbacks(doubler, tripler, 5), 25);

    expect(invokeOptionCallback(optionCallback, 7), 70);
    expect(invokeOptionCallback(optionCallback, 0), isNull);
    expect(invokeResultCallback(resultCallback, 7), 70);
    expect(
      () => invokeResultCallback(resultCallback, -1),
      throwsA(MathError.negativeInput),
    );
    expect(invokeOffsetCallback(offsetCallback, -5, 8), 3);
    expect(invokeBoxedOffsetCallback(offsetCallback, 10, 4), 14);

    final richPoint = invokeFalliblePointTransformer(
      falliblePointTransformer,
      Point(x: 2.0, y: 3.0),
      Status.active,
    );
    expect(richPoint.x, 102.0);
    expect(richPoint.y, 203.0);
    expect(
      () => invokeFalliblePointTransformer(
        falliblePointTransformer,
        Point(x: 2.0, y: 3.0),
        Status.inactive,
      ),
      throwsA(MathError.negativeInput),
    );

    final transformed = transformPoint(pointTransformer, Point(x: 1.0, y: 2.0));
    expect(transformed.x, 11.0);
    expect(transformed.y, 22.0);

    final transformedBoxed = transformPointBoxed(
      pointTransformer,
      Point(x: 3.0, y: 4.0),
    );
    expect(transformedBoxed.x, 13.0);
    expect(transformedBoxed.y, 24.0);

    final observedValue = [0];

    expect(applyClosure((value) => value * 2, 5), 10);
    applyVoidClosure((value) => observedValue[0] = value, 42);
    expect(observedValue[0], 42);
    expect(applyNullaryClosure(() => 99), 99);
    expect(applyStringClosure((val) => val.toUpperCase(), 'hello'), 'HELLO');
    expect(applyBoolClosure((value) => !value, true), isFalse);
    expect(
      applyF64Closure((value) => value * value, 3.0),
      closeTo(9.0, 0.0001),
    );
    expect(applyBinaryClosure((left, right) => left + right, 3, 4), 7);
    expect(applyOffsetClosure((value, delta) => value + delta, -5, 8), 3);
    expect(
      applyStatusClosure(
        (status) => status == Status.active ? Status.pending : Status.active,
        Status.active,
      ),
      Status.pending,
    );

    final optionalPoint = applyOptionalPointClosure(
      (point) =>
          point != null ? Point(x: point.x + 2.0, y: point.y + 3.0) : null,
      Point(x: 1.0, y: 2.0),
    );
    expect(optionalPoint, isNotNull);
    expect(optionalPoint!.x, 3.0);
    expect(optionalPoint.y, 5.0);
    expect(applyOptionalPointClosure((point) => point, null), isNull);

    expect(
      applyResultClosure((value) {
        if (value < 0) {
          throw MathError.negativeInput;
        }
        return value * 4;
      }, 6),
      24,
    );

    expect(
      () => applyResultClosure((value) {
        throw MathError.negativeInput;
      }, -1),
      throwsA(MathError.negativeInput),
    );

    final transformedPoint = applyPointClosure(
      (point) => Point(x: point.x + 1.0, y: point.y + 1.0),
      Point(x: 1.0, y: 2.0),
    );
    expect(transformedPoint.x, 2.0);
    expect(transformedPoint.y, 3.0);

    final mapped = mapVecWithClosure(
      (value) => value * 2,
      Int32List.fromList([1, 2, 3]),
    );
    expect(mapped.length, 3);
    expect(mapped[0], 2);
    expect(mapped[1], 4);
    expect(mapped[2], 6);

    final filtered = filterVecWithClosure(
      (value) => value % 2 == 0,
      Int32List.fromList([1, 2, 3, 4]),
    );
    expect(filtered.length, 2);
    expect(filtered[0], 2);
    expect(filtered[1], 4);
  });
}
