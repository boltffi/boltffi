{{ function.documentation() }}{% if function.async_call().is_some() %}suspend {% endif %}fun {{ function.name() }}({% for parameter in function.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = function.returns() %}: {{ return_type }}{% endif %} {
{%- if let Some(async_call) = function.async_call() %}
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
{%- for statement in async_call.create_prepare() %}
                {{ statement }}
{%- endfor %}
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
{%- for statement in function.setup() %}
    {{ statement }}
{%- endfor %}
{%- if function.has_cleanup() %}
    try {
{%- for statement in function.call() %}
        {{ statement }}
{%- endfor %}
    } finally {
{%- for statement in function.cleanup() %}
        {{ statement }}
{%- endfor %}
    }
{%- else %}
{%- for statement in function.call() %}
    {{ statement }}
{%- endfor %}
{%- endif %}
{%- endif %}
}
