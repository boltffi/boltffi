@FunctionalInterface
interface BoltFfiFutureStart {
    long start();
}

@FunctionalInterface
interface BoltFfiFuturePoll {
    void poll(long future, long continuation);
}

@FunctionalInterface
interface BoltFfiFutureComplete<T> {
    T complete(long future);
}

@FunctionalInterface
interface BoltFfiFutureLifecycle {
    void apply(long future);
}

final class BoltFfiAsync {
    private static final byte READY = 0;
    private static final java.util.concurrent.atomic.AtomicLong NEXT_CONTINUATION =
        new java.util.concurrent.atomic.AtomicLong(1L);
    private static final java.util.concurrent.ConcurrentHashMap<Long, PollSignal> CONTINUATIONS =
        new java.util.concurrent.ConcurrentHashMap<>();

    private BoltFfiAsync() {}

    static void resume(long handle, byte result) {
        PollSignal signal = CONTINUATIONS.get(handle);
        if (signal != null && signal.begin(result)) {
            signal.finish(result);
            CONTINUATIONS.remove(handle, signal);
        }
    }

    static <T> java.util.concurrent.CompletableFuture<T> call(
        BoltFfiFutureStart start,
        BoltFfiFuturePoll poll,
        BoltFfiFutureComplete<T> complete,
        BoltFfiFutureLifecycle cancel,
        BoltFfiFutureLifecycle free
    ) {
        long future;
        try {
            future = start.start();
        } catch (Throwable error) {
            java.util.concurrent.CompletableFuture<T> failed =
                new java.util.concurrent.CompletableFuture<>();
            failed.completeExceptionally(error);
            return failed;
        }
        Operation<T> operation = new Operation<>(future, poll, complete, cancel, free);
        operation.drive();
        return operation.result;
    }

    static RuntimeException failure(
        Throwable failure,
        java.util.function.Supplier<byte[]> panic
    ) {
        try {
            byte[] message = panic.get();
            if (message != null && message.length != 0) {
                return new RuntimeException(
                    new String(message, java.nio.charset.StandardCharsets.UTF_8),
                    failure
                );
            }
        } catch (Throwable ignored) {}
        if (failure instanceof RuntimeException) return (RuntimeException) failure;
        return new RuntimeException(failure);
    }

    static java.util.concurrent.CompletableFuture<Byte> poll(
        long owner,
        BoltFfiFuturePoll poll
    ) {
        PollSignal signal = new PollSignal(() -> {});
        long handle = NEXT_CONTINUATION.getAndIncrement();
        CONTINUATIONS.put(handle, signal);
        try {
            poll.poll(owner, handle);
        } catch (Throwable failure) {
            CONTINUATIONS.remove(handle, signal);
            signal.future.completeExceptionally(failure);
        }
        return signal.future;
    }

    private enum Phase {
        ACTIVE,
        POLLING,
        WAITING,
        CANCEL_REQUESTED,
        READY,
        OWNED
    }

    private enum Cancellation {
        REJECTED,
        DEFERRED,
        OWNED
    }

    private enum Pending {
        STOPPED,
        CONTINUE,
        CANCELLED
    }

    private enum Delivery {
        WAITING,
        PENDING,
        READY,
        CANCELLED
    }

    private static final class PollSignal {
        private final java.util.concurrent.atomic.AtomicReference<Delivery> delivery =
            new java.util.concurrent.atomic.AtomicReference<>(Delivery.WAITING);
        private final java.util.concurrent.CompletableFuture<Byte> future =
            new java.util.concurrent.CompletableFuture<>();
        private final Runnable ready;

        private PollSignal(Runnable ready) {
            this.ready = ready;
        }

        private boolean begin(byte result) {
            Delivery next = result == READY ? Delivery.READY : Delivery.PENDING;
            if (!delivery.compareAndSet(Delivery.WAITING, next)) return false;
            if (next == Delivery.READY) ready.run();
            return true;
        }

        private Cancellation cancel() {
            while (true) {
                Delivery current = delivery.get();
                if (current == Delivery.READY) return Cancellation.REJECTED;
                if (current == Delivery.PENDING) return Cancellation.DEFERRED;
                if (current == Delivery.CANCELLED) return Cancellation.OWNED;
                if (delivery.compareAndSet(Delivery.WAITING, Delivery.CANCELLED)) {
                    return Cancellation.OWNED;
                }
            }
        }

