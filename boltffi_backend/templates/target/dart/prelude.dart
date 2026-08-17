import 'dart:async' as $$async;
import 'dart:collection' as $$collection;
import 'dart:convert' as $$convert;
import 'dart:typed_data' as $$typed_data;
import 'dart:ffi' as $$ffi;
import 'package:ffi/ffi.dart' as $$extffi;

/// Process-lifetime [NativeCallable.listener]s. Dart keeps the isolate
/// alive until every listener is [NativeCallable.close]d — a CLI that
/// never closes them hangs after `main` returns.
final _$$boltProcessListeners = <Object>[];

T _$$boltTrackListener<T>(T callable) {
  _$$boltProcessListeners.add(callable as Object);
  return callable;
}

/// Closes process-lifetime listeners so a Dart CLI can exit. Do not call
/// this while native code may still invoke those callbacks.
void shutdownBoltffi() {
  for (final callable in _$$boltProcessListeners) {
    (callable as dynamic).close();
  }
  _$$boltProcessListeners.clear();
}

final class _$$BoltFFIStatus extends $$ffi.Struct {
  @$$ffi.Int32()
  external int code;
}

final class _$$BoltFFIString extends $$ffi.Struct {
  external $$ffi.Pointer<$$ffi.Uint8> ptr;

  @$$ffi.UintPtr()
  external int len;

  @$$ffi.UintPtr()
  external int cap;
}

final class _$$BoltFFIBuf extends $$ffi.Struct {
  external $$ffi.Pointer<$$ffi.Uint8> ptr;

  @$$ffi.UintPtr()
  external int len;

  @$$ffi.UintPtr()
  external int cap;

  @$$ffi.UintPtr()
  external int align;
}

final class _$$BoltFFIError extends $$ffi.Struct {
  external _$$BoltFFIString message;
}

final class _$$BoltCallbackHandle extends $$ffi.Struct {
  @$$ffi.Uint64()
  external int handle;

  external $$ffi.Pointer<$$ffi.Void> vtable;
}

final _k$BoltCallbackHandleNull = $$ffi.Struct.create<_$$BoltCallbackHandle>();

final class _$$BoltReturnedClosureRegistration extends $$ffi.Struct {
  external $$ffi.Pointer<$$ffi.Void> invoke;
  external $$ffi.Pointer<$$ffi.Void> context;
  external $$ffi.Pointer<$$ffi.Void> release;
}

final class _$$BoltReturnedClosureRelease {
  final $$ffi.Pointer<$$ffi.Void> context;
  final $$ffi.Pointer<$$ffi.Void> function;

  const _$$BoltReturnedClosureRelease(this.context, this.function);

  void call() {
    if (function == $$ffi.nullptr) return;
    function
        .cast<
          $$ffi.NativeFunction<
            $$ffi.Void Function($$ffi.Pointer<$$ffi.Void>)
          >
        >()
        .asFunction<void Function($$ffi.Pointer<$$ffi.Void>)>()(context);
  }
}

final class _$$BoltReturnedClosureOwner {
  static final Finalizer<_$$BoltReturnedClosureRelease> _finalizer = Finalizer(
    (release) => release.call(),
  );

  final $$ffi.Pointer<$$ffi.Void> invoke;
  final $$ffi.Pointer<$$ffi.Void> context;

  _$$BoltReturnedClosureOwner(_$$BoltReturnedClosureRegistration registration)
    : invoke = registration.invoke,
      context = registration.context {
    _finalizer.attach(
      this,
      _$$BoltReturnedClosureRelease(registration.context, registration.release),
      detach: this,
    );
  }
}

final class $$BoltException implements Exception {
  final String message;

  const $$BoltException(this.message);

  factory $$BoltException._m$wireDecode(_$$BoltWireDecoder _p$reader) {
    return $$BoltException(_p$reader.readString());
  }

  void _m$wireEncode(_$$BoltWireEncoder _p$writer) {
    _p$writer.writeString(message);
  }

  int _m$wireEncodedSize() => (4 + (message.length * 3));

  @override
  String toString() {
    return '[BoltException] $message';
  }
}

final class _$$BoltCallocPtr<T extends $$ffi.SizedNativeType>
    implements $$ffi.Finalizable {
  final $$ffi.Pointer<T> ptr;
  final int len;

  static final _finalizer = $$ffi.NativeFinalizer($$extffi.calloc.nativeFree);

  _$$BoltCallocPtr._(this.ptr, this.len);

  factory _$$BoltCallocPtr.alloc(int size) {
    final o = _$$BoltCallocPtr._($$extffi.calloc.allocate<T>(size), size);
    _finalizer.attach(o, o.ptr.cast(), detach: o);
    return o;
  }

  factory _$$BoltCallocPtr.allocUnmanaged(int size) {
    final o = _$$BoltCallocPtr._($$extffi.calloc.allocate<T>(size), size);
    return o;
  }

  void dispose() {
    _finalizer.detach(this);
    $$extffi.calloc.free(ptr);
  }
}

abstract class _$$BoltStoragePool {
  static const int kMinCapacity = 1 << 3;
  static const int kMaxCapacity = 1 << 20;
  static const int kBucketCount = 18;
  static const int kCapacityPerBucket = 4;

  static final List<_$$BoltCallocPtr<$$ffi.Uint8>?> buckets = List.filled(kCapacityPerBucket * kBucketCount, null);
  static final $$typed_data.Uint8List counts = $$typed_data.Uint8List(kBucketCount);

  static int getBucketIndex(int capacity) {
    return (capacity - 1).bitLength - 3;
  }

  @pragma('vm:prefer-inline')
  static _$$BoltCallocPtr<$$ffi.Uint8> acquireStorage(int capacity) {
    if (capacity > kMaxCapacity) {
      return _$$BoltCallocPtr.alloc(capacity);
    }
    if (capacity < kMinCapacity) {
      capacity = kMinCapacity;
    }

    final bucketIdx = getBucketIndex(capacity);
    final bucketCount = counts[bucketIdx];

    if (bucketCount == 0) {
      return _$$BoltCallocPtr.alloc(kMinCapacity << bucketIdx);
    }

    final storageIdx = bucketIdx * kCapacityPerBucket + bucketCount - 1;
    final candidate = buckets[storageIdx]!;
    buckets[storageIdx] = null;
    counts[bucketIdx]--;
    return candidate;
  }

  @pragma('vm:never-inline')
  static void _disposeStorage(_$$BoltCallocPtr<$$ffi.Uint8> storage) {
    storage.dispose();
  }

