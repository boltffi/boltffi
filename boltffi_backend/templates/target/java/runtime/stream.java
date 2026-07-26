@FunctionalInterface
interface BoltFfiStreamBatch<T> {
    java.util.List<T> read(long stream, long maxCount);
}

@FunctionalInterface
interface BoltFfiStreamWait {
    int waitForItems(long stream, int timeout);
}

final class BoltFfiStream {
    private static final byte CLOSED = 1;

    private BoltFfiStream() {}

    static <T> StreamSubscription<T> callback(
        long stream,
        long batchSize,
        BoltFfiStreamBatch<T> readBatch,
        BoltFfiFuturePoll poll,
        BoltFfiFutureLifecycle unsubscribe,
        BoltFfiFutureLifecycle free,
        java.util.function.Consumer<T> deliver
    ) {
        if (stream == 0L) return StreamSubscription.callback(() -> {});
        Context<T> context = new Context<>(
            stream,
            batchSize,
            readBatch,
            poll,
            unsubscribe,
            free,
            deliver
        );
        context.start();
        return StreamSubscription.callback(context::requestTermination);
    }

    static RuntimeException failure(Throwable failure) {
        if (failure instanceof RuntimeException) return (RuntimeException) failure;
        if (failure instanceof Error) throw (Error) failure;
        return new RuntimeException(failure);
    }

    private static final class Context<T> {
        private static final int ACTIVE = 0;
        private static final int TERMINATING = 1;
        private static final int RELEASABLE = 2;
        private static final int RELEASED = 3;

        private final long stream;
        private final long batchSize;
        private final BoltFfiStreamBatch<T> readBatch;
        private final BoltFfiFuturePoll poll;
        private final BoltFfiFutureLifecycle unsubscribe;
        private final BoltFfiFutureLifecycle free;
        private final java.util.function.Consumer<T> deliver;
        private final java.util.concurrent.atomic.AtomicInteger lifecycle =
            new java.util.concurrent.atomic.AtomicInteger(ACTIVE);
        private final java.util.concurrent.atomic.AtomicBoolean processing =
            new java.util.concurrent.atomic.AtomicBoolean(false);

        private Context(
            long stream,
            long batchSize,
            BoltFfiStreamBatch<T> readBatch,
            BoltFfiFuturePoll poll,
            BoltFfiFutureLifecycle unsubscribe,
            BoltFfiFutureLifecycle free,
            java.util.function.Consumer<T> deliver
        ) {
            this.stream = stream;
            this.batchSize = batchSize;
            this.readBatch = readBatch;
            this.poll = poll;
            this.unsubscribe = unsubscribe;
            this.free = free;
            this.deliver = deliver;
        }

        private void start() {
            drive();
        }

        private void requestTermination() {
            Throwable failure = null;
            if (lifecycle.compareAndSet(ACTIVE, TERMINATING)) {
                try {
                    unsubscribe.apply(stream);
                } catch (Throwable error) {
                    failure = error;
                } finally {
                    lifecycle.compareAndSet(TERMINATING, RELEASABLE);
                }
            }
            try {
                finalizeIfIdle();
            } catch (Throwable error) {
                if (failure == null) failure = error;
                else failure.addSuppressed(error);
            }
            if (failure != null) throw BoltFfiStream.failure(failure);
        }

        private void drive() {
            while (lifecycle.get() == ACTIVE) {
                java.util.concurrent.CompletableFuture<Byte> result =
                    BoltFfiAsync.poll(stream, poll);
                if (!result.isDone()) {
                    result.whenComplete(this::finishPoll);
                    return;
                }
                byte pollResult;
                try {
                    pollResult = result.join();
                } catch (Throwable failure) {
                    finishPoll(null, failure);
                    return;
                }
                if (!processPoll(pollResult)) return;
            }
            finalizeIfIdle();
        }

        private void finishPoll(Byte pollResult, Throwable failure) {
            if (failure != null) {
                try {
                    requestTermination();
                } catch (Throwable terminationFailure) {
                    failure.addSuppressed(terminationFailure);
                }
                throw BoltFfiStream.failure(failure);
            }
            if (processPoll(pollResult)) drive();
        }

        private boolean processPoll(byte pollResult) {
            if (!processing.compareAndSet(false, true)) return false;
            Throwable failure = null;
            try {
                if (lifecycle.get() == ACTIVE) drain();
            } catch (Throwable error) {
                failure = error;
            } finally {
                processing.set(false);
                try {
                    finalizeIfIdle();
                } catch (Throwable releaseFailure) {
                    if (failure == null) failure = releaseFailure;
                    else failure.addSuppressed(releaseFailure);
                }
            }
            if (failure != null) {
                try {
                    requestTermination();
                } catch (Throwable terminationFailure) {
                    failure.addSuppressed(terminationFailure);
                }
                throw BoltFfiStream.failure(failure);
            }
            if (pollResult == CLOSED) {
                requestTermination();
                return false;
            }
            return lifecycle.get() == ACTIVE;
        }

