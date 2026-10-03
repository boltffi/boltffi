import 'dart:io';

import 'package:demo/demo.dart';
import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';

void main() {
  boltffiTestHooks();

  test('scalar and string defaults allow named overrides', () {
    expect(
      repeatGreeting('ada'),
      'hello ada, hello ada',
      reason:
          'case:primitives.default_arguments.should_apply_omitted_scalar_and_string_defaults',
    );
    expect(repeatGreeting('ada', shout: true), 'HELLO ADA, HELLO ADA');
    expect(
      repeatGreeting('ada', greeting: 'hi', times: 1, shout: true),
      'HI ADA',
    );
    expect(repeatGreeting('ada', times: 1), 'hello ada');
  });

  test('optional defaults preserve explicit null', () {
    expect(
      describeLimit(),
      'none:7',
      reason:
          'case:primitives.default_arguments.should_apply_none_and_value_defaults_to_optionals',
    );
    expect(describeLimit(label: 'jobs'), 'jobs:7');
    expect(describeLimit(limit: null), 'none:unlimited');
    expect(describeLimit(label: 'jobs', limit: 2), 'jobs:2');
  });

  test('optional callbacks and closures default to absent handles', () {
    expect(
      applyOptionalCallback(4),
      4,
      reason:
          'case:primitives.default_arguments.should_default_an_optional_callback_to_none',
    );
    expect(applyOptionalCallback(4, callback: null), 4);
    expect(applyOptionalCallback(4, callback: DoublerValueCallbackImpl()), 8);
    expect(applyOptionalClosure(4), 4);
    expect(applyOptionalClosure(4, callback: null), 4);
    expect(applyOptionalClosure(4, callback: (value) => value * 3), 12);
  });

  test('constructors and methods use their Rust defaults', () {
    final counter = DefaultedCounter();
    final explicit = DefaultedCounter(start: 5);
    final fromText = DefaultedCounter.fromText();
    final adjusted = DefaultedCounter.withOffset(offset: 3);
    final explicitAdjusted = DefaultedCounter.withOffset(start: 5, offset: 3);
    addTearDown(counter.dispose$);
    addTearDown(explicit.dispose$);
    addTearDown(fromText.dispose$);
    addTearDown(adjusted.dispose$);
    addTearDown(explicitAdjusted.dispose$);

    expect(
      counter.offset(),
      11,
      reason:
          'case:primitives.default_arguments.defaulted_counter.should_apply_constructor_and_method_defaults',
    );
    expect(explicit.offset(step: 3), 8);
    expect(fromText.offset(), 41);
    expect(adjusted.offset(), 24);
    expect(explicitAdjusted.offset(), 9);
  });

  test('float and enum defaults preserve nullable overrides', () {
    expect(
      scaleDefault(),
      0.75,
      reason:
          'case:primitives.default_arguments.should_apply_float_and_enum_defaults',
    );
    expect(scaleDefault(weight: null), 0.5);
    expect(scaleDefault(mode: DefaultMode.loud), 1.5);
    expect(scaleDefault(ratio: 0.25, weight: 2, mode: DefaultMode.loud), 1);
    expect(DefaultMode.quiet.matches(), isTrue);
    expect(DefaultMode.loud.matches(), isFalse);
    expect(DefaultMode.loud.matches(mode: DefaultMode.loud), isTrue);
    expect(chooseDefault(), 0);
    expect(chooseDefault(second: DefaultChoice.value(value0: 4)), -4);
    expect(chooseDefault(first: DefaultChoice.value(value0: 5)), 5);
    expect(chooseDefault(second: null), 0);
  });

  test('integer limits and negative zero retain their bits', () {
    expect(defaultFloatBits(), 0x80000000);
    expect(defaultDoubleBits(), 0x8000000000000000);
    expect(defaultFloatBits(value: 0), 0);
    expect(defaultDoubleBits(value: 0), 0);
    final limits = DefaultedCounter.integerLimits();
    expect(limits.lower, -9223372036854775808);
    expect(limits.upper, 0xffffffffffffffff);
  });

  test('record fields and record callables keep separate defaults', () {
    expect(
      DefaultAmount().value,
      3,
      reason: 'case:primitives.default_arguments.should_apply_record_defaults',
    );
    expect(DefaultAmount().offset(), 5);
    expect(DefaultAmount(value: 5).offset(step: 3), 8);
    expect(DefaultAmount.withScaledValue().value, 4);
    expect(DefaultAmount.tryScaledValue()?.value, 4);
    expect(DefaultAmount.tryScaledValue(value: -1), isNull);
    expect(NamedAmount.withValue().value, 5);
    expect(NamedAmount.withValue(value: 9).value, 9);
  });

  test('custom defaults follow their representations', () {
    expect(
      defaultTimeoutSeconds(),
      1.5,
      reason:
          'case:primitives.default_arguments.should_apply_custom_type_defaults',
    );
    final timeout = TimeoutFfi(seconds: 2.5);
    expect(defaultTimeoutSeconds(timeout: timeout), 2.5);
    timeout.seconds = 7;
    expect(defaultTimeoutSeconds(timeout: timeout), 7);
    expect(defaultTimeoutSeconds(), 1.5);
    expect(defaultLimit(), isNull);
    expect(defaultLimit(limit: 7), 7);
    expect(defaultEmail(), 'mailto:ada@example.com');
    expect(
      defaultEmail(email: 'mailto:grace@example.com'),
      'mailto:grace@example.com',
    );
    expect(defaultOptionalEmail(), 'mailto:ada@example.com');
    expect(defaultOptionalEmail(email: null), isNull);
  });

  test(
    'async functions methods and factories allow omitted arguments',
    () async {
      expect(
        await asyncDefault(),
        9,
        reason: 'case:primitives.default_arguments.should_apply_async_defaults',
      );
      expect(await asyncDefault(value: 5), 5);
      expect(
        await asyncDefault(cancellationToken: $$BoltCancellationToken()),
        9,
      );
      final counter = DefaultedCounter();
      final started = await DefaultedCounter.start();
      final withCallback = await DefaultedCounter.start(
        second: DoublerValueCallbackImpl(),
      );
      addTearDown(counter.dispose$);
      addTearDown(started.dispose$);
      addTearDown(withCallback.dispose$);

      expect(await counter.asyncOffset(), 13);
      expect(await counter.asyncOffset(step: 5), 15);
      expect(started.offset(), 31);
      expect(withCallback.offset(), 61);
      expect((await NamedAmount.load()).value, 6);
      expect((await NamedAmount.load(value: 8)).value, 8);
      expect(await DefaultMode.load(), DefaultMode.quiet);
      expect(await DefaultMode.load(mode: DefaultMode.loud), DefaultMode.loud);
    },
  );

  test(
    'generated defaults keep argument types and requiredness checked',
    () async {
      final directory = await Directory(
        '.dart_tool',
      ).createTemp('default-arguments-');
      try {
        final valid = File('${directory.path}/valid.dart');
        await valid.writeAsString(r'''
import 'package:demo/demo.dart';

void main() {
  repeatGreeting('ada', shout: true);
  describeLimit(limit: null);
  DefaultedCounter.withOffset(offset: 3).dispose$();
  defaultTimeoutSeconds();
  defaultTimeoutSeconds(timeout: TimeoutFfi(seconds: 2.5));
  asyncDefault(value: 3, cancellationToken: $$BoltCancellationToken());
}
''');
        final accepted = await Process.run(Platform.resolvedExecutable, [
          'analyze',
          '--format=machine',
          valid.path,
        ]);
        expect(
          accepted.exitCode,
          0,
          reason: '${accepted.stdout}\n${accepted.stderr}',
        );

        final invalid = File('${directory.path}/invalid.dart');
        await invalid.writeAsString(r'''
import 'package:demo/demo.dart';

void main() {
  DefaultedCounter.withOffset();
  repeatGreeting();
  repeatGreeting('ada', shout: 'yes');
  defaultTimeoutSeconds(timeout: 'timeout');
}
''');
        final rejected = await Process.run(Platform.resolvedExecutable, [
          'analyze',
          '--format=machine',
          invalid.path,
        ]);
        expect(rejected.exitCode, isNot(0));
        final diagnostics = rejected.stdout.toString();
        expect(diagnostics, contains('MISSING_REQUIRED_ARGUMENT'));
        expect(diagnostics, contains('NOT_ENOUGH_POSITIONAL_ARGUMENTS'));
        expect(diagnostics.split('ARGUMENT_TYPE_NOT_ASSIGNABLE').length - 1, 2);
      } finally {
        await directory.delete(recursive: true);
      }
    },
  );
}
