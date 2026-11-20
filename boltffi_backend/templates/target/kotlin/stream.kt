{%- if stream.async_delivery() %}
{{ stream.documentation() }}{% if let Some(receiver) = stream.receiver() %}fun {{ receiver }}.{{ stream.name() }}(){% else %}fun {{ stream.name() }}(){% endif %}: kotlinx.coroutines.flow.Flow<{{ stream.item() }}> = kotlinx.coroutines.flow.callbackFlow {
    val subscription = Native.{{ stream.subscribe() }}({% if stream.receiver().is_some() %}boltffiHandle(){% endif %})
    if (subscription == 0L) {
        close()
        return@callbackFlow
    }
    val context = BoltFfiStreamContext(
        scope = this,
        subscription = subscription,
        batchSize = 16L,
        popBatch = Native::{{ stream.pop_batch() }},
        poll = Native::{{ stream.poll() }},
        unsubscribe = Native::{{ stream.unsubscribe() }},
        free = Native::{{ stream.free() }},
        processItems = { bytes ->
{%- for statement in stream.item_setup() %}
            {{ statement }}
{%- endfor %}
            val items = {{ stream.items() }}
            items.forEach { item ->
                send(item)
            }
        },
        finish = { {% if stream.failure().is_some() %}close(it){% else %}close(){% endif %} }{% if let Some(failure) = stream.failure() %},
        takeFailure = { handle ->
            val bytes = Native.{{ failure.take_error() }}(handle)
            if (bytes == null || bytes.isEmpty()) null else {
                val {{ failure.reader() }} = WireReader(bytes)
                {{ failure.throwable() }}
            }
        }{% endif %}
    )
    context.start()
    awaitClose { context.requestTermination() }
}
{%- endif %}
{%- if let Some(subscription) = stream.batch_subscription() %}
{{ stream.documentation() }}{% if let Some(receiver) = stream.receiver() %}fun {{ receiver }}.{{ stream.name() }}(){% else %}fun {{ stream.name() }}(){% endif %}: {{ subscription }} =
    {{ subscription }}(
        handle = Native.{{ stream.subscribe() }}({% if stream.receiver().is_some() %}boltffiHandle(){% endif %}),
        popBatch = Native::{{ stream.pop_batch() }},
        wait = Native::{{ stream.wait() }},
        unsubscribe = Native::{{ stream.unsubscribe() }},
        free = Native::{{ stream.free() }}{% if let Some(failure) = stream.failure() %},
        takeFailure = { handle ->
            val bytes = Native.{{ failure.take_error() }}(handle)
            if (bytes == null || bytes.isEmpty()) null else {
                val {{ failure.reader() }} = WireReader(bytes)
                {{ failure.throwable() }}
            }
        }{% endif %}
    )

class {{ subscription }}(
    private val handle: Long,
    private val popBatch: (Long, Long) -> ByteArray?,
    private val wait: (Long, Int) -> Int,
    private val unsubscribe: (Long) -> Unit,
    private val free: (Long) -> Unit{% if stream.failure().is_some() %},
    private val takeFailure: (Long) -> Throwable?{% endif %}
) : AutoCloseable {
    private val closed = java.util.concurrent.atomic.AtomicBoolean(false)
{%- if stream.failure().is_some() %}
    @Volatile private var failure: Throwable? = null
{%- endif %}

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        if (handle == 0L) return
        free(handle)
    }

    fun popBatch(maxCount: Long = 16L): List<{{ stream.item() }}> {
        if (handle == 0L) return emptyList()
{%- if stream.failure().is_some() %}
        // closed before this pop, so nothing can arrive after it: an empty pop is final
        val ended = failure == null && wait(handle, 0) < 0
{%- endif %}
        val bytes = popBatch(handle, maxCount)
            ?: throw IllegalStateException("BoltFFI stream pop_batch returned null")
{%- if stream.failure().is_some() %}
        if (bytes.isEmpty()) {
            if (ended) failure = takeFailure(handle)
            failure?.let { throw it }
            return emptyList()
        }
{%- else %}
        if (bytes.isEmpty()) return emptyList()
{%- endif %}
{%- for statement in stream.item_setup() %}
        {{ statement }}
{%- endfor %}
        val items = {{ stream.items() }}
        return items
    }

    fun wait(timeout: Int): Int {
        if (handle == 0L) return -1
        return wait(handle, timeout)
    }

    fun unsubscribe() {
        if (handle == 0L) return
        unsubscribe(handle)
    }
}
{%- endif %}
{%- if let Some(cancellable) = stream.callback_cancellable() %}
{{ stream.documentation() }}{% if let Some(receiver) = stream.receiver() %}fun {{ receiver }}.{{ stream.name() }}({% if stream.failure().is_some() %}onError: (Throwable) -> Unit, {% endif %}callback: ({{ stream.item() }}) -> Unit){% else %}fun {{ stream.name() }}({% if stream.failure().is_some() %}onError: (Throwable) -> Unit, {% endif %}callback: ({{ stream.item() }}) -> Unit){% endif %}: {{ cancellable }} {
    val subscription = Native.{{ stream.subscribe() }}({% if stream.receiver().is_some() %}boltffiHandle(){% endif %})
    if (subscription == 0L) return {{ cancellable }} {}
    val context = BoltFfiStreamContext(
        scope = boltffiCallbackScope,
        subscription = subscription,
        batchSize = 16L,
        popBatch = Native::{{ stream.pop_batch() }},
        poll = Native::{{ stream.poll() }},
        unsubscribe = Native::{{ stream.unsubscribe() }},
        free = Native::{{ stream.free() }},
        processItems = { bytes ->
{%- for statement in stream.item_setup() %}
            {{ statement }}
{%- endfor %}
            val items = {{ stream.items() }}
            items.forEach { item ->
                callback(item)
            }
        },
        finish = {% if stream.failure().is_some() %}{ failure -> failure?.let(onError) }{% else %}{}{% endif %}{% if let Some(failure) = stream.failure() %},
        takeFailure = { handle ->
            val bytes = Native.{{ failure.take_error() }}(handle)
            if (bytes == null || bytes.isEmpty()) null else {
                val {{ failure.reader() }} = WireReader(bytes)
                {{ failure.throwable() }}
            }
        }{% endif %}
    )
    context.start()
    return {{ cancellable }} { context.requestTermination() }
}

class {{ cancellable }}(
    private val onCancel: () -> Unit = {}
) : AutoCloseable {
    private val cancelled = java.util.concurrent.atomic.AtomicBoolean(false)

    fun cancel() {
        if (!cancelled.compareAndSet(false, true)) return
        onCancel()
    }

    override fun close() {
        cancel()
    }
}
{%- endif %}
