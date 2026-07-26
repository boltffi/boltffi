{%- if enumeration.c_style() %}
{%- if let Some(value_type) = enumeration.value_type() %}
{%- if enumeration.error() %}
sealed class {{ enumeration.name() }}(val value: {{ value_type }}) : Exception() {
{%- else %}
enum class {{ enumeration.name() }}(val value: {{ value_type }}) {
{%- endif %}
{%- for variant in enumeration.c_style_variants() %}
{%- if enumeration.error() %}
    object {{ variant.name() }} : {{ enumeration.name() }}({{ variant.value() }})
{%- else %}
    {{ variant.name() }}({{ variant.value() }}){% if !loop.last %},{% else %};{% endif %}
{%- endif %}
{%- endfor %}

    companion object {
        fun fromValue(value: {{ value_type }}): {{ enumeration.name() }} =
{%- if enumeration.error() %}
            when (value) {
{%- for variant in enumeration.c_style_variants() %}
                {{ variant.value() }} -> {{ variant.name() }}
{%- endfor %}
                else -> throw IllegalArgumentException("unknown {{ enumeration.name() }} value: $value")
            }
{%- else %}
            entries.first { it.value == value }
{%- endif %}
{%- for initializer in enumeration.initializers() %}

        fun {{ initializer.name() }}({% for parameter in initializer.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = initializer.returns() %}: {{ return_type }}{% endif %} {
{%- for statement in initializer.setup() %}
            {{ statement }}
{%- endfor %}
{%- if initializer.has_cleanup() %}
            try {
{%- for statement in initializer.call() %}
                {{ statement }}
{%- endfor %}
            } finally {
{%- for statement in initializer.cleanup() %}
                {{ statement }}
{%- endfor %}
            }
{%- else %}
{%- for statement in initializer.call() %}
            {{ statement }}
{%- endfor %}
{%- endif %}
        }
{%- endfor %}
{%- for method in enumeration.static_methods() %}

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
{%- for method in enumeration.instance_methods() %}

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
{%- else if enumeration.data() %}
sealed class {{ enumeration.name() }}{% if enumeration.error() %} : Exception(){% endif %} {
    internal abstract fun wireSize(): {{ enumeration.wire_size_type() }}

    internal abstract fun writeTo(writer: WireWriter)

    internal fun toByteArray(): ByteArray {
        val buffer = WireWriterPool.acquire(wireSize())
        val writer = buffer.writer
        try {
            writeTo(writer)
            return buffer.bytes()
        } finally {
            buffer.close()
        }
    }

{% for variant in enumeration.data_variants() %}
{%- if variant.unit() %}
    object {{ variant.name() }} : {{ enumeration.name() }}() {
        internal override fun wireSize(): {{ enumeration.wire_size_type() }} {
            return {{ variant.size() }}
        }

        internal override fun writeTo(writer: WireWriter) {
            {{ variant.tag_write() }}
        }
    }
{%- else %}
    data class {{ variant.name() }}(
{%- for field in variant.fields() %}
        val {{ field.name() }}: {{ field.ty() }}{% if !loop.last %},{% endif %}
{%- endfor %}
    ) : {{ enumeration.name() }}() {
        internal override fun wireSize(): {{ enumeration.wire_size_type() }} {
            return {{ variant.size() }}
        }

        internal override fun writeTo(writer: WireWriter) {
            {{ variant.tag_write() }}
{%- for field in variant.fields() %}
            {{ field.write() }}
{%- endfor %}
        }
    }
{%- endif %}
{%- endfor %}

    companion object {
        internal fun fromReader(reader: WireReader): {{ enumeration.name() }} {
            val tag = reader.readU32()
            return when (tag) {
{%- for variant in enumeration.data_variants() %}
                {{ variant.tag() }} -> {{ variant.read() }}
{%- endfor %}
                else -> {{ enumeration.unknown_tag() }}
            }
        }

        internal fun fromByteArray(bytes: ByteArray): {{ enumeration.name() }} {
            val reader = WireReader(bytes)
            return fromReader(reader)
        }
{%- for initializer in enumeration.initializers() %}

        fun {{ initializer.name() }}({% for parameter in initializer.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = initializer.returns() %}: {{ return_type }}{% endif %} {
{%- for statement in initializer.setup() %}
            {{ statement }}
{%- endfor %}
{%- if initializer.has_cleanup() %}
            try {
{%- for statement in initializer.call() %}
                {{ statement }}
{%- endfor %}
            } finally {
{%- for statement in initializer.cleanup() %}
                {{ statement }}
{%- endfor %}
            }
{%- else %}
{%- for statement in initializer.call() %}
            {{ statement }}
{%- endfor %}
{%- endif %}
        }
{%- endfor %}
{%- for method in enumeration.static_methods() %}

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
{%- for method in enumeration.instance_methods() %}

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
