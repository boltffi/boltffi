sealed class $$BoltResult<Ok, Err extends Exception> {
  const $$BoltResult();

  factory $$BoltResult.ok(Ok value) = $$BoltResult$Ok;

  factory $$BoltResult.err(Err value) = $$BoltResult$Err;

  Ok okOrThrow() {
    return switch (this) {
      $$BoltResult$Ok<Ok, Err>(:final value) => value,
      $$BoltResult$Err<Ok, Err>(:final value) => throw value,
    };
  }

  $$BoltResult<Ok, MErr> mapError<MErr extends Exception>(
    MErr Function(Err) m,
  ) {
    return switch (this) {
      $$BoltResult$Ok<Ok, Err>(:final value) => $$BoltResult.ok(value),
      $$BoltResult$Err<Ok, Err>(:final value) => $$BoltResult.err(m(value)),
    };
  }

  Ok? okValue() {
    return switch (this) {
      $$BoltResult$Ok<Ok, Err>(:final value) => value,
      $$BoltResult$Err<Ok, Err>() => null,
    };
  }

  Err? errValue() {
    return switch (this) {
      $$BoltResult$Ok<Ok, Err>() => null,
      $$BoltResult$Err<Ok, Err>(:final value) => value,
    };
  }
}

final class $$BoltResult$Ok<Ok, Err extends Exception>
    extends $$BoltResult<Ok, Err> {
  final Ok value;

  const $$BoltResult$Ok(this.value);
}

final class $$BoltResult$Err<Ok, Err extends Exception>
    extends $$BoltResult<Ok, Err> {
  final Err value;

  const $$BoltResult$Err(this.value);
}

final class $$BoltBoolList extends $$collection.ListBase<bool> {
  final $$typed_data.Uint8List _bytes;

  $$BoltBoolList(int length) : _bytes = $$typed_data.Uint8List(length);

  $$BoltBoolList._m$fromUint8List($$typed_data.Uint8List data) : _bytes = data;

  $$BoltBoolList.fromList(Iterable<bool> values)
    : _bytes = $$typed_data.Uint8List.fromList(
        values.map((v) => v ? 1 : 0).toList(),
      );

  int get lengthInBytes => _bytes.length;

  @override
  int get length => _bytes.length;

  @override
  set length(int newLength) => throw UnsupportedError("Fixed Length");

  @override
  bool operator [](int index) => _bytes[index] != 0;

  @override
  void operator []=(int index, bool value) {
    _bytes[index] = value ? 1 : 0;
  }
}
