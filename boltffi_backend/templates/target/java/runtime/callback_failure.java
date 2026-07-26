final class BoltFfiCallbackFailure {
    private BoltFfiCallbackFailure() {}

    static <T> java.util.concurrent.CompletableFuture<T> failed(Throwable failure) {
        java.util.concurrent.CompletableFuture<T> future =
            new java.util.concurrent.CompletableFuture<>();
        future.completeExceptionally(failure);
        return future;
    }

    static Throwable unwrap(Throwable failure) {
        Throwable current = failure;
        while (current instanceof java.util.concurrent.CompletionException
            || current instanceof java.util.concurrent.ExecutionException) {
            Throwable cause = current.getCause();
            if (cause == null) return current;
            current = cause;
        }
        return current;
    }
}