  @pragma('vm:prefer-inline')
  static void releaseStorage(_$$BoltCallocPtr<$$ffi.Uint8> storage) {
    final len = storage.len;
    if (len > kMaxCapacity || len < kMinCapacity) {
      return _disposeStorage(storage);
    }
    final bucketIdx = getBucketIndex(len);
    final bucketCount = counts[bucketIdx];
    if (bucketCount == kCapacityPerBucket) {
      return _disposeStorage(storage);
    }
    buckets[bucketIdx * kCapacityPerBucket + bucketCount] = storage;
    counts[bucketIdx]++;
  }
}

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
      $$BoltResult$Err<Ok, Err>(:final value) => null,
    };
  }

  Err? errValue() {
    return switch (this) {
      $$BoltResult$Ok<Ok, Err>(:final value) => null,
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

abstract final class _$$BoltUtil {
  @pragma('vm:prefer-inline')
  static void writeU32le($$ffi.Pointer<$$ffi.Uint8> ptr, int value) {
    ptr[0] = value & 0xff;
    ptr[1] = (value >> 8) & 0xff;
    ptr[2] = (value >> 16) & 0xff;
    ptr[3] = (value >> 24) & 0xff;
  }

  /// Wire string: little-endian u32 byte length + UTF-8. ASCII skips encode.
  @pragma('vm:prefer-inline')
  static int writeI32le($$ffi.Pointer<$$ffi.Uint8> ptr, int offset, int v) {
    ptr[offset] = v & 0xff;
    ptr[offset + 1] = (v >> 8) & 0xff;
    ptr[offset + 2] = (v >> 16) & 0xff;
    ptr[offset + 3] = (v >> 24) & 0xff;
    return offset + 4;
  }

  @pragma('vm:prefer-inline')
  static int readI32le($$ffi.Pointer<$$ffi.Uint8> ptr, int offset) {
    final x =
        ptr[offset] |
        (ptr[offset + 1] << 8) |
        (ptr[offset + 2] << 16) |
        (ptr[offset + 3] << 24);
    return x >= 0x80000000 ? x - 0x100000000 : x;
  }

  @pragma('vm:prefer-inline')
  static int writeOptionI32($$ffi.Pointer<$$ffi.Uint8> ptr, int? v) {
    if (v == null) {
      ptr[0] = 0;
      return 1;
    }
    ptr[0] = 1;
    writeI32le(ptr, 1, v);
    return 5;
  }

  @pragma('vm:prefer-inline')
  static int? readOptionI32($$ffi.Pointer<$$ffi.Uint8> ptr, int len) {
    if (len < 1 || ptr[0] == 0) return null;
    return readI32le(ptr, 1);
  }

  @pragma('vm:prefer-inline')
  static int readU32le($$ffi.Pointer<$$ffi.Uint8> ptr, int offset) {
    return ptr[offset] |
        (ptr[offset + 1] << 8) |
        (ptr[offset + 2] << 16) |
        (ptr[offset + 3] << 24);
  }

  @pragma('vm:prefer-inline')
  static int writeOptionU32($$ffi.Pointer<$$ffi.Uint8> ptr, int? v) {
    if (v == null) {
      ptr[0] = 0;
      return 1;
    }
    ptr[0] = 1;
    writeI32le(ptr, 1, v);
    return 5;
  }

  @pragma('vm:prefer-inline')
  static int? readOptionU32($$ffi.Pointer<$$ffi.Uint8> ptr, int len) {
    if (len < 1 || ptr[0] == 0) return null;
    return readU32le(ptr, 1);
  }

  @pragma('vm:prefer-inline')
  static int writeOptionString(
    $$ffi.Pointer<$$ffi.Uint8> ptr,
    int cap,
    String? v,
  ) {
    if (v == null) {
      ptr[0] = 0;
      return 1;
    }
    ptr[0] = 1;
    return 1 + writeString(ptr.elementAt(1), cap - 1, v);
  }

  @pragma('vm:prefer-inline')
  static String? readOptionString($$ffi.Pointer<$$ffi.Uint8> ptr, int len) {
    if (len < 1 || ptr[0] == 0) return null;
    return readString(ptr.elementAt(1), len - 1);
  }

  @pragma('vm:prefer-inline')
  static int writeBytes(
    $$ffi.Pointer<$$ffi.Uint8> ptr,
    int cap,
    $$typed_data.Uint8List v,
  ) {
    writeU32le(ptr, v.length);
    ptr.asTypedList(cap).setRange(4, 4 + v.length, v);
    return 4 + v.length;
  }

  @pragma('vm:prefer-inline')
  static $$typed_data.Uint8List readBytes($$ffi.Pointer<$$ffi.Uint8> ptr, int len) {
    if (len < 4) return $$typed_data.Uint8List(0);
    final n = ptr[0] | (ptr[1] << 8) | (ptr[2] << 16) | (ptr[3] << 24);
    if (n <= 0 || 4 + n > len) return $$typed_data.Uint8List(0);
    return $$typed_data.Uint8List.fromList(
      $$typed_data.Uint8List.sublistView(ptr.asTypedList(len), 4, 4 + n),
    );
  }

  @pragma('vm:prefer-inline')
  static int writeStringList(
    $$ffi.Pointer<$$ffi.Uint8> ptr,
    int cap,
    List<String> v,
  ) {
    writeU32le(ptr, v.length);
    var offset = 4;
    for (final item in v) {
      offset += writeString(ptr.elementAt(offset), cap - offset, item);
    }
    return offset;
  }

  @pragma('vm:prefer-inline')
  static List<String> readStringList($$ffi.Pointer<$$ffi.Uint8> ptr, int len) {
    if (len < 4) return const <String>[];
    final n = ptr[0] | (ptr[1] << 8) | (ptr[2] << 16) | (ptr[3] << 24);
    if (n <= 0) return const <String>[];
    final out = List<String>.filled(n, '');
    var offset = 4;
    for (var i = 0; i < n; i++) {
      if (offset + 4 > len) break;
      final slen =
          ptr[offset] |
          (ptr[offset + 1] << 8) |
          (ptr[offset + 2] << 16) |
          (ptr[offset + 3] << 24);
      out[i] = readString(ptr.elementAt(offset), len - offset);
      offset += 4 + slen;
    }
    return out;
  }

  @pragma('vm:prefer-inline')
  static int writeString($$ffi.Pointer<$$ffi.Uint8> ptr, int cap, String v) {
    final units = v.length;
    writeU32le(ptr, units);
    final bytes = ptr.asTypedList(cap);
    for (var i = 0; i < units; i++) {
      final c = v.codeUnitAt(i);
      if (c > 127) {
        final encoded = $$convert.utf8.encode(v);
        writeU32le(ptr, encoded.length);
        bytes.setRange(4, 4 + encoded.length, encoded);
        return 4 + encoded.length;
      }
      bytes[4 + i] = c;
    }
    return 4 + units;
  }

  @pragma('vm:prefer-inline')
  static String readString($$ffi.Pointer<$$ffi.Uint8> ptr, int len) {
    if (len < 4) return '';
    final n = ptr[0] | (ptr[1] << 8) | (ptr[2] << 16) | (ptr[3] << 24);
    if (n <= 0 || 4 + n > len) return '';
    final view = ptr.asTypedList(len);
    for (var i = 0; i < n; i++) {
      if (view[4 + i] > 127) {
        return $$convert.utf8.decode(
          $$typed_data.Uint8List.sublistView(view, 4, 4 + n),
        );
      }
    }
    return String.fromCharCodes(view, 4, 4 + n);
  }

  @pragma('vm:prefer-inline')
  static bool listCompare<T>(List<T> a, List<T> b, bool Function(T, T) cmp) {
    if (identical(a, b)) return true;
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; ++i) {
      if (!cmp(a[i], b[i])) return false;
    }
    return true;
  }

  @pragma('vm:prefer-inline')
  static int listHash<T>(List<T> v, int Function(T) hasher) {
    int result = 1;
    for (final i in v) {
        result = 31 * result + hasher(i);
    }
    return result;
  }

  @pragma('vm:prefer-inline')
  static bool mapCompare<K, V>(
    Map<K, V> a,
    Map<K, V> b,
    bool Function(K, K) keyComparer,
    bool Function(V, V) valueComparer,
  ) {
    if (identical(a, b)) return true;
    if (a.length != b.length) return false;
    final remaining = b.entries.toList();
    return a.entries.every((left) {
      final index = remaining.indexWhere(
        (right) =>
            keyComparer(left.key, right.key) &&
            valueComparer(left.value, right.value),
      );
      if (index == -1) return false;
      remaining.removeAt(index);
      return true;
    });
  }

  @pragma('vm:prefer-inline')
  static int mapHash<K, V>(
    Map<K, V> value,
    int Function(K) keyHasher,
    int Function(V) valueHasher,
  ) {
    return value.entries.fold(
      0,
      (hash, entry) =>
          hash + Object.hash(keyHasher(entry.key), valueHasher(entry.value)),
    );
  }

  @pragma('vm:prefer-inline')
  static bool nullableCompare<T>(T? a, T? b, bool Function(T, T) comparer) {
    if (identical(a, b)) return true;
    if (a == null || b == null) return false;
    return comparer(a, b);
  }

  @pragma('vm:prefer-inline')
  static bool fallibleCompare<T, E extends Exception>($$BoltResult<T, E> a, $$BoltResult<T, E> b, bool Function(T, T) okCompare, bool Function(E, E) errCompare) {
    if (identical(a, b)) return true;
    return switch ((a, b)) {
      ($$BoltResult$Ok(value: final okA), $$BoltResult$Ok(value: final okB)) => okCompare(okA, okB),
      ($$BoltResult$Err(value: final errA), $$BoltResult$Err(value: final errB)) => errCompare(errA, errB),
      _ => false,
    };
  }

  static final $$typed_data.Uint8List _k$asciiToHex = () {
    final table = $$typed_data.Uint8List(128);
    table.fillRange(0, 128, 0xff);
    for (int i = 0; i < 10; i++) {
      table[48 + i] = i; // '0'-'9'
    }
    for (int i = 0; i < 6; i++) {
      table[97 + i] = 10 + i; // 'a'-'f'
      table[65 + i] = 10 + i; // 'A'-'F'
    }
    return table;
  }();

  static final $$typed_data.Uint8List _k$hexChars = $$typed_data.Uint8List.fromList([
    48, 49, 50, 51, 52, 53, 54, 55, 56, 57, // 0-9
    97, 98, 99, 100, 101, 102               // a-f
  ]);
}

final class $$BoltUUIDValue {
  final int highBits;
  final int lowBits;

  const $$BoltUUIDValue(this.highBits, this.lowBits);

  factory $$BoltUUIDValue.parse(String uuid) {
    if (uuid.length != 36) {
      throw const FormatException('Invalid UUID length');
    }
    if (uuid.codeUnitAt(8) != 45 ||
        uuid.codeUnitAt(13) != 45 ||
        uuid.codeUnitAt(18) != 45 ||
        uuid.codeUnitAt(23) != 45) {
      throw const FormatException('Invalid UUID hyphens');
    }

    final _asciiToHex = _$$BoltUtil._k$asciiToHex;

    final c0 = _asciiToHex[uuid.codeUnitAt(0) & 0x7f];
    final c1 = _asciiToHex[uuid.codeUnitAt(1) & 0x7f];
    final c2 = _asciiToHex[uuid.codeUnitAt(2) & 0x7f];
    final c3 = _asciiToHex[uuid.codeUnitAt(3) & 0x7f];
    final c4 = _asciiToHex[uuid.codeUnitAt(4) & 0x7f];
    final c5 = _asciiToHex[uuid.codeUnitAt(5) & 0x7f];
    final c6 = _asciiToHex[uuid.codeUnitAt(6) & 0x7f];
    final c7 = _asciiToHex[uuid.codeUnitAt(7) & 0x7f];

    final c8 = _asciiToHex[uuid.codeUnitAt(9) & 0x7f];
    final c9 = _asciiToHex[uuid.codeUnitAt(10) & 0x7f];
    final c10 = _asciiToHex[uuid.codeUnitAt(11) & 0x7f];
    final c11 = _asciiToHex[uuid.codeUnitAt(12) & 0x7f];

    final c12 = _asciiToHex[uuid.codeUnitAt(14) & 0x7f];
    final c13 = _asciiToHex[uuid.codeUnitAt(15) & 0x7f];
    final c14 = _asciiToHex[uuid.codeUnitAt(16) & 0x7f];
    final c15 = _asciiToHex[uuid.codeUnitAt(17) & 0x7f];

    final c16 = _asciiToHex[uuid.codeUnitAt(19) & 0x7f];
    final c17 = _asciiToHex[uuid.codeUnitAt(20) & 0x7f];
    final c18 = _asciiToHex[uuid.codeUnitAt(21) & 0x7f];
    final c19 = _asciiToHex[uuid.codeUnitAt(22) & 0x7f];

    final c20 = _asciiToHex[uuid.codeUnitAt(24) & 0x7f];
    final c21 = _asciiToHex[uuid.codeUnitAt(25) & 0x7f];
    final c22 = _asciiToHex[uuid.codeUnitAt(26) & 0x7f];
    final c23 = _asciiToHex[uuid.codeUnitAt(27) & 0x7f];
    final c24 = _asciiToHex[uuid.codeUnitAt(28) & 0x7f];
    final c25 = _asciiToHex[uuid.codeUnitAt(29) & 0x7f];
    final c26 = _asciiToHex[uuid.codeUnitAt(30) & 0x7f];
    final c27 = _asciiToHex[uuid.codeUnitAt(31) & 0x7f];
    final c28 = _asciiToHex[uuid.codeUnitAt(32) & 0x7f];
    final c29 = _asciiToHex[uuid.codeUnitAt(33) & 0x7f];
    final c30 = _asciiToHex[uuid.codeUnitAt(34) & 0x7f];
    final c31 = _asciiToHex[uuid.codeUnitAt(35) & 0x7f];

    final check = c0 | c1 | c2 | c3 | c4 | c5 | c6 | c7 | c8 | c9 | c10 | c11 | c12 | c13 | c14 | c15 | c16 | c17 | c18 | c19 | c20 | c21 | c22 | c23 | c24 | c25 | c26 | c27 | c28 | c29 | c30 | c31;

    if (check > 15) {
      throw const FormatException('Invalid hex character in UUID');
    }

    return $$BoltUUIDValue(
      (c0 << 60) | (c1 << 56) | (c2 << 52) | (c3 << 48) | (c4 << 44) | (c5 << 40) | (c6 << 36) | (c7 << 32) | (c8 << 28) | (c9 << 24) | (c10 << 20) | (c11 << 16) | (c12 << 12) |(c13 << 8) |(c14 << 4) | c15,
      (c16 << 60) | (c17 << 56) | (c18 << 52) | (c19 << 48) | (c20 << 44) | (c21 << 40) | (c22 << 36) | (c23 << 32) | (c24 << 28) | (c25 << 24) | (c26 << 20) | (c27 << 16) | (c28 << 12) | (c29 << 8) | (c30 << 4) | c31
    );
  }

  @override
  String toString() {
    final _hexChars = _$$BoltUtil._k$hexChars;

    final out = $$typed_data.Uint8List(36)
      ..[0] = _hexChars[(highBits >> 60) & 0x0f]
      ..[1] = _hexChars[(highBits >> 56) & 0x0f]
      ..[2] = _hexChars[(highBits >> 52) & 0x0f]
      ..[3] = _hexChars[(highBits >> 48) & 0x0f]
      ..[4] = _hexChars[(highBits >> 44) & 0x0f]
      ..[5] = _hexChars[(highBits >> 40) & 0x0f]
      ..[6] = _hexChars[(highBits >> 36) & 0x0f]
      ..[7] = _hexChars[(highBits >> 32) & 0x0f]
      ..[8] = 45
      ..[9] = _hexChars[(highBits >> 28) & 0x0f]
      ..[10] = _hexChars[(highBits >> 24) & 0x0f]
      ..[11] = _hexChars[(highBits >> 20) & 0x0f]
      ..[12] = _hexChars[(highBits >> 16) & 0x0f]
      ..[13] = 45
      ..[14] = _hexChars[(highBits >> 12) & 0x0f]
      ..[15] = _hexChars[(highBits >> 8) & 0x0f]
      ..[16] = _hexChars[(highBits >> 4) & 0x0f]
      ..[17] = _hexChars[highBits & 0x0f]
      ..[18] = 45
      ..[19] = _hexChars[(lowBits >> 60) & 0x0f]
      ..[20] = _hexChars[(lowBits >> 56) & 0x0f]
      ..[21] = _hexChars[(lowBits >> 52) & 0x0f]
      ..[22] = _hexChars[(lowBits >> 48) & 0x0f]
      ..[23] = 45
      ..[24] = _hexChars[(lowBits >> 44) & 0x0f]
      ..[25] = _hexChars[(lowBits >> 40) & 0x0f]
      ..[26] = _hexChars[(lowBits >> 36) & 0x0f]
      ..[27] = _hexChars[(lowBits >> 32) & 0x0f]
      ..[28] = _hexChars[(lowBits >> 28) & 0x0f]
      ..[29] = _hexChars[(lowBits >> 24) & 0x0f]
      ..[30] = _hexChars[(lowBits >> 20) & 0x0f]
      ..[31] = _hexChars[(lowBits >> 16) & 0x0f]
      ..[32] = _hexChars[(lowBits >> 12) & 0x0f]
      ..[33] = _hexChars[(lowBits >> 8) & 0x0f] 
      ..[34] = _hexChars[(lowBits >> 4) & 0x0f] 
      ..[35] = _hexChars[lowBits & 0x0f];

    return String.fromCharCodes(out);
  }

  @override
  int get hashCode => Object.hash(highBits, lowBits);

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;

    return other is $$BoltUUIDValue && highBits == other.highBits && lowBits == other.lowBits;
  }
}