        private void drain() {
            while (lifecycle.get() == ACTIVE) {
                java.util.List<T> items = readBatch.read(stream, batchSize);
                if (items.isEmpty()) return;
                items.forEach(deliver);
            }
        }

        private void finalizeIfIdle() {
            if (processing.get()) return;
            if (!lifecycle.compareAndSet(RELEASABLE, RELEASED)) return;
            free.apply(stream);
        }
    }
}

final class StreamSubscription<T> implements AutoCloseable {
    private enum Mode {
        BATCH,
        CALLBACK
    }

    private final java.util.concurrent.atomic.AtomicBoolean closed =
        new java.util.concurrent.atomic.AtomicBoolean(false);
    private final java.util.concurrent.atomic.AtomicBoolean publisherAttached =
        new java.util.concurrent.atomic.AtomicBoolean(false);
    private final java.util.concurrent.atomic.AtomicReference<Thread> publisherWorker =
        new java.util.concurrent.atomic.AtomicReference<>();
    private final Mode mode;
    private final long stream;
    private final Runnable cancel;
    private final BoltFfiStreamBatch<T> readBatch;
    private final BoltFfiStreamWait waitForItems;

    private StreamSubscription(
        Mode mode,
        long stream,
        Runnable cancel,
        BoltFfiStreamBatch<T> readBatch,
        BoltFfiStreamWait waitForItems
    ) {
        this.mode = mode;
        this.stream = stream;
        this.cancel = cancel;
        this.readBatch = readBatch;
        this.waitForItems = waitForItems;
    }

    static <T> StreamSubscription<T> callback(Runnable cancel) {
        return new StreamSubscription<>(Mode.CALLBACK, 0L, cancel, null, null);
    }

    static <T> StreamSubscription<T> batch(
        long stream,
        BoltFfiStreamBatch<T> readBatch,
        BoltFfiStreamWait waitForItems,
        BoltFfiFutureLifecycle unsubscribe,
        BoltFfiFutureLifecycle free
    ) {
        return new StreamSubscription<>(
            Mode.BATCH,
            stream,
            () -> release(stream, unsubscribe, free),
            readBatch,
            waitForItems
        );
    }

    public java.util.List<T> popBatch(long maxCount) {
        requireBatch("popBatch");
        if (stream == 0L || closed.get()) return java.util.Collections.emptyList();
        return readBatch.read(stream, maxCount);
    }

    public int waitForItems(int timeout) {
        requireBatch("waitForItems");
        if (stream == 0L || closed.get()) return -1;
        return waitForItems.waitForItems(stream, timeout);
    }

    public void unsubscribe() {
        close();
    }

    public void cancel() {
        close();
    }

    @Override
    public void close() {
        if (!closed.compareAndSet(false, true)) return;
        try {
            cancel.run();
        } finally {
            Thread worker = publisherWorker.get();
            if (worker != null) java.util.concurrent.locks.LockSupport.unpark(worker);
        }
    }

{% if flow %}    public java.util.concurrent.Flow.Publisher<T> toPublisher() {
        requireBatch("toPublisher");
        return subscriber -> {
            if (stream == 0L) {
                subscriber.onSubscribe(new java.util.concurrent.Flow.Subscription() {
                    public void request(long count) {}
                    public void cancel() {}
                });
                subscriber.onComplete();
                return;
            }
            if (!publisherAttached.compareAndSet(false, true)) {
                subscriber.onSubscribe(new java.util.concurrent.Flow.Subscription() {
                    public void request(long count) {}
                    public void cancel() {}
                });
                subscriber.onError(new IllegalStateException("Stream publisher already attached"));
                return;
            }
            Publisher<T> publisher = new Publisher<>(this, subscriber);
            subscriber.onSubscribe(publisher);
            publisher.start();
        };
    }

