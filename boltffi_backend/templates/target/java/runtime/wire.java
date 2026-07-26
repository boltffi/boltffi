@FunctionalInterface
interface WireRead<T> {
    T read();
}

@FunctionalInterface
interface WireWrite<T> {
    void write(T value);
}

@FunctionalInterface
interface WireSize<T> {
    int size(T value);
}

final class WireReader {
    private final java.nio.ByteBuffer buffer;

    WireReader(byte[] bytes) {
        this.buffer = java.nio.ByteBuffer
            .wrap(java.util.Objects.requireNonNull(bytes, "null buffer returned"))
            .order(java.nio.ByteOrder.LITTLE_ENDIAN);
    }

    boolean readBoolean() { return buffer.get() != 0; }
    byte readByte() { return buffer.get(); }
    short readShort() { return buffer.getShort(); }
    int readInt() { return buffer.getInt(); }
    long readLong() { return buffer.getLong(); }
    float readFloat() { return buffer.getFloat(); }
    double readDouble() { return buffer.getDouble(); }

    java.time.Duration readDuration() {
        long seconds = readLong();
        int nanos = readInt();
        if (seconds < 0 || nanos < 0) {
            throw new IllegalArgumentException("duration out of range");
        }
        return java.time.Duration.ofSeconds(seconds, nanos);
    }

    java.time.Instant readInstant() {
        long seconds = readLong();
        int nanos = readInt();
        if (nanos < 0) {
            throw new IllegalArgumentException("instant nanos out of range");
        }
        return java.time.Instant.ofEpochSecond(seconds, nanos);
    }

    java.util.UUID readUuid() {
        return new java.util.UUID(readLong(), readLong());
    }

    java.net.URI readUri() { return java.net.URI.create(readString()); }

    String readString() {
        int length = readLength();
        String value = new String(
            buffer.array(),
            buffer.arrayOffset() + buffer.position(),
            length,
            java.nio.charset.StandardCharsets.UTF_8
        );
        buffer.position(buffer.position() + length);
        return value;
    }

    byte[] readBytes() {
        int length = readLength();
        byte[] value = new byte[length];
        buffer.get(value);
        return value;
    }

    <T> java.util.Optional<T> readOptional(WireRead<T> read) {
        return readBoolean()
            ? java.util.Optional.ofNullable(read.read())
            : java.util.Optional.empty();
    }

    <T> java.util.List<T> readSequence(WireRead<T> read) {
        int length = readCount();
        java.util.ArrayList<T> values = new java.util.ArrayList<>(length);
        int index = 0;
        while (index < length) {
            values.add(read.read());
            index += 1;
        }
        return values;
    }

    <K, V> java.util.Map<K, V> readMap(WireRead<K> readKey, WireRead<V> readValue) {
        int length = readCount();
        java.util.HashMap<K, V> values = new java.util.HashMap<>(mapCapacity(length));
        int index = 0;
        while (index < length) {
            K key = readKey.read();
            if (values.containsKey(key)) {
                throw new IllegalArgumentException("duplicate wire map key");
            }
            values.put(key, readValue.read());
            index += 1;
        }
        return values;
    }

    java.util.List<String> readStringSequence() {
        int length = readCount();
        java.util.ArrayList<String> values = new java.util.ArrayList<>(length);
        int index = 0;
        while (index < length) {
            values.add(readString());
            index += 1;
        }
        return values;
    }

    boolean[] readBooleanArray() {
        int length = readCount();
        boolean[] values = new boolean[length];
        int index = 0;
        while (index < length) {
            values[index] = readBoolean();
            index += 1;
        }
        return values;
    }

    byte[] readByteArray() { return readBytes(); }

    short[] readShortArray() {
        int length = readArrayLength(2);
        short[] values = new short[length];
        int byteCount = length * 2;
        buffer.asShortBuffer().get(values);
        buffer.position(buffer.position() + byteCount);
        return values;
    }

    int[] readIntArray() {
        int length = readArrayLength(4);
        int[] values = new int[length];
        int byteCount = length * 4;
        buffer.asIntBuffer().get(values);
        buffer.position(buffer.position() + byteCount);
        return values;
    }

    long[] readLongArray() {
        int length = readArrayLength(8);
        long[] values = new long[length];
        int byteCount = length * 8;
        buffer.asLongBuffer().get(values);
        buffer.position(buffer.position() + byteCount);
        return values;
    }

