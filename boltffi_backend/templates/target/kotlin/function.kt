{%- macro exported_call(call, indent) -%}
{{ call.documentation().indented(indent) }}{{ indent }}{% if call.async_call().is_some() %}suspend {% endif %}fun {{ call.name() }}({% for parameter in call.parameters() %}{{ parameter.declaration() }}{% if !loop.last %}, {% endif %}{% endfor %}){% if let Some(return_type) = call.returns() %}: {{ return_type }}{% endif %} {
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
{%- endmacro -%}
{% call exported_call(function, "") %}{% endcall %}