    private static final class Publisher<T>
        implements java.util.concurrent.Flow.Subscription, Runnable {
        private static final int WAIT_TIMEOUT_MILLIS = 100;
        private final StreamSubscription<T> subscription;
        private final java.util.concurrent.Flow.Subscriber<? super T> subscriber;
        private final java.util.concurrent.atomic.AtomicBoolean done =
            new java.util.concurrent.atomic.AtomicBoolean(false);
        private final java.util.concurrent.atomic.AtomicLong requested =
            new java.util.concurrent.atomic.AtomicLong(0L);
        private final Thread worker;

        private Publisher(
            StreamSubscription<T> subscription,
            java.util.concurrent.Flow.Subscriber<? super T> subscriber
        ) {
            this.subscription = subscription;
            this.subscriber = subscriber;
            this.worker = new Thread(this, "boltffi-stream-publisher");
            this.worker.setDaemon(true);
            subscription.publisherWorker.set(this.worker);
        }

        private void start() {
            worker.start();
        }

        @Override
        public void request(long count) {
            if (done.get()) return;
            if (count <= 0L) {
                fail(new IllegalArgumentException("Flow subscription request must be positive"));
                return;
            }
            requested.getAndUpdate(current -> {
                if (current == Long.MAX_VALUE) return Long.MAX_VALUE;
                long next = current + count;
                return next < 0L ? Long.MAX_VALUE : next;
            });
            java.util.concurrent.locks.LockSupport.unpark(worker);
        }

        @Override
        public void cancel() {
            if (!done.compareAndSet(false, true)) return;
            subscription.close();
            java.util.concurrent.locks.LockSupport.unpark(worker);
        }

        @Override
        public void run() {
            try {
                while (!done.get() && !subscription.closed.get()) {
                    if (requested.get() == 0L) {
                        java.util.concurrent.locks.LockSupport.park(this);
                        continue;
                    }
                    long batchSize = Math.max(1L, Math.min(requested.get(), 256L));
                    java.util.List<T> items = subscription.readBatch.read(
                        subscription.stream,
                        batchSize
                    );
                    if (items.isEmpty()) {
                        int waitResult = subscription.waitForItems.waitForItems(
                            subscription.stream,
                            WAIT_TIMEOUT_MILLIS
                        );
                        if (waitResult < 0) complete();
                        continue;
                    }
                    int index = 0;
                    while (index < items.size() && !done.get()) {
                        T item = items.get(index);
                        subscriber.onNext(item);
                        requested.getAndUpdate(current ->
                            current == Long.MAX_VALUE ? Long.MAX_VALUE : Math.max(0L, current - 1L)
                        );
                        index += 1;
                    }
                }
                if (!done.get()) complete();
            } catch (Throwable failure) {
                fail(failure);
            }
        }

        private void complete() {
            if (!done.compareAndSet(false, true)) return;
            try {
                subscriber.onComplete();
            } finally {
                subscription.close();
            }
        }

        private void fail(Throwable failure) {
            if (!done.compareAndSet(false, true)) return;
            try {
                subscriber.onError(failure);
            } finally {
                subscription.close();
                java.util.concurrent.locks.LockSupport.unpark(worker);
            }
        }
    }

{% endif %}    private void requireBatch(String operation) {
        if (mode == Mode.BATCH) return;
        throw new IllegalStateException(
            operation + " is only available for batch stream subscriptions"
        );
    }

    private static void release(
        long stream,
        BoltFfiFutureLifecycle unsubscribe,
        BoltFfiFutureLifecycle free
    ) {
        if (stream == 0L) return;
        Throwable failure = null;
        try {
            unsubscribe.apply(stream);
        } catch (Throwable error) {
            failure = error;
        }
        try {
            free.apply(stream);
        } catch (Throwable error) {
            if (failure == null) failure = error;
            else failure.addSuppressed(error);
        }
        if (failure != null) throw BoltFfiStream.failure(failure);
    }
}

final class BoltFfiStreamBatches {
    private BoltFfiStreamBatches() {}

    static java.util.List<Boolean> booleans(byte[] bytes) {
        boolean[] values = DirectVectorCodec.readBooleanArray(bytes);
        return new java.util.AbstractList<Boolean>() {
            public Boolean get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static java.util.List<Byte> bytes(byte[] bytes) {
        return new java.util.AbstractList<Byte>() {
            public Byte get(int index) { return bytes[index]; }
            public int size() { return bytes.length; }
        };
    }

    static java.util.List<Short> shorts(byte[] bytes) {
        short[] values = DirectVectorCodec.readShortArray(bytes);
        return new java.util.AbstractList<Short>() {
            public Short get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static java.util.List<Integer> ints(byte[] bytes) {
        int[] values = DirectVectorCodec.readIntArray(bytes);
        return new java.util.AbstractList<Integer>() {
            public Integer get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static java.util.List<Long> longs(byte[] bytes) {
        long[] values = DirectVectorCodec.readLongArray(bytes);
        return new java.util.AbstractList<Long>() {
            public Long get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static java.util.List<Float> floats(byte[] bytes) {
        float[] values = DirectVectorCodec.readFloatArray(bytes);
        return new java.util.AbstractList<Float>() {
            public Float get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static java.util.List<Double> doubles(byte[] bytes) {
        double[] values = DirectVectorCodec.readDoubleArray(bytes);
        return new java.util.AbstractList<Double>() {
            public Double get(int index) { return values[index]; }
            public int size() { return values.length; }
        };
    }

    static <Source, Target> java.util.List<Target> map(
        java.util.List<Source> source,
        java.util.function.Function<Source, Target> transform
    ) {
        return new java.util.AbstractList<Target>() {
            public Target get(int index) { return transform.apply(source.get(index)); }
            public int size() { return source.size(); }
        };
    }
}
