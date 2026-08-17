import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('records with strings', () {
    final person = makePerson('Alice', 30);
    expect(
      person,
      isA<Person>()
          .having((p) => p.name, "`Person` name property", equals('Alice'))
          .having((p) => p.age, "`Person` age property", equals(30)),
      reason: 'case:records.with_strings.person.should_make_from_fields',
    );

    expect(
      echoPerson(person),
      person,
      reason: 'case:records.with_strings.person.should_roundtrip_value',
    );

    expect(
      greetPerson(person),
      'Hello, Alice! You are 30 years old.',
      reason: 'case:records.with_strings.person.should_format_greeting',
    );

    final emojiPerson = makePerson('🎉 Party', 25);
    expect(emojiPerson.name, '🎉 Party');

    final echoedEmojiPerson = echoPerson(emojiPerson);
    expect(echoedEmojiPerson.name, '🎉 Party');

    final address = Address(
      street: '123 Main St',
      city: 'Springfield',
      zip: '12345',
    );

    expect(
      echoAddress(address),
      address,
      reason: 'case:records.with_strings.address.should_roundtrip_value',
    );

    expect(
      formatAddress(address),
      '123 Main St, Springfield, 12345',
      reason: 'case:records.with_strings.address.should_format_value',
    );
  });
}
