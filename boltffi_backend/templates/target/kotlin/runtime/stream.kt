private const val BOLTFFI_STREAM_POLL_CLOSED: Byte = 1

internal class BoltFfiStreamContext(
    private val scope: kotlinx.coroutines.CoroutineScope,
    private val subscription: Long,
    private val batchSize: Long,
    private val popBatch: (Long, Long) -> ByteArray?,
    private val poll: (Long, Long) -> Unit,
    private val unsubscribe: (Long) -> Unit,
    private val free: (Long) -> Unit,
    private val processItems: suspend (ByteArray) -> Unit,
    private val finish: () -> Unit
) {
    private val lifecycle = java.util.concurrent.atomic.AtomicInteger(0)
    private val processing = java.util.concurrent.atomic.AtomicInteger(0)

    fun start() {
        registerPoll()
    }

    fun requestTermination() {
        if (lifecycle.compareAndSet(0, 1)) {
            unsubscribe(subscription)
            lifecycle.compareAndSet(1, 2)
        }
        finalizeIfIdle()
    }

    private fun registerPoll() {
        if (!lifecycle.compareAndSet(0, 0)) {
            finalizeIfIdle()
            return
        }
        scope.launch {
            val pollResult = kotlinx.coroutines.suspendCancellableCoroutine { continuation ->
                poll(subscription, boltffiContinuationMap.insert(continuation))
            }
            handlePoll(pollResult)
        }
    }

    private suspend fun handlePoll(pollResult: Byte) {
        val closed = pollResult == BOLTFFI_STREAM_POLL_CLOSED
        if (!processing.compareAndSet(0, 1)) {
            finalizeIfIdle()
            return
        }
        try {
            if (lifecycle.compareAndSet(0, 0)) {
                drain()
            }
        } finally {
            processing.compareAndSet(1, 0)
            finalizeIfIdle()
        }
        if (closed) {
            requestTermination()
            return
        }
        if (lifecycle.compareAndSet(0, 0)) {
            registerPoll()
        }
    }

    private suspend fun drain() {
        while (true) {
            val bytes = popBatch(subscription, batchSize)
                ?: throw IllegalStateException("BoltFFI stream pop_batch returned null")
            if (bytes.isEmpty()) return
            processItems(bytes)
        }
    }

    private fun finalizeIfIdle() {
        if (!processing.compareAndSet(0, 0)) return
        if (!lifecycle.compareAndSet(2, 3)) return
        free(subscription)
        finish()
    }
}
