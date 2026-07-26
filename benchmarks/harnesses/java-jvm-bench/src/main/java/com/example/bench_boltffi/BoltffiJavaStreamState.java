package com.example.bench_boltffi;

public final class BoltffiJavaStreamState implements AutoCloseable {
    private final EventBus bus = new EventBus();
    private final StreamSubscription<Integer> subscription = bus.subscribeValuesBatch();
    private int nextValue = 1;

    public int roundTrip() {
        int expected = nextValue++;
        bus.emitValue(expected);
        java.util.List<Integer> batch = subscription.popBatch(1L);
        if (batch.size() != 1) {
            throw new AssertionError("stream batch behavior mismatch");
        }
        int actual = batch.get(0);
        if (actual != expected) throw new AssertionError("stream batch behavior mismatch");
        return actual;
    }

    @Override
    public void close() {
        try {
            subscription.close();
        } finally {
            bus.close();
        }
    }
}
