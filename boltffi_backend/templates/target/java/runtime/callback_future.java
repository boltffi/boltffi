final class BoltFfiCallbackFutures {
    private static final java.util.concurrent.atomic.AtomicLong NEXT =
        new java.util.concurrent.atomic.AtomicLong(1L);
    private static final java.util.concurrent.ConcurrentHashMap<
        Long,
        java.util.concurrent.CompletableFuture<?>
    > VALUES = new java.util.concurrent.ConcurrentHashMap<>();

    private BoltFfiCallbackFutures() {}

    static long insert(java.util.concurrent.CompletableFuture<?> future) {
        long handle = NEXT.getAndIncrement();
        VALUES.put(handle, future);
        return handle;
    }

    @SuppressWarnings({"rawtypes", "unchecked"})
    static void success(long handle, Object value) {
        java.util.concurrent.CompletableFuture future = VALUES.remove(handle);
        if (future != null) future.complete(value);
    }

    static void failure(long handle, Throwable failure) {
        java.util.concurrent.CompletableFuture<?> future = VALUES.remove(handle);
        if (future != null) future.completeExceptionally(failure);
    }
}
