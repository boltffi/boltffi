@FunctionalInterface
interface DirectRecordWrite<T> {
    void write(T value, java.nio.ByteBuffer buffer, int offset);
}

@FunctionalInterface
interface DirectRecordRead<T> {
    T read(java.nio.ByteBuffer buffer, int offset);
}

final class DirectVectorCodec {
    private DirectVectorCodec() {}

    static boolean[] readBooleanArray(byte[] bytes) {
        boolean[] values = new boolean[bytes.length];
        int index = 0;
        while (index < bytes.length) {
            values[index] = bytes[index] != 0;
            index += 1;
        }
        return values;
    }

    static byte[] writeBooleanArray(boolean[] values) {
        byte[] bytes = new byte[values.length];
        int index = 0;
        while (index < values.length) {
            bytes[index] = (byte) (values[index] ? 1 : 0);
            index += 1;
        }
        return bytes;
    }

    static byte[] readByteArray(byte[] bytes) { return bytes; }
    static byte[] writeByteArray(byte[] values) { return values; }

    static short[] readShortArray(byte[] bytes) {
        short[] values = new short[exactLength(bytes, 2)];
        ordered(bytes).asShortBuffer().get(values);
        return values;
    }

    static byte[] writeShortArray(short[] values) {
        byte[] bytes = new byte[Math.multiplyExact(values.length, 2)];
        ordered(bytes).asShortBuffer().put(values);
        return bytes;
    }

    static int[] readIntArray(byte[] bytes) {
        int[] values = new int[exactLength(bytes, 4)];
        ordered(bytes).asIntBuffer().get(values);
        return values;
    }

    static byte[] writeIntArray(int[] values) {
        byte[] bytes = new byte[Math.multiplyExact(values.length, 4)];
        ordered(bytes).asIntBuffer().put(values);
        return bytes;
    }

    static long[] readLongArray(byte[] bytes) {
        long[] values = new long[exactLength(bytes, 8)];
        ordered(bytes).asLongBuffer().get(values);
        return values;
    }

    static byte[] writeLongArray(long[] values) {
        byte[] bytes = new byte[Math.multiplyExact(values.length, 8)];
        ordered(bytes).asLongBuffer().put(values);
        return bytes;
    }

    static float[] readFloatArray(byte[] bytes) {
        float[] values = new float[exactLength(bytes, 4)];
        ordered(bytes).asFloatBuffer().get(values);
        return values;
    }

    static byte[] writeFloatArray(float[] values) {
        byte[] bytes = new byte[Math.multiplyExact(values.length, 4)];
        ordered(bytes).asFloatBuffer().put(values);
        return bytes;
    }

    static double[] readDoubleArray(byte[] bytes) {
        double[] values = new double[exactLength(bytes, 8)];
        ordered(bytes).asDoubleBuffer().get(values);
        return values;
    }

    static byte[] writeDoubleArray(double[] values) {
        byte[] bytes = new byte[Math.multiplyExact(values.length, 8)];
        ordered(bytes).asDoubleBuffer().put(values);
        return bytes;
    }

    static <T> byte[] writeRecords(java.util.List<T> values, int size, DirectRecordWrite<T> write) {
        byte[] bytes = new byte[Math.multiplyExact(values.size(), size)];
        java.nio.ByteBuffer buffer = ordered(bytes);
        int index = 0;
        while (index < values.size()) {
            write.write(values.get(index), buffer, Math.multiplyExact(index, size));
            index += 1;
        }
        return bytes;
    }

    static <T> java.util.List<T> readRecords(byte[] bytes, int size, DirectRecordRead<T> read) {
        int count = exactLength(bytes, size);
        java.util.ArrayList<T> values = new java.util.ArrayList<>(count);
        java.nio.ByteBuffer buffer = ordered(bytes);
        int index = 0;
        while (index < count) {
            values.add(read.read(buffer, Math.multiplyExact(index, size)));
            index += 1;
        }
        return values;
    }

    private static java.nio.ByteBuffer ordered(byte[] bytes) {
        return java.nio.ByteBuffer.wrap(bytes).order(java.nio.ByteOrder.nativeOrder());
    }

    private static int exactLength(byte[] bytes, int width) {
        if (width <= 0 || bytes.length % width != 0) {
            throw new IllegalArgumentException("invalid direct vector byte size");
        }
        return bytes.length / width;
    }
}
