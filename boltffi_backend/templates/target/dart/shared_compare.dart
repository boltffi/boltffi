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
