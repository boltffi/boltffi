{%- if record.empty() %}
object {{ record.name() }}{% if record.error() %} : Exception(){% endif %} {
{%- if record.encoded() %}
    internal fun wireSize(): Int = 0

    internal fun writeTo(writer: WireWriter) {
    }

    internal fun toByteArray(): ByteArray = ByteArray(0)

    internal fun fromReader(reader: WireReader): {{ record.name() }} {
        return {{ record.name() }}
    }

    internal fun fromByteArray(bytes: ByteArray): {{ record.name() }} {
        require(bytes.size == 0)
        return {{ record.name() }}
    }
{%- else %}
    internal fun toByteArray(): ByteArray = ByteArray(0)

    internal fun toDirectBuffer(): java.nio.ByteBuffer =
        java.nio.ByteBuffer.allocateDirect(0)

    internal fun fromByteArray(bytes: ByteArray): {{ record.name() }} {
        require(bytes.size == 0)
        return {{ record.name() }}
    }
{%- endif %}
{%- for initializer in record.initializers() %}

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
{%- for method in record.static_methods() %}

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
{%- else if record.encoded() %}
data class {{ record.name() }}(
{%- for field in record.fields() %}
    {% if record.error() && field.is_string_message() %}override {% endif %}val {{ field.name() }}: {{ field.ty() }}{% if let Some(default) = field.default() %} = {{ default }}{% endif %}{% if !loop.last %},{% endif %}
{%- endfor %}
){% if record.error() %} : Exception({% if let Some(message) = record.error_message() %}{{ message }}{% endif %}){% endif %} {
    internal fun wireSize(): Int {
{%- if let Some(wire_size) = record.wire_size() %}
        return {{ wire_size }}
{%- endif %}
    }

    internal fun writeTo(writer: WireWriter) {
{%- for field in record.fields() %}
        {{ field.write() }}
{%- endfor %}
    }

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

    companion object {
        internal fun fromReader(reader: WireReader): {{ record.name() }} {
            return {{ record.name() }}(
{%- for field in record.fields() %}
                {{ field.read() }}{% if !loop.last %},{% endif %}
{%- endfor %}
            )
        }

        internal fun fromByteArray(bytes: ByteArray): {{ record.name() }} {
            val reader = WireReader(bytes)
            return fromReader(reader)
        }
{%- for initializer in record.initializers() %}

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
{%- for method in record.static_methods() %}

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
{%- for method in record.instance_methods() %}

    {% if method.async_call().is_some() %}suspend {% endif %}fun {{ method.name() }}({% for parameter in method.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = method.returns() %}: {{ return_type }}{% endif %} {
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
{%- else %}
data class {{ record.name() }}(
{%- for field in record.fields() %}
    {% if record.error() && field.is_string_message() %}override {% endif %}val {{ field.name() }}: {{ field.ty() }}{% if let Some(default) = field.default() %} = {{ default }}{% endif %}{% if !loop.last %},{% endif %}
{%- endfor %}
){% if record.error() %} : Exception({% if let Some(message) = record.error_message() %}{{ message }}{% endif %}){% endif %} {
    internal fun toByteArray(): ByteArray {
        val buffer = java.nio.ByteBuffer
            .allocate(STRUCT_SIZE)
            .order(java.nio.ByteOrder.nativeOrder())
        writeTo(buffer, 0)
        return buffer.array()
    }

    internal fun toDirectBuffer(): java.nio.ByteBuffer {
        val buffer = java.nio.ByteBuffer
            .allocateDirect(STRUCT_SIZE)
            .order(java.nio.ByteOrder.nativeOrder())
        writeTo(buffer, 0)
        return buffer
    }

    internal fun writeTo(buffer: java.nio.ByteBuffer, offset: Int) {
{%- for field in record.fields() %}
        {{ field.write_from_base() }}
{%- endfor %}
    }

    companion object {
        internal const val STRUCT_SIZE: Int = {{ record.size() }}

        internal fun fromByteArray(bytes: ByteArray): {{ record.name() }} {
            require(bytes.size == STRUCT_SIZE)
            val buffer = java.nio.ByteBuffer
                .wrap(bytes)
                .order(java.nio.ByteOrder.nativeOrder())
            return fromBuffer(buffer, 0)
        }

        internal fun fromBuffer(buffer: java.nio.ByteBuffer, offset: Int): {{ record.name() }} {
            return {{ record.name() }}(
{%- for field in record.direct_fields() %}
                {{ field.read_from_base() }}{% if !loop.last %},{% endif %}
{%- endfor %}
            )
        }
{%- for initializer in record.initializers() %}

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
{%- for method in record.static_methods() %}

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
{%- for method in record.instance_methods() %}

    {% if method.async_call().is_some() %}suspend {% endif %}fun {{ method.name() }}({% for parameter in method.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = method.returns() %}: {{ return_type }}{% endif %} {
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