    float[] readFloatArray() {
        int length = readArrayLength(4);
        float[] values = new float[length];
        int byteCount = length * 4;
        buffer.asFloatBuffer().get(values);
        buffer.position(buffer.position() + byteCount);
        return values;
    }

    double[] readDoubleArray() {
        int length = readArrayLength(8);
        double[] values = new double[length];
        int byteCount = length * 8;
        buffer.asDoubleBuffer().get(values);
        buffer.position(buffer.position() + byteCount);
        return values;
    }

    private int readLength() {
        int length = buffer.getInt();
        if (length < 0 || length > buffer.remaining()) {
            throw new IllegalArgumentException("invalid wire length");
        }
        return length;
    }

    private int readCount() {
        int count = buffer.getInt();
        if (count < 0) {
            throw new IllegalArgumentException("invalid wire count");
        }
        return count;
    }

    private int readArrayLength(int width) {
        int length = readCount();
        if (length > buffer.remaining() / width) {
            throw new IllegalArgumentException("invalid wire array length");
        }
        return length;
    }

    private int mapCapacity(int count) {
        return count < 3 ? count + 1 : Math.min(1 << 30, (int) Math.ceil(count / 0.75d));
    }
}

final class BoltFfiErrorBufferException extends RuntimeException {
    private final byte[] bytes;

    BoltFfiErrorBufferException(byte[] bytes) {
        super("BoltFFI call failed");
        this.bytes = bytes;
    }

    byte[] bytes() { return bytes; }
}

final class WireWriter {
    private final java.nio.ByteBuffer buffer;

    WireWriter(java.nio.ByteBuffer buffer) {
        this.buffer = buffer;
    }

    int size() { return buffer.position(); }
    void writeBoolean(boolean value) { buffer.put(value ? (byte) 1 : (byte) 0); }
    void writeByte(byte value) { buffer.put(value); }
    void writeShort(short value) { buffer.putShort(value); }
    void writeInt(int value) { buffer.putInt(value); }
    void writeLong(long value) { buffer.putLong(value); }
    void writeFloat(float value) { buffer.putFloat(value); }
    void writeDouble(double value) { buffer.putDouble(value); }

    void writeDuration(java.time.Duration value) {
        if (value.isNegative()) {
            throw new IllegalArgumentException("duration must be non-negative");
        }
        writeLong(value.getSeconds());
        writeInt(value.getNano());
    }

    void writeInstant(java.time.Instant value) {
        writeLong(value.getEpochSecond());
        writeInt(value.getNano());
    }

    void writeUuid(java.util.UUID value) {
        writeLong(value.getMostSignificantBits());
        writeLong(value.getLeastSignificantBits());
    }

    void writeUri(java.net.URI value) { writeString(value.toString()); }

    void writeString(String value) {
        writeBytes(value.getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }

    void writeBytes(byte[] value) {
        writeInt(value.length);
        buffer.put(value);
    }

    <T> void writeOptional(java.util.Optional<T> value, WireWrite<T> write) {
        writeBoolean(value.isPresent());
        if (value.isPresent()) {
            write.write(value.get());
        }
    }

    <T> void writeSequence(java.util.List<T> values, WireWrite<T> write) {
        writeInt(values.size());
        int index = 0;
        while (index < values.size()) {
            write.write(values.get(index));
            index += 1;
        }
    }

    <K, V> void writeMap(
        java.util.Map<K, V> values,
        WireWrite<K> writeKey,
        WireWrite<V> writeValue
    ) {
        writeInt(values.size());
        java.util.Iterator<java.util.Map.Entry<K, V>> entries = values.entrySet().iterator();
        while (entries.hasNext()) {
            java.util.Map.Entry<K, V> entry = entries.next();
            writeKey.write(entry.getKey());
            writeValue.write(entry.getValue());
        }
    }

    void writeStringSequence(java.util.List<String> values) {
        writeInt(values.size());
        int index = 0;
        while (index < values.size()) {
            writeString(values.get(index));
            index += 1;
        }
    }

    void writeBooleanArray(boolean[] values) {
        writeInt(values.length);
        int index = 0;
        while (index < values.length) {
            writeBoolean(values[index]);
            index += 1;
        }
    }

    void writeByteArray(byte[] values) { writeBytes(values); }

    void writeShortArray(short[] values) {
        writeInt(values.length);
        int byteCount = Math.multiplyExact(values.length, 2);
        buffer.asShortBuffer().put(values);
        buffer.position(buffer.position() + byteCount);
    }

    void writeIntArray(int[] values) {
        writeInt(values.length);
        int byteCount = Math.multiplyExact(values.length, 4);
        buffer.asIntBuffer().put(values);
        buffer.position(buffer.position() + byteCount);
    }

    void writeLongArray(long[] values) {
        writeInt(values.length);
        int byteCount = Math.multiplyExact(values.length, 8);
        buffer.asLongBuffer().put(values);
        buffer.position(buffer.position() + byteCount);
    }

    void writeFloatArray(float[] values) {
        writeInt(values.length);
        int byteCount = Math.multiplyExact(values.length, 4);
        buffer.asFloatBuffer().put(values);
        buffer.position(buffer.position() + byteCount);
    }

    void writeDoubleArray(double[] values) {
        writeInt(values.length);
        int byteCount = Math.multiplyExact(values.length, 8);
        buffer.asDoubleBuffer().put(values);
        buffer.position(buffer.position() + byteCount);
    }
}

final class WireSizes {
    private WireSizes() {}