final class _$$BoltBufWriter {
  final $$typed_data.Uint8List bytes;
  final $$typed_data.ByteData data;
  int _offset = 0;

  _$$BoltBufWriter._(this.bytes) : data = $$typed_data.ByteData.sublistView(bytes);

  _$$BoltBufWriter([int capacity = 256]) : this._($$typed_data.Uint8List(capacity));

  factory _$$BoltBufWriter.fromSpan($$ffi.Pointer<$$ffi.Uint8> ptr, int len) {
    return _$$BoltBufWriter._(ptr.asTypedList(len));
  }

  int get len => _offset;

  set len(int value) => _offset = value;

  @pragma('vm:prefer-inline')
  void reset() => _offset = 0;

  @pragma('vm:prefer-inline')
  int advance(int size) {
    final start = _offset;
    _offset += size;
    return start;
  }

  @pragma('vm:prefer-inline')
  void writeU8(int v, int offset) => data.setUint8(offset, v);

  @pragma('vm:prefer-inline')
  void writeI8(int v, int offset) => data.setInt8(offset, v);

  @pragma('vm:prefer-inline')
  void writeU16(int v, int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.setUint16(offset, v, endian);

  @pragma('vm:prefer-inline')
  void writeI16(int v, int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.setInt16(offset, v, endian);

  @pragma('vm:prefer-inline')
  void writeU32(int v, int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.setUint32(offset, v, endian);

  @pragma('vm:prefer-inline')
  void writeI32(int v, int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.setInt32(offset, v, endian);

  @pragma('vm:prefer-inline')
  void writeU64(int v, int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.setUint64(offset, v, endian);

  @pragma('vm:prefer-inline')
  void writeI64(int v, int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.setInt64(offset, v, endian);

  @pragma('vm:prefer-inline')
  void writeF32(double v, int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.setFloat32(offset, v, endian);

  @pragma('vm:prefer-inline')
  void writeF64(double v, int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.setFloat64(offset, v, endian);

  @pragma('vm:prefer-inline')
  void writeBytes($$typed_data.TypedData v, int offset) {
    final length = v.lengthInBytes;
    bytes.setRange(offset, offset + length, v.buffer.asUint8List(v.offsetInBytes, length));
  }

  @pragma('vm:prefer-inline')
  void writeBool(bool v, int offset) => writeU8(v ? 1 : 0, offset);
}


extension type _$$BoltWireEncoder(_$$BoltBufWriter writer) {
  @pragma('vm:prefer-inline')
  int get len => writer.len;

  @pragma('vm:prefer-inline')
  void writeU8(int v) => writer.writeU8(v, writer.advance(1));

  @pragma('vm:prefer-inline')
  void writeI8(int v) => writer.writeI8(v, writer.advance(1));

  @pragma('vm:prefer-inline')
  void writeU16(int v, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => writer.writeU16(v, writer.advance(2), endian);

  @pragma('vm:prefer-inline')
  void writeI16(int v, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => writer.writeI16(v, writer.advance(2), endian);

  @pragma('vm:prefer-inline')
  void writeU32(int v, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => writer.writeU32(v, writer.advance(4), endian);

  @pragma('vm:prefer-inline')
  void writeI32(int v, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => writer.writeI32(v, writer.advance(4), endian);

  @pragma('vm:prefer-inline')
  void writeU64(int v, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => writer.writeU64(v, writer.advance(8), endian);

  @pragma('vm:prefer-inline')
  void writeI64(int v, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => writer.writeI64(v, writer.advance(8), endian);

  @pragma('vm:prefer-inline')
  void writeF32(double v, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => writer.writeF32(v, writer.advance(4), endian);

  @pragma('vm:prefer-inline')
  void writeF64(double v, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => writer.writeF64(v, writer.advance(8), endian);

  @pragma('vm:prefer-inline')
  void writeBool(bool v) => writeU8(v ? 1 : 0);

  @pragma('vm:prefer-inline')
  void writeBytes($$typed_data.TypedData v) {
    final byteLen = v.lengthInBytes;
    writeU32(byteLen ~/ v.elementSizeInBytes);
    writer.writeBytes(v, writer.advance(byteLen));
  }

  @pragma('vm:prefer-inline')
  void writeList<T>(
    List<T> v,
    void Function(T item, _$$BoltWireEncoder encoder) onItem,
  ) {
    final len = v.length;
    writeU32(len);
    for (int i = 0; i < len; ++i) {
      onItem(v[i], this);
    }
  }

  @pragma('vm:prefer-inline')
  void writeString(String v) {
    final units = v.length;
    final start = writer.len;
    writeU32(units);
    final offset = writer.advance(units);
    final bytes = writer.bytes;
    for (var i = 0; i < units; i++) {
      final c = v.codeUnitAt(i);
      if (c > 127) {
        writer.len = start;
        writeBytes($$convert.utf8.encode(v));
        return;
      }
      bytes[offset + i] = c;
    }
  }

  @pragma('vm:prefer-inline')
  void writeUri(Uri v) => writeString(v.toString());

  @pragma('vm:prefer-inline')
  void writeDuration(Duration v) {
    final micros = v.inMicroseconds;
    final seconds = micros ~/ 1000000;
    final nanos = (micros % 1000000) * 1000;
    writeU64(seconds);
    writeU32(nanos);
  }

  @pragma('vm:prefer-inline')
  void writeInstant(DateTime v) {
    final micros = v.microsecondsSinceEpoch;
    final subsecondMicros = micros % 1000000;
    final seconds = (micros - subsecondMicros) ~/ 1000000;
    final nanos = subsecondMicros * 1000;
    writeI64(seconds);
    writeU32(nanos);
  }

  @pragma('vm:prefer-inline')
  void writeUUID($$BoltUUIDValue v) {
    writeU64(v.highBits);
    writeU64(v.lowBits);
  }
}

const _k$unexpectedCallbackErrorMarker = <int>[
  0x42, 0x4f, 0x4c, 0x54, 0x46, 0x46, 0x49, 0x5f,
  0x43, 0x41, 0x4c, 0x4c, 0x42, 0x41, 0x43, 0x4b,
];

_$$BoltFFIBuf _f$encodeUnexpectedCallbackError(Object error) {
  final message = error.toString();
  final encoded = $$convert.utf8.encode(message);
  final size = _k$unexpectedCallbackErrorMarker.length + 1 + 4 + encoded.length;
  final storage = _$$BoltCallocPtr<$$ffi.Uint8>.alloc(size);
  final writer = _$$BoltWireEncoder(
    _$$BoltBufWriter.fromSpan(storage.ptr, storage.len),
  );
  for (final byte in _k$unexpectedCallbackErrorMarker) {
    writer.writeU8(byte);
  }
  writer.writeU8(1);
  writer.writeString(message);
  return _f$boltffi_buf_from_bytes(storage.ptr, writer.len);
}

final class _$$BoltBufReader {
  final $$typed_data.Uint8List bytes;
  final $$typed_data.ByteData data;
  int _offset = 0;

  _$$BoltBufReader(this.bytes): data = $$typed_data.ByteData.sublistView(bytes);

  factory _$$BoltBufReader.fromSpan($$ffi.Pointer<$$ffi.Uint8> ptr, int len) {
    return _$$BoltBufReader(ptr.asTypedList(len));
  }

  @pragma('vm:prefer-inline')
  void ensureCapacity(int size) {
    if (_offset + size > bytes.lengthInBytes) {
      throw StateError("Buffer overflow");
    }
  }

  int get len => _offset;

  @pragma('vm:prefer-inline')
  int advance(int size) {
    ensureCapacity(size);
    final start = _offset;
    _offset += size;
    return start;
  }

  @pragma('vm:prefer-inline')
  int readU8(int offset) => data.getUint8(offset);

  @pragma('vm:prefer-inline')
  int readI8(int offset) => data.getInt8(offset);

  @pragma('vm:prefer-inline')
  int readU16(int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.getUint16(offset, endian);

  @pragma('vm:prefer-inline')
  int readI16(int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.getInt16(offset, endian);

  @pragma('vm:prefer-inline')
  int readU32(int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.getUint32(offset, endian);

  @pragma('vm:prefer-inline')
  int readI32(int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.getInt32(offset, endian);

  @pragma('vm:prefer-inline')
  int readU64(int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.getUint64(offset, endian);

  @pragma('vm:prefer-inline')
  int readI64(int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.getInt64(offset, endian);

  @pragma('vm:prefer-inline')
  double readF32(int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.getFloat32(offset, endian);

  @pragma('vm:prefer-inline')
  double readF64(int offset, [$$typed_data.Endian endian = $$typed_data.Endian.little]) => data.getFloat64(offset, endian);

  @pragma('vm:prefer-inline')
  $$typed_data.Uint8List readBytes(int size, int offset) {
    final dst = $$typed_data.Uint8List(size);
    dst.setRange(0, size, bytes, offset);
    return dst;
  }

  @pragma('vm:prefer-inline')
  bool readBool(int offset) => readU8(offset) != 0;

  @pragma('vm:prefer-inline')
  $$typed_data.Uint8List readUint8List(int bytesLen, int offset) => readBytes(bytesLen, offset);

  @pragma('vm:prefer-inline')
  $$typed_data.Int8List readInt8List(int bytesLen, int offset) =>
      readBytes(bytesLen, offset).buffer.asInt8List();

  @pragma('vm:prefer-inline')
  $$typed_data.Uint16List readUint16List(int bytesLen, int offset) =>
      readBytes(bytesLen, offset).buffer.asUint16List();

  @pragma('vm:prefer-inline')
  $$typed_data.Int16List readInt16List(int bytesLen, int offset) =>
      readBytes(bytesLen, offset).buffer.asInt16List();

  @pragma('vm:prefer-inline')
  $$typed_data.Uint32List readUint32List(int bytesLen, int offset) =>
      readBytes(bytesLen, offset).buffer.asUint32List();

  @pragma('vm:prefer-inline')
  $$typed_data.Int32List readInt32List(int bytesLen, int offset) =>
      readBytes(bytesLen, offset).buffer.asInt32List();

  @pragma('vm:prefer-inline')
  $$typed_data.Uint64List readUint64List(int bytesLen, int offset) =>
      readBytes(bytesLen, offset).buffer.asUint64List();

  @pragma('vm:prefer-inline')
  $$typed_data.Int64List readInt64List(int bytesLen, int offset) =>
      readBytes(bytesLen, offset).buffer.asInt64List();

  @pragma('vm:prefer-inline')
  $$typed_data.Float32List readFloat32List(int bytesLen, int offset) =>
      readBytes(bytesLen, offset).buffer.asFloat32List();

  @pragma('vm:prefer-inline')
  $$typed_data.Float64List readFloat64List(int bytesLen, int offset) =>
      readBytes(bytesLen, offset).buffer.asFloat64List();

  @pragma('vm:prefer-inline')
  $$BoltBoolList readBoolList(int bytesLen, int offset) =>
      $$BoltBoolList._m$fromUint8List(readBytes(bytesLen, offset));
}


extension type _$$BoltWireDecoder(_$$BoltBufReader reader) {
  @pragma('vm:prefer-inline')
  int readU8() => reader.readU8(reader.advance(1));

  @pragma('vm:prefer-inline')
  int readI8() => reader.readI8(reader.advance(1));

  @pragma('vm:prefer-inline')
  int readU16([$$typed_data.Endian endian = $$typed_data.Endian.little]) => reader.readU16(reader.advance(2), endian);

  @pragma('vm:prefer-inline')
  int readI16([$$typed_data.Endian endian = $$typed_data.Endian.little]) => reader.readI16(reader.advance(2), endian);

  @pragma('vm:prefer-inline')
  int readU32([$$typed_data.Endian endian = $$typed_data.Endian.little]) => reader.readU32(reader.advance(4), endian);

  @pragma('vm:prefer-inline')
  int readI32([$$typed_data.Endian endian = $$typed_data.Endian.little]) => reader.readI32(reader.advance(4), endian);

  @pragma('vm:prefer-inline')
  int readU64([$$typed_data.Endian endian = $$typed_data.Endian.little]) => reader.readU64(reader.advance(8), endian);

  @pragma('vm:prefer-inline')
  int readI64([$$typed_data.Endian endian = $$typed_data.Endian.little]) => reader.readI64(reader.advance(8), endian);

  @pragma('vm:prefer-inline')
  double readF32([$$typed_data.Endian endian = $$typed_data.Endian.little]) => reader.readF32(reader.advance(4), endian);

  @pragma('vm:prefer-inline')
  double readF64([$$typed_data.Endian endian = $$typed_data.Endian.little]) => reader.readF64(reader.advance(8), endian);

  @pragma('vm:prefer-inline')
  bool readBool() => readU8() != 0;

  @pragma('vm:prefer-inline')
  $$typed_data.Uint8List readUint8List() {
    final size = readU32();
    return reader.readBytes(size, reader.advance(size));
  }

  @pragma('vm:prefer-inline')
  $$typed_data.Int8List readInt8List() {
    final size = readU32();
    return reader.readBytes(size, reader.advance(size)).buffer.asInt8List();
  }

  @pragma('vm:prefer-inline')
  $$typed_data.Uint16List readUint16List() {
    final len = readU32();
    final size = len * 2;
    return reader.readBytes(size, reader.advance(size)).buffer.asUint16List();
  }

  @pragma('vm:prefer-inline')
  $$typed_data.Int16List readInt16List() {
    final len = readU32();
    final size = len * 2;
    return reader.readBytes(size, reader.advance(size)).buffer.asInt16List();
  }

  @pragma('vm:prefer-inline')
  $$typed_data.Uint32List readUint32List() {
    final len = readU32();
    final size = len * 4;
    return reader.readBytes(size, reader.advance(size)).buffer.asUint32List();
  }

  @pragma('vm:prefer-inline')
  $$typed_data.Int32List readInt32List() {
    final len = readU32();
    final size = len * 4;
    return reader.readBytes(size, reader.advance(size)).buffer.asInt32List();
  }

  @pragma('vm:prefer-inline')
  $$typed_data.Uint64List readUint64List() {
    final len = readU32();
    final size = len * 8;
    return reader.readBytes(size, reader.advance(size)).buffer.asUint64List();
  }

  @pragma('vm:prefer-inline')
  $$typed_data.Int64List readInt64List() {
    final len = readU32();
    final size = len * 8;
    return reader.readBytes(size, reader.advance(size)).buffer.asInt64List();
  }

  @pragma('vm:prefer-inline')
  $$typed_data.Float32List readFloat32List() {
    final len = readU32();
    final size = len * 4;
    return reader.readBytes(size, reader.advance(size)).buffer.asFloat32List();
  }

  @pragma('vm:prefer-inline')
  $$typed_data.Float64List readFloat64List() {
    final len = readU32();
    final size = len * 8;
    return reader.readBytes(size, reader.advance(size)).buffer.asFloat64List();
  }

  @pragma('vm:prefer-inline')
  $$BoltBoolList readBoolList() {
    final len = readU32();
    return $$BoltBoolList._m$fromUint8List(reader.readBytes(len, reader.advance(len)));
  }

  @pragma('vm:prefer-inline')
  List<T> readList<T>(T Function(_$$BoltWireDecoder decoder) readValue) {
    return List.generate(readU32(), (_) => readValue(this));
  }

  @pragma('vm:prefer-inline')
  Map<K, V> readMap<K, V>(
    K Function(_$$BoltWireDecoder decoder) readKey,
    V Function(_$$BoltWireDecoder decoder) readValue,
  ) {
    return Map.fromEntries(List.generate(readU32(), (_) {
      final decoder = _$$BoltWireDecoder(reader);
      return MapEntry(readKey(decoder), readValue(decoder));
    }));
  }

  @pragma('vm:prefer-inline')
  String readString() {
    final size = readU32();
    final offset = reader.advance(size);
    final view = $$typed_data.Uint8List.sublistView(
      reader.bytes,
      offset,
      offset + size,
    );
    for (var i = 0; i < size; i++) {
      if (view[i] > 127) {
        return $$convert.utf8.decode(view);
      }
    }
    return String.fromCharCodes(view);
  }

  @pragma('vm:prefer-inline')
  Uri readUri() => Uri.parse(readString());

  @pragma('vm:prefer-inline')
  Duration readDuration() {
    final seconds = readU64();
    final nanos = readU32();
    return Duration(microseconds: (seconds * 1000000) + (nanos ~/ 1000));
  }

  @pragma('vm:prefer-inline')
  DateTime readInstant() {
    final seconds = readI64();
    final nanos = readU32();
    return DateTime.fromMicrosecondsSinceEpoch((seconds * 1000000) + (nanos ~/ 1000));
  }

  @pragma('vm:prefer-inline')
  $$BoltUUIDValue readUUID() {
    return $$BoltUUIDValue(readU64(), readU64());
  }
}

final class _$$BoltFFIHandleMap<O> {
  final Map<int, O> _map = {};
  int _counter = 1;

  int insert(O value) {
    final int handle = _counter + 2;
    _counter = handle;
    _map[handle] = value;
    return handle;
  }

  // Handle is the Hooks pointer from register; do not mint a second id.
  void insertAt(int handle, O value) => _map[handle] = value;

  O? get(int handle) => _map[handle];

  O? remove(int handle) => _map.remove(handle);

  int clone(int handle) {
    final obj = _map[handle];

    if (obj == null) {
      return 0;
    }

    return insert(obj);
  }
}

final class $$BoltCancellationToken {
  final Set<void Function()> _attached = {};
  bool _cancelled = false;

  bool get isCancelled => _cancelled;

  void cancel() {
    if (_cancelled) return;
    _cancelled = true;
    for (final onCancel in _attached.toList()) {
      onCancel();
    }
    _attached.clear();
  }

  void _attach(void Function() onCancel) => _attached.add(onCancel);
  void _detach(void Function() onCancel) => _attached.remove(onCancel);
}

final class $$BoltCancelledException implements Exception {
  const $$BoltCancelledException();

  @override
  String toString() => 'BoltFFI call was cancelled';
}

final class _$$BoltAsyncWait {
  final $$ffi.Pointer<$$ffi.Void> handle;
  final void Function(Object? value) complete;
  final void Function(Object error) completeError;
  final Object? Function($$ffi.Pointer<$$ffi.Void> handle) completeFuture;
  final void Function($$ffi.Pointer<$$ffi.Void> handle) freeFuture;
  final int Function(
    $$ffi.Pointer<$$ffi.Void> handle,
    int callback_data,
    $$ffi.Pointer<
      $$ffi.NativeFunction<$$ffi.Void Function($$ffi.Uint64, $$ffi.Int8)>
    >
    callback,
  )
  pollFuture;
  bool cancelled;
  final $$BoltCancellationToken? token;
  final void Function()? onCancel;

  _$$BoltAsyncWait({
    required this.handle,
    required this.complete,
    required this.completeError,
    required this.completeFuture,
    required this.freeFuture,
    required this.pollFuture,
    required this.cancelled,
    this.token,
    this.onCancel,
  });
}

final class _$$BoltFFIAsync {
  static const int _k$RustFuturePoll$Ready = 0;
  static const int _k$RustFuturePoll$MaybeReady = 1;

  static int _nextId = 1;
  static final _pending = <int, _$$BoltAsyncWait>{};
  static $$ffi.NativeCallable<$$ffi.Void Function($$ffi.Uint64, $$ffi.Int8)>?
  _pollCallable;
  static final $$ffi.Pointer<
    $$ffi.NativeFunction<$$ffi.Void Function($$ffi.Uint64, $$ffi.Int8)>
  >
  _noopNative = $$ffi.Native.addressOf<
    $$ffi.NativeFunction<$$ffi.Void Function($$ffi.Uint64, $$ffi.Int8)>
  >(_f$poll_continuation_noop);

  static $$ffi.Pointer<
    $$ffi.NativeFunction<$$ffi.Void Function($$ffi.Uint64, $$ffi.Int8)>
  >
  _pollNative() {
    // Recreate on demand; closed again when `_pending` empties so a CLI that
    // only awaited BoltFFI futures can exit without `shutdownBoltffi()`.
    if (_pollCallable == null) {
      final callable = $$ffi.NativeCallable<
        $$ffi.Void Function($$ffi.Uint64, $$ffi.Int8)
      >.listener(_onPoll);
      _$$boltTrackListener(callable);
      _pollCallable = callable;
    }
    return _pollCallable!.nativeFunction;
  }

  static void _releasePollCallableIfIdle() {
    if (_pending.isNotEmpty) return;
    final callable = _pollCallable;
    if (callable == null) return;
    _pollCallable = null;
    _$$boltProcessListeners.remove(callable);
    callable.close();
  }

  static void _onPoll(int data, int res) {
    final wait = _pending[data];
    if (wait == null) return;
    switch (res) {
      case _k$RustFuturePoll$Ready:
        _pending.remove(data);
        final onCancel = wait.onCancel;
        if (onCancel != null) {
          wait.token?._detach(onCancel);
        }
        if (wait.cancelled) {
          try {
            wait.completeError(const $$BoltCancelledException());
          } finally {
            wait.freeFuture(wait.handle);
          }
          _releasePollCallableIfIdle();
          return;
        }
        try {
          wait.complete(wait.completeFuture(wait.handle));
        } catch (err) {
          wait.completeError(err);
        } finally {
          wait.freeFuture(wait.handle);
        }
        _releasePollCallableIfIdle();
      case _k$RustFuturePoll$MaybeReady:
        wait.pollFuture(wait.handle, data, _pollNative());
      case _:
        throw $$BoltException("Unexpected poll result: $res");
    }
  }

  static Future<R> create<R>({
    required $$ffi.Pointer<$$ffi.Void> Function() createFuture,
    required int Function(
      $$ffi.Pointer<$$ffi.Void> handle,
      int callback_data,
      $$ffi.Pointer<
        $$ffi.NativeFunction<$$ffi.Void Function($$ffi.Uint64, $$ffi.Int8)>
      >
      callback,
    )
    pollFuture,
    required R Function($$ffi.Pointer<$$ffi.Void> handle) completeFuture,
    required void Function($$ffi.Pointer<$$ffi.Void> handle) freeFuture,
    required void Function($$ffi.Pointer<$$ffi.Void> handle) cancelFuture,
    $$BoltCancellationToken? cancellationToken,
  }) {
    if (cancellationToken?.isCancelled ?? false) {
      return $$async.Future<R>.error(const $$BoltCancelledException());
    }
    final handle = createFuture();
    if (cancellationToken?.isCancelled ?? false) {
      try {
        cancelFuture(handle);
      } finally {
        freeFuture(handle);
      }
      return $$async.Future<R>.error(const $$BoltCancelledException());
    }
    final completer = $$async.Completer<R>();
    final id = _nextId++;
    late final _$$BoltAsyncWait wait;
    void onTokenCancel() {
      if (wait.cancelled) return;
      wait.cancelled = true;
      cancelFuture(handle);
    }
    wait = _$$BoltAsyncWait(
      handle: handle,
      complete: (value) => completer.complete(value as R),
      completeError: completer.completeError,
      completeFuture: completeFuture,
      freeFuture: freeFuture,
      pollFuture: pollFuture,
      cancelled: false,
      token: cancellationToken,
      onCancel: cancellationToken == null ? null : onTokenCancel,
    );
    _pending[id] = wait;
    cancellationToken?._attach(onTokenCancel);
    final status = pollFuture(handle, id, _pollNative());
    if (status == _k$RustFuturePoll$Ready) {
      _onPoll(id, status);
    }
    return completer.future;
  }
}

final class $$BoltStreamPopBatchHandle<O> {
  List<O> Function(int batchSize) popBatch;
  void Function() cancel;

  $$BoltStreamPopBatchHandle({required this.popBatch, required this.cancel});
}

final class _$$BoltStreamCtx {
  static const _k$StreamPollResult$Ready = 0;
  static const _k$StreamPollResult$Closed = 1;

  static const _k$defaultBatchSize = 16;
  // Caps how many batches one readiness wake drains before yielding back to
  // the event queue -- an unbounded drain loop would starve timers,
  // cancellation, and other isolate work for as long as a producer keeps
  // refilling as fast as it's drained.
  static const _k$maxBatchesPerWake = 32;

  late final int Function() subscribe;
  late final void Function(
    int,
    int,
    $$ffi.Pointer<
      $$ffi.NativeFunction<$$ffi.Void Function($$ffi.Uint64, $$ffi.Int8)>
    >,
  )
  pollFn;
  late final int Function(int, int) waitFn;
  late final void Function(int) unsubscribeFn;
  late final void Function(int) freeFn;
  late final int? itemSize;

  _$$BoltStreamCtx({
    required this.subscribe,
    required this.pollFn,
    required this.waitFn,
    required this.unsubscribeFn,
    required this.freeFn,
    this.itemSize,
  });

  Stream<O> stream<O>(
    // Returns whether the batch it just delivered was full (i.e. there may
    // be more items already buffered on the Rust side worth reading
    // immediately, without paying for another `NativeCallable.listener`
    // round-trip through the event queue).
    bool Function(
      int handle,
      int batchSize,
      int? itemSize,
      $$async.StreamController<O> controller,
    )
    onReady,
  ) {
    final handle = subscribe();
    var active = true;
    var released = false;
    late final $$ffi.NativeCallable<
      $$ffi.Void Function($$ffi.Uint64, $$ffi.Int8)
    >
    streamCallbackCallable;

    void release() {
      if (released) return;
      released = true;
      streamCallbackCallable.close();
      freeFn(handle);
    }

    final controller = $$async.StreamController<O>(
      onCancel: () {
        if (!active) return;
        active = false;
        unsubscribeFn(handle);
        release();
      },
    );

    void streamCallback(int data, int res) {
      if (!active) return;
      switch (res) {
        case _k$StreamPollResult$Ready:
          var batches = 0;
          var more = true;
          while (active &&
              more &&
              batches < _k$maxBatchesPerWake) {
            more = onReady(handle, _k$defaultBatchSize, itemSize, controller);
            batches++;
          }
          if (!active) break;
          if (more) {
            // More is likely already buffered -- keep draining without
            // paying for another native poll round-trip, but only after
            // yielding this turn so other microtasks/timers get a chance.
            $$async.scheduleMicrotask(() => streamCallback(data, res));
          } else {
            pollFn(handle, 0, streamCallbackCallable.nativeFunction);
          }
        case _k$StreamPollResult$Closed:
          while (onReady(handle, _k$defaultBatchSize, itemSize, controller)) {}
          active = false;
          unsubscribeFn(handle);
          release();
          controller.close();
      }
    }

    streamCallbackCallable = $$ffi.NativeCallable.listener(streamCallback);

    pollFn(handle, 0, streamCallbackCallable.nativeFunction);

    return controller.stream;
  }

  $$BoltStreamPopBatchHandle<O> batch<O>(
    List<O> Function(int, int, int?) mapper,
  ) {
    var handle = subscribe();

    return $$BoltStreamPopBatchHandle(
      popBatch: (batchSize) {
        if (handle == 0) {
          return [];
        }
        return mapper(handle, batchSize, itemSize);
      },
      cancel: () {
        if (handle == 0) return;
        unsubscribeFn(handle);
        freeFn(handle);
        handle = 0;
      },
    );
  }
}

// Symbols from `boltffi_dart_runtime`, linked into the user's cdylib.
@$$ffi.Native<$$ffi.Void Function($$ffi.Uint64, $$ffi.Int8)>(
  symbol: 'poll_continuation_noop',
)
external void _f$poll_continuation_noop(int data, int res);

@$$ffi.Native<$$ffi.Void Function($$ffi.Pointer<$$ffi.Void>)>(
  symbol: 'signal_gate_ok',
)
external void _f$signal_gate_ok($$ffi.Pointer<$$ffi.Void> gate);

@$$ffi.Native<$$ffi.Void Function($$ffi.Pointer<$$ffi.Void>)>(
  symbol: 'signal_gate_error',
)
external void _f$signal_gate_error($$ffi.Pointer<$$ffi.Void> gate);
