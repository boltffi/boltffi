class {{ class.name() }} internal constructor(internal val handle: Long) : AutoCloseable {
    private val __boltffi_closed = java.util.concurrent.atomic.AtomicBoolean(false)

    override fun close() {
        if (__boltffi_closed.compareAndSet(false, true)) {
            Native.{{ class.release() }}(handle)
        }
    }

    internal fun boltffiHandle(): Long {
        check(!__boltffi_closed.get()) { "{{ class.name() }} is closed" }
        return handle
    }
{%- for initializer in class.initializers() %}
{%- if initializer.constructor() %}

    constructor({% for parameter in initializer.call().parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}) : this({{ initializer.call().name() }}({% for parameter in initializer.call().parameters() %}{{ parameter.name() }}{% if !loop.last %}, {% endif %}{% endfor %}).handle)
{%- endif %}
{%- endfor %}
{%- if !class.initializers().is_empty() || !class.static_methods().is_empty() %}

    companion object {
{%- for initializer in class.initializers() %}
        fun {{ initializer.call().name() }}({% for parameter in initializer.call().parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = initializer.call().returns() %}: {{ return_type }}{% endif %} {
{%- for statement in initializer.call().setup() %}
            {{ statement }}
{%- endfor %}
{%- if initializer.call().has_cleanup() %}
            try {
{%- for statement in initializer.call().call() %}
                {{ statement }}
{%- endfor %}
            } finally {
{%- for statement in initializer.call().cleanup() %}
                {{ statement }}
{%- endfor %}
            }
{%- else %}
{%- for statement in initializer.call().call() %}
            {{ statement }}
{%- endfor %}
{%- endif %}
        }
{%- endfor %}
{%- for method in class.static_methods() %}
        {% if method.async_call().is_some() %}suspend {% endif %}fun {{ method.name() }}({% for parameter in method.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = method.returns() %}: {{ return_type }}{% endif %} {
{%- if let Some(async_call) = method.async_call() %}
{%- if async_call.returns_value() %}
            return boltffiCallAsync(
{%- else %}
            boltffiCallAsync(
{%- endif %}
                createFuture = {
{%- for statement in async_call.create_setup() %}
                    {{ statement }}
{%- endfor %}
{%- if async_call.has_create_cleanup() %}
                    try {
                        {{ async_call.create() }}
                    } finally {
{%- for statement in async_call.create_cleanup() %}
                        {{ statement }}
{%- endfor %}
                    }
{%- else %}
                    {{ async_call.create() }}
{%- endif %}
                },
                poll = { future, contHandle -> Native.{{ async_call.poll() }}(future, contHandle) },
                complete = { future ->
{%- for statement in async_call.complete_body() %}
                    {{ statement }}
{%- endfor %}
                },
                free = { future -> Native.{{ async_call.free() }}(future) },
                cancel = { future -> Native.{{ async_call.cancel() }}(future) },
            )
{%- else %}
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
{%- endif %}
        }
{%- endfor %}
    }
{%- endif %}
{%- for method in class.instance_methods() %}

    {% if method.async_call().is_some() %}suspend {% endif %}fun {{ method.name() }}({% for parameter in method.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = method.returns() %}: {{ return_type }}{% endif %} {
{%- if let Some(async_call) = method.async_call() %}
{%- if async_call.returns_value() %}
        return boltffiCallAsync(
{%- else %}
        boltffiCallAsync(
{%- endif %}
            createFuture = {
{%- for statement in async_call.create_setup() %}
                {{ statement }}
{%- endfor %}
{%- if async_call.has_create_cleanup() %}
                try {
                    {{ async_call.create() }}
                } finally {
{%- for statement in async_call.create_cleanup() %}
                    {{ statement }}
{%- endfor %}
                }
{%- else %}
                {{ async_call.create() }}
{%- endif %}
            },
            poll = { future, contHandle -> Native.{{ async_call.poll() }}(future, contHandle) },
            complete = { future ->
{%- for statement in async_call.complete_body() %}
                {{ statement }}
{%- endfor %}
            },
            free = { future -> Native.{{ async_call.free() }}(future) },
            cancel = { future -> Native.{{ async_call.cancel() }}(future) },
        )
{%- else %}
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
{%- endif %}
    }
{%- endfor %}
}