        private boolean readyStarted() {
            return delivery.get() == Delivery.READY;
        }

        private void finish(byte result) {
            future.complete(result);
        }
    }

    private static final class ActivePoll {
        private final long handle;
        private final PollSignal signal;

        private ActivePoll(long handle, PollSignal signal) {
            this.handle = handle;
            this.signal = signal;
        }
    }

    private static final class Result<T> extends java.util.concurrent.CompletableFuture<T> {
        private final Operation<T> operation;

        private Result(Operation<T> operation) {
            this.operation = operation;
        }

        @Override
        public boolean cancel(boolean mayInterruptIfRunning) {
            Cancellation cancellation = operation.requestCancellation();
            if (cancellation == Cancellation.REJECTED) return false;
            if (cancellation == Cancellation.OWNED) operation.cancelAndFree();
            return super.cancel(mayInterruptIfRunning);
        }

        private void finishCancelled() {
            operation.cancelAndFree();
            super.cancel(false);
        }
    }

    private static final class Operation<T> {
        private final long future;
        private final BoltFfiFuturePoll poll;
        private final BoltFfiFutureComplete<T> complete;
        private final BoltFfiFutureLifecycle cancel;
        private final BoltFfiFutureLifecycle free;
        private final java.util.concurrent.atomic.AtomicReference<Phase> phase =
            new java.util.concurrent.atomic.AtomicReference<>(Phase.ACTIVE);
        private final java.util.concurrent.atomic.AtomicReference<ActivePoll> active =
            new java.util.concurrent.atomic.AtomicReference<>();
        private final Result<T> result = new Result<>(this);

        private Operation(
            long future,
            BoltFfiFuturePoll poll,
            BoltFfiFutureComplete<T> complete,
            BoltFfiFutureLifecycle cancel,
            BoltFfiFutureLifecycle free
        ) {
            this.future = future;
            this.poll = poll;
            this.complete = complete;
            this.cancel = cancel;
            this.free = free;
        }

        private void drive() {
            while (true) {
                if (!phase.compareAndSet(Phase.ACTIVE, Phase.POLLING)) return;
                PollSignal signal = new PollSignal(this::markReady);
                long handle = NEXT_CONTINUATION.getAndIncrement();
                ActivePoll currentPoll = new ActivePoll(handle, signal);
                CONTINUATIONS.put(handle, signal);
                active.set(currentPoll);
                try {
                    poll.poll(future, handle);
                } catch (Throwable error) {
                    finishPollingFailure(currentPoll, error);
                    return;
                }
                if (!signal.future.isDone()) {
                    if (phase.compareAndSet(Phase.POLLING, Phase.WAITING)) {
                        signal.future.whenComplete(
                            (pollResult, error) -> finishAsyncPoll(currentPoll, pollResult, error)
                        );
                        return;
                    }
                    release(currentPoll);
                    if (finishDeferredCancellation()) return;
                    if (claimReady()) finishReady();
                    return;
                }
                byte pollResult;
                try {
                    pollResult = signal.future.join();
                } catch (Throwable error) {
                    finishPollingFailure(currentPoll, error);
                    return;
                }
                if (pollResult == READY) {
                    finishJoinedReady(currentPoll);
                    return;
                }
                if (finishDeferredCancellation(currentPoll)) return;
                Pending pending = finishJoinedPending(currentPoll);
                if (pending == Pending.CANCELLED) {
                    result.finishCancelled();
                    return;
                }
                if (pending != Pending.CONTINUE) return;
            }
        }

        private void finishAsyncPoll(ActivePoll currentPoll, Byte pollResult, Throwable error) {
            release(currentPoll);
            if (error != null) {
                if (phase.getAndSet(Phase.OWNED) != Phase.OWNED) cancelAndFree();
                result.completeExceptionally(error);
                return;
            }
            if (pollResult == READY) {
                if (claimReady()) {
                    finishReady();
                    return;
                }
                finishDeferredCancellation();
                return;
            }
            if (finishDeferredCancellation()) return;
            if (rearm()) drive();
        }

