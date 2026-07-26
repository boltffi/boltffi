final class BoltFFIValueIdentity {
    private BoltFFIValueIdentity() {}

    static <T> boolean optionalEquals(
        java.util.Optional<T> left,
        java.util.Optional<T> right,
        java.util.function.BiPredicate<T, T> equals
    ) {
        if (left == right) return true;
        if (left == null || right == null) return false;
        if (left.isPresent() != right.isPresent()) return false;
        return !left.isPresent() || equals.test(left.get(), right.get());
    }

    static <T> int optionalHash(
        java.util.Optional<T> value,
        java.util.function.ToIntFunction<T> hash
    ) {
        if (value == null || !value.isPresent()) return 0;
        return 31 + hash.applyAsInt(value.get());
    }

    static <T> boolean sequenceEquals(
        java.util.List<T> left,
        java.util.List<T> right,
        java.util.function.BiPredicate<T, T> equals
    ) {
        if (left == right) return true;
        if (left == null || right == null || left.size() != right.size()) return false;
        int index = 0;
        while (index < left.size()) {
            if (!equals.test(left.get(index), right.get(index))) return false;
            index += 1;
        }
        return true;
    }

    static <T> int sequenceHash(
        java.util.List<T> values,
        java.util.function.ToIntFunction<T> hash
    ) {
        if (values == null) return 0;
        int result = 1;
        int index = 0;
        while (index < values.size()) {
            result = 31 * result + hash.applyAsInt(values.get(index));
            index += 1;
        }
        return result;
    }
}
