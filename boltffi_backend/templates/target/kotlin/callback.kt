{% if callback.fun_interface() %}fun {% endif %}interface {{ callback.name() }} {
{%- for method in callback.methods() %}
    {% if method.asynchronous() %}suspend {% endif %}fun {{ method.name() }}({% for parameter in method.public_parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = method.public_return() %}: {{ return_type }}{% endif %}
{%- endfor %}
}

private object {{ callback.map_name() }} {
    private val map = java.util.concurrent.ConcurrentHashMap<Long, {{ callback.name() }}>()
    private val counter = java.util.concurrent.atomic.AtomicLong(1L)

    fun insert(value: {{ callback.name() }}): Long {
        val handle = counter.getAndAdd(2L)
        map[handle] = value
        return handle
    }

    fun get(handle: Long): {{ callback.name() }}? = map[handle]

    fun remove(handle: Long): {{ callback.name() }}? = map.remove(handle)

    fun clone(handle: Long): Long {
        val value = map[handle] ?: return 0L
        return insert(value)
    }
}

object {{ callback.callbacks_name() }} {
    @JvmStatic
    fun free(handle: Long) {
        {{ callback.map_name() }}.remove(handle)
    }

    @JvmStatic
    fun clone(handle: Long): Long {
        return {{ callback.map_name() }}.clone(handle)
    }
{%- for method in callback.methods() %}

    @JvmStatic
    fun {{ method.jvm_name() }}(handle: Long{% for parameter in method.jvm_parameters() %}, {{ parameter.name() }}: {{ parameter.ty() }}{% endfor %}){% if let Some(return_type) = method.jvm_return() %}: {{ return_type }}{% endif %} {
        val impl = {{ callback.map_name() }}.get(handle) ?: error("{{ callback.map_name() }}: invalid handle $handle")
{%- for statement in method.setup() %}
        {{ statement }}
{%- endfor %}
{%- if let Some(async_body) = method.async_body() %}
        boltffiLaunchCallback {
            try {
{%- for statement in async_body.statements() %}
                {{ statement }}
{%- endfor %}
            } catch (throwable: Throwable) {
                {{ async_body.failure() }}
            }
        }
{%- else %}
{%- for statement in method.call_return() %}
        {{ statement }}
{%- endfor %}
{%- endif %}
    }
{%- endfor %}
}
{%- if !callback.handle_methods().is_empty() %}

private class {{ callback.handle_name() }}(private val handle: Long) : {{ callback.name() }}, AutoCloseable {
    private val __boltffi_closed = java.util.concurrent.atomic.AtomicBoolean(false)

    override fun close() {
{%- if let Some(release) = callback.handle_release() %}
        if (__boltffi_closed.compareAndSet(false, true)) {
            Native.{{ release }}(handle)
        }
{%- endif %}
    }

    private fun requireOpen(): Long {
        check(!__boltffi_closed.get()) { "callback handle is closed" }
        return handle
    }

    fun rawHandle(): Long = requireOpen()
{%- for method in callback.handle_methods() %}

    override fun {{ method.name() }}({% for parameter in method.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = method.returns() %}: {{ return_type }}{% endif %} {
{%- for statement in method.setup() %}
        {{ statement }}
{%- endfor %}
{%- if method.has_cleanup() %}
        try {
{%- for statement in method.call() %}
            {{ statement }}
{%- endfor %}
        } finally {
{%- for statement in method.cleanup() %}
            {{ statement }}
{%- endfor %}
        }
{%- else %}
{%- for statement in method.call() %}
        {{ statement }}
{%- endfor %}
{%- endif %}
    }
{%- endfor %}
}
{%- endif %}

object {{ callback.bridge_name() }} {
    fun create(value: {{ callback.name() }}): Long {
{%- if !callback.handle_methods().is_empty() %}
        if (value is {{ callback.handle_name() }}) {
            return value.rawHandle()
        }
{%- endif %}
        return {{ callback.map_name() }}.insert(value)
    }
}
