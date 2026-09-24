{%- macro exported_call(call, indent) %}

{{ call.documentation().indented(indent) }}{{ indent }}{% if call.async_call().is_some() %}suspend {% endif %}fun {{ call.name() }}({% for parameter in call.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = call.returns() %}: {{ return_type }}{% endif %} {
{%- if let Some(async_call) = call.async_call() %}
{%- if async_call.returns_value() %}
{{ indent }}    return boltffiCallAsync(
{%- else %}
{{ indent }}    boltffiCallAsync(
{%- endif %}
{{ indent }}        createFuture = {
{%- for statement in async_call.create_setup() %}
{{ indent }}            {{ statement }}
{%- endfor %}
{%- if async_call.has_create_cleanup() %}
{{ indent }}            try {
{{ indent }}                {{ async_call.create() }}
{{ indent }}            } finally {
{%- for statement in async_call.create_cleanup() %}
{{ indent }}                {{ statement }}
{%- endfor %}
{{ indent }}            }
{%- else %}
{{ indent }}            {{ async_call.create() }}
{%- endif %}
{{ indent }}        },
{{ indent }}        poll = { future, contHandle -> Native.{{ async_call.poll() }}(future, contHandle) },
{{ indent }}        complete = { future ->
{%- for statement in async_call.complete_body() %}
{{ indent }}            {{ statement }}
{%- endfor %}
{{ indent }}        },
{{ indent }}        free = { future -> Native.{{ async_call.free() }}(future) },
{{ indent }}        cancel = { future -> Native.{{ async_call.cancel() }}(future) },
{{ indent }}    )
{%- else %}
{%- for statement in call.setup() %}
{{ indent }}    {{ statement }}
{%- endfor %}
{%- if call.has_cleanup() %}
{{ indent }}    try {
{%- for statement in call.call() %}
{{ indent }}        {{ statement }}
{%- endfor %}
{{ indent }}    } finally {
{%- for statement in call.cleanup() %}
{{ indent }}        {{ statement }}
{%- endfor %}
{{ indent }}    }
{%- else %}
{%- for statement in call.call() %}
{{ indent }}    {{ statement }}
{%- endfor %}
{%- endif %}
{%- endif %}
{{ indent }}}
{%- endmacro %}
{%- if record.empty() %}
{{ record.documentation() }}object {{ record.name() }}{% if record.error() %} : Exception(){% endif %} {
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
{%- for constant in constants %}

{{ constant }}
{%- endfor %}
{%- for initializer in record.initializers() %}{% call exported_call(initializer, "    ") %}{% endcall %}
{%- endfor %}
{%- for method in record.static_methods() %}{% call exported_call(method, "    ") %}{% endcall %}
{%- endfor %}
{%- for method in record.instance_methods() %}{% call exported_call(method, "    ") %}{% endcall %}
{%- endfor %}
}
{%- else if record.encoded() %}
{{ record.documentation() }}data class {{ record.name() }}(
{%- for field in record.fields() %}
{{ field.documentation().indented("    ") }}    {% if record.overrides_message(field.name()) %}override {% endif %}val {{ field.name() }}: {{ field.ty() }}{% if let Some(default) = field.default() %} = {{ default }}{% endif %}{% if !loop.last %},{% endif %}
{%- endfor %}
){% if record.error() %} : Exception({% if let Some(message) = record.error_message() %}{{ message }}{% endif %}){% endif %} {
{%- if let Some(wire_size) = record.wire_size() %}
    internal fun wireSize(): Int {
        return {{ wire_size }}
    }

    internal fun writeTo(writer: WireWriter) {
{%- for field in record.fields() %}
        {{ field.write() }}
{%- endfor %}
    }
{%- endif %}

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
{%- for constant in constants %}

{{ constant }}
{%- endfor %}
{%- for initializer in record.initializers() %}{% call exported_call(initializer, "        ") %}{% endcall %}
{%- endfor %}
{%- for method in record.static_methods() %}{% call exported_call(method, "        ") %}{% endcall %}
{%- endfor %}
    }
{%- for method in record.instance_methods() %}{% call exported_call(method, "    ") %}{% endcall %}
{%- endfor %}
}
{%- else %}
{{ record.documentation() }}data class {{ record.name() }}(
{%- for field in record.fields() %}
{{ field.documentation().indented("    ") }}    {% if record.overrides_message(field.name()) %}override {% endif %}val {{ field.name() }}: {{ field.ty() }}{% if let Some(default) = field.default() %} = {{ default }}{% endif %}{% if !loop.last %},{% endif %}
{%- endfor %}
){% if record.error() %} : Exception({% if let Some(message) = record.error_message() %}{{ message }}{% endif %}){% endif %} {
{%- if let Some(wire_size) = record.wire_size() %}
    internal fun wireSize(): Int {
        return {{ wire_size }}
    }

    internal fun writeTo(writer: WireWriter) {
{%- for field in record.fields() %}
        {{ field.write() }}
{%- endfor %}
{%- if let Some(padding) = record.trailing_padding() %}
        writer.pad({{ padding }})
{%- endif %}
    }
{%- endif %}
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

{%- if record.wire_size().is_some() %}
        internal fun fromReader(reader: WireReader): {{ record.name() }} {
            return {{ record.name() }}(
{%- for field in record.fields() %}
                {{ field.read() }}{% if !loop.last %},{% endif %}
{%- endfor %}
            ){% if let Some(padding) = record.trailing_padding() %}.also { reader.skip({{ padding }}) }{% endif %}
        }
{%- endif %}

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
{%- for constant in constants %}

{{ constant }}
{%- endfor %}
{%- for initializer in record.initializers() %}{% call exported_call(initializer, "        ") %}{% endcall %}
{%- endfor %}
{%- for method in record.static_methods() %}{% call exported_call(method, "        ") %}{% endcall %}
{%- endfor %}
    }
{%- for method in record.instance_methods() %}{% call exported_call(method, "    ") %}{% endcall %}
{%- endfor %}
}
{%- endif %}