    static int string(String value) {
        return Math.addExact(4, Math.multiplyExact(value.length(), 3));
    }

    static <T> int optional(java.util.Optional<T> value, WireSize<T> size) {
        return value.isPresent() ? Math.addExact(1, size.size(value.get())) : 1;
    }

    static <T> int sequence(java.util.List<T> values, WireSize<T> size) {
        int total = 4;
        int index = 0;
        while (index < values.size()) {
            total = Math.addExact(total, size.size(values.get(index)));
            index += 1;
        }
        return total;
    }

    static <K, V> int map(
        java.util.Map<K, V> values,
        WireSize<K> keySize,
        WireSize<V> valueSize
    ) {
        int total = 4;
        java.util.Iterator<java.util.Map.Entry<K, V>> entries = values.entrySet().iterator();
        while (entries.hasNext()) {
            java.util.Map.Entry<K, V> entry = entries.next();
            total = Math.addExact(total, keySize.size(entry.getKey()));
            total = Math.addExact(total, valueSize.size(entry.getValue()));
        }
        return total;
    }

    static int stringSequence(java.util.List<String> values) {
        int total = 4;
        int index = 0;
        while (index < values.size()) {
            total = Math.addExact(total, string(values.get(index)));
            index += 1;
        }
        return total;
    }

}

final class WireLease implements AutoCloseable {
    private final WireWriterPoolState owner;
    private final java.nio.ByteBuffer buffer;
    private final WireWriter writer;
    private boolean closed;

    WireLease(WireWriterPoolState owner, java.nio.ByteBuffer buffer) {
        this.owner = owner;
        this.buffer = buffer;
        this.writer = new WireWriter(buffer);
        reopen();
    }

    WireWriter writer() { return writer; }
    java.nio.ByteBuffer directBuffer() { return buffer; }
    int size() { return writer.size(); }
    int capacity() { return buffer.capacity(); }

    WireLease reopen() {
        buffer.clear();
        buffer.order(java.nio.ByteOrder.LITTLE_ENDIAN);
        closed = false;
        return this;
    }

    byte[] bytes() {
        java.nio.ByteBuffer source = buffer.duplicate();
        source.flip();
        byte[] bytes = new byte[source.remaining()];
        source.get(bytes);
        return bytes;
    }

    @Override
    public void close() {
        if (!closed) {
            closed = true;
            owner.release(this);
        }
    }
}

final class WireWriterPoolState {
    private static final int CACHE_SIZE = 4;
    private final java.util.ArrayDeque<WireLease> leases = new java.util.ArrayDeque<>(CACHE_SIZE);

    WireLease acquire(int capacity) {
        int required = Math.max(capacity, 1);
        int remaining = leases.size();
        while (remaining > 0) {
            WireLease candidate = leases.pollFirst();
            if (candidate.capacity() >= required) return candidate.reopen();
            leases.offerLast(candidate);
            remaining -= 1;
        }
        return new WireLease(this, java.nio.ByteBuffer.allocateDirect(required));
    }

    void release(WireLease lease) {
        if (leases.size() < CACHE_SIZE) {
            leases.addFirst(lease);
        }
    }
}

final class WireWriterPool {
    private static final ThreadLocal<WireWriterPoolState> STATE =
        ThreadLocal.withInitial(WireWriterPoolState::new);

    private WireWriterPool() {}

    static WireLease acquire(int capacity) {
        return STATE.get().acquire(capacity);
    }
}