        private Cancellation requestCancellation() {
            while (true) {
                Phase current = phase.get();
                if (current == Phase.OWNED || current == Phase.READY) {
                    return Cancellation.REJECTED;
                }
                if (current == Phase.CANCEL_REQUESTED) return Cancellation.DEFERRED;
                if (current == Phase.ACTIVE) {
                    if (phase.compareAndSet(Phase.ACTIVE, Phase.OWNED)) {
                        return Cancellation.OWNED;
                    }
                    continue;
                }
                ActivePoll currentPoll = active.get();
                if (currentPoll == null) continue;
                if (current == Phase.WAITING) {
                    Cancellation delivery = currentPoll.signal.cancel();
                    if (delivery == Cancellation.REJECTED) return Cancellation.REJECTED;
                    if (delivery == Cancellation.DEFERRED) {
                        if (phase.compareAndSet(Phase.WAITING, Phase.CANCEL_REQUESTED)) {
                            return Cancellation.DEFERRED;
                        }
                        continue;
                    }
                    if (!phase.compareAndSet(Phase.WAITING, Phase.OWNED)) continue;
                    release(currentPoll);
                    CONTINUATIONS.remove(currentPoll.handle, currentPoll.signal);
                    return Cancellation.OWNED;
                }
                if (current == Phase.POLLING) {
                    if (currentPoll.signal.readyStarted()) return Cancellation.REJECTED;
                    if (!phase.compareAndSet(Phase.POLLING, Phase.CANCEL_REQUESTED)) continue;
                    ActivePoll updated = active.get();
                    if (updated != null && updated.signal.readyStarted()) {
                        if (phase.compareAndSet(Phase.CANCEL_REQUESTED, Phase.READY)) {
                            return Cancellation.REJECTED;
                        }
                        continue;
                    }
                    return Cancellation.DEFERRED;
                }
            }
        }

        private void finishJoinedReady(ActivePoll currentPoll) {
            if (claimReady()) {
                finishReady();
                release(currentPoll);
                return;
            }
            finishDeferredCancellation(currentPoll);
            release(currentPoll);
        }

        private Pending finishJoinedPending(ActivePoll currentPoll) {
            if (claimDeferredCancellation()) {
                release(currentPoll);
                return Pending.CANCELLED;
            }
            Pending pending = rearm()
                ? Pending.CONTINUE
                : claimDeferredCancellation()
                    ? Pending.CANCELLED
                    : Pending.STOPPED;
            release(currentPoll);
            return pending;
        }

        private void finishPollingFailure(ActivePoll currentPoll, Throwable error) {
            release(currentPoll);
            CONTINUATIONS.remove(currentPoll.handle, currentPoll.signal);
            if (phase.getAndSet(Phase.OWNED) != Phase.OWNED) cancelAndFree();
            result.completeExceptionally(error);
        }

        private void finishReady() {
            try {
                result.complete(complete.complete(future));
            } catch (Throwable error) {
                result.completeExceptionally(error);
            } finally {
                releaseFuture();
            }
        }

        private void cancelAndFree() {
            try {
                cancel.apply(future);
            } catch (Throwable ignored) {}
            releaseFuture();
        }

        private void releaseFuture() {
            try {
                free.apply(future);
            } catch (Throwable ignored) {}
        }

        private boolean finishDeferredCancellation() {
            if (!claimDeferredCancellation()) return false;
            result.finishCancelled();
            return true;
        }

        private boolean finishDeferredCancellation(ActivePoll currentPoll) {
            if (!claimDeferredCancellation()) return false;
            release(currentPoll);
            result.finishCancelled();
            return true;
        }

        private void markReady() {
            while (true) {
                Phase current = phase.get();
                if (current == Phase.CANCEL_REQUESTED
                    || current == Phase.READY
                    || current == Phase.OWNED) return;
                if (phase.compareAndSet(current, Phase.READY)) return;
            }
        }

        private boolean claimReady() {
            return phase.compareAndSet(Phase.READY, Phase.OWNED);
        }

        private boolean claimDeferredCancellation() {
            return phase.compareAndSet(Phase.CANCEL_REQUESTED, Phase.OWNED);
        }

        private boolean rearm() {
            while (true) {
                Phase current = phase.get();
                if (current != Phase.WAITING && current != Phase.POLLING) return false;
                if (phase.compareAndSet(current, Phase.ACTIVE)) return true;
            }
        }

        private void release(ActivePoll currentPoll) {
            active.compareAndSet(currentPoll, null);
        }
    }
}
